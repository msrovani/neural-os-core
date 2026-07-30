//! R3 — Rollout Routing Replay (Trinity MoE) — crate cortex.
//! Rotas e tokens na TensorArena (Tier 2).

use crate::arena::TensorArena;
use crate::trinity::{Expert, TrinityRouter, ROUTER_HIDDEN, ROUTER_MAX_EXPERTS};

#[derive(Clone, Copy)]
pub struct RouteTrace {
    pub embedding_addr: usize,
    pub logits_addr: usize,
    pub num_experts: u8,
    pub selected_expert: u8,
    pub old_log_prob: f32,
    pub token_ids_addr: usize,
    pub token_count: u16,
}

impl RouteTrace {
    pub fn embedding_slice<'a>(&self, _arena: &'a TensorArena) -> Option<&'a [f32]> {
        if self.embedding_addr == 0 {
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts(self.embedding_addr as *const f32, ROUTER_HIDDEN) })
    }

    pub fn logits_slice<'a>(&self, _arena: &'a TensorArena) -> Option<&'a [f32]> {
        if self.logits_addr == 0 || self.num_experts == 0 {
            return None;
        }
        Some(unsafe {
            core::slice::from_raw_parts(self.logits_addr as *const f32, self.num_experts as usize)
        })
    }
}

#[derive(Clone, Copy)]
pub struct TokenRoute {
    pub token_id: u16,
    pub step: u16,
    pub selected_expert: u8,
    pub old_log_prob: f32,
}

pub struct R3Session<'a> {
    pub arena: &'a mut TensorArena,
    pub trace: Option<RouteTrace>,
    pub response_len: usize,
}

impl<'a> R3Session<'a> {
    pub fn new(arena: &'a mut TensorArena) -> Self {
        R3Session {
            arena,
            trace: None,
            response_len: 0,
        }
    }

    pub fn reset_moe_cache(&mut self) {
        self.arena.reset_moe_cache();
        self.trace = None;
        self.response_len = 0;
    }
}

pub fn record_router_trace(
    arena: &mut TensorArena,
    embedding: &[f32; ROUTER_HIDDEN],
    logits: &[f32],
    selected: usize,
) -> Option<RouteTrace> {
    let emb = arena.alloc_slice::<f32>(ROUTER_HIDDEN)?;
    emb.copy_from_slice(embedding);
    let embedding_addr = emb.as_ptr() as usize;

    let logits_slot = arena.alloc_slice::<f32>(logits.len())?;
    logits_slot.copy_from_slice(logits);
    let logits_addr = logits_slot.as_ptr() as usize;

    let prob = logits.get(selected).copied().unwrap_or(1e-12);
    let old_log_prob = libm::logf(prob.max(1e-12));

    Some(RouteTrace {
        embedding_addr,
        logits_addr,
        num_experts: logits.len().min(ROUTER_MAX_EXPERTS) as u8,
        selected_expert: selected as u8,
        old_log_prob,
        token_ids_addr: 0,
        token_count: 0,
    })
}

pub fn record_token_id(arena: &mut TensorArena, token_id: u16, step: u16) -> Option<*mut TokenRoute> {
    let slot = arena.alloc_slice::<TokenRoute>(1)?;
    slot[0] = TokenRoute {
        token_id,
        step,
        selected_expert: 0,
        old_log_prob: 0.0,
    };
    Some(slot.as_mut_ptr())
}

pub fn generate_and_record_routes(
    router: &TrinityRouter,
    prompt: &str,
    arena: &mut TensorArena,
    dispatch: impl FnOnce(&Expert, &str) -> usize,
) -> (usize, RouteTrace) {
    let (expert, trace) = router.classify_intent_with_trace(prompt, arena);
    let response_len = dispatch(expert, prompt);
    (response_len, trace)
}

/// Deterministic Gaussian noise (Box-Muller) seeded from trace fields,
/// so the same trace always gets the same noise — prevents overfitting without
/// a PRNG dependency.
fn gaussian_noise(trace: &RouteTrace) -> f32 {
    // Mix trace fields into a seed via wrapping ops
    let seed = trace
        .embedding_addr
        .wrapping_mul(2654435761)
        .wrapping_add((trace.token_count as usize).wrapping_mul(2246822519))
        .wrapping_add((trace.selected_expert as usize).wrapping_mul(3266489917));
    // xorshift32
    let mut x = seed as u32;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    // Box-Muller: two uniforms → gaussian
    let u1 = ((x & 0x7FFF) as f32 + 1.0) / 32768.0; // (0, 1]
    let u2 = ((x >> 16) & 0x7FFF) as f32 / 32768.0; // [0, 1)
    let r = libm::sqrtf(-2.0 * libm::logf(u1));
    let theta = 6.2831853 * u2;
    r * libm::cosf(theta)
}

pub fn update_with_replay(
    _router: &TrinityRouter,
    trace: &RouteTrace,
    reward: f32,
    router_weights: &mut [i8],
    lr: f32,
    noise_scale: f32,
) -> f32 {
    let advantage = (reward - 0.5).clamp(-1.0, 1.0);
    let policy_ratio = libm::expf(trace.old_log_prob.clamp(-8.0, 0.0));
    let clipped = policy_ratio.clamp(0.8, 1.2);
    let mut grad_scale = advantage * clipped * lr;
    // Inject Gaussian noise to prevent overfitting (SRC replay)
    if noise_scale > 0.0 {
        grad_scale += gaussian_noise(trace) * noise_scale;
    }

    let selected = trace.selected_expert as usize;
    let num_exp = (trace.num_experts as usize).max(1);
    let mut loss = 0.0f32;
    let hidden = ROUTER_HIDDEN.min(router_weights.len() / num_exp.max(1));
    if hidden == 0 {
        for w in router_weights.iter_mut() {
            let update = if grad_scale > 0.05 {
                1
            } else if grad_scale < -0.05 {
                -1
            } else {
                0
            };
            *w = (*w as i32 + update).clamp(-1, 1) as i8;
        }
        return grad_scale * grad_scale;
    }
    for h in 0..hidden {
        let idx = h * num_exp + selected.min(num_exp - 1);
        if idx >= router_weights.len() {
            break;
        }
        let update = if grad_scale > 0.05 {
            1
        } else if grad_scale < -0.05 {
            -1
        } else {
            0
        };
        let w = &mut router_weights[idx];
        *w = (*w as i32 + update).clamp(-1, 1) as i8;
        loss += grad_scale * grad_scale;
    }
    loss
}
