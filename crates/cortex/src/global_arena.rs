//! Arena Tier 2 global — acesso serializado para Trinity R3.
//! Onda 6: pending_route Hermes→Cortex promovido do bin (zero perda).

use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use crate::arena::TensorArena;
use crate::r3::RouteTrace;

const TRACE_RING: usize = 64;
const TOKEN_RING: usize = 256;

static CORTEX_ARENA: Mutex<Option<TensorArena>> = Mutex::new(None);

/// Traces de intent (Copy) — válidos para update_with_replay via old_log_prob/selected.
static TRACE_BUF: Mutex<[Option<RouteTrace>; TRACE_RING]> = Mutex::new([None; TRACE_RING]);
static TRACE_HEAD: AtomicUsize = AtomicUsize::new(0);
static TRACE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Pending route Hermes → CortexAgent (elimina double routing).
static PENDING_EXPERT: Mutex<Option<&'static str>> = Mutex::new(None);
static PENDING_TRACE: Mutex<Option<RouteTrace>> = Mutex::new(None);

/// Contagem de tokens gravados na sessão atual (cache token-a-token).
static TOKEN_STEPS: AtomicUsize = AtomicUsize::new(0);

pub fn install_global_arena(arena: TensorArena) {
    *CORTEX_ARENA.lock() = Some(arena);
}

pub fn with_arena<R>(f: impl FnOnce(&mut TensorArena) -> R) -> Option<R> {
    let mut guard = CORTEX_ARENA.lock();
    guard.as_mut().map(f)
}

pub fn reset_moe_cache() {
    if let Some(arena) = CORTEX_ARENA.lock().as_mut() {
        arena.reset_moe_cache();
        k_nano::slog_cortex!(
            "R3",
            "info",
            "reset_moe_cache: arena liberada ({} MB capacity)",
            arena.capacity_bytes() / (1024 * 1024)
        );
    }
    TOKEN_STEPS.store(0, Ordering::SeqCst);
}

pub fn arena_stats() -> (usize, usize) {
    if let Some(arena) = CORTEX_ARENA.lock().as_ref() {
        (arena.used_bytes(), arena.capacity_bytes())
    } else {
        (0, 0)
    }
}

/// Hermes classifica uma vez e deixa a rota pendente para generate_via_model.
pub fn set_pending_route(expert_name: &'static str, trace: Option<RouteTrace>) {
    *PENDING_EXPERT.lock() = Some(expert_name);
    *PENDING_TRACE.lock() = trace;
    if let Some(t) = trace {
        push_route_trace(t);
    }
}

/// CortexAgent / generate_via_model consome a rota (one-shot).
pub fn take_pending_route() -> Option<(&'static str, Option<RouteTrace>)> {
    let name = PENDING_EXPERT.lock().take()?;
    let trace = PENDING_TRACE.lock().take();
    Some((name, trace))
}

pub fn push_route_trace(trace: RouteTrace) {
    let idx = TRACE_HEAD.fetch_add(1, Ordering::SeqCst) % TRACE_RING;
    TRACE_BUF.lock()[idx] = Some(trace);
    let c = TRACE_COUNT.load(Ordering::SeqCst);
    if c < TRACE_RING {
        TRACE_COUNT.store(c + 1, Ordering::SeqCst);
    }
}

pub fn route_trace_count() -> usize {
    TRACE_COUNT.load(Ordering::SeqCst).min(TRACE_RING)
}

/// Snapshot Copy dos traces para replay (não depende de ponteiros da arena).
pub fn snapshot_route_traces(out: &mut [RouteTrace]) -> usize {
    let count = route_trace_count();
    let buf = TRACE_BUF.lock();
    let head = TRACE_HEAD.load(Ordering::SeqCst);
    let start = if count < TRACE_RING {
        0
    } else {
        head % TRACE_RING
    };
    let mut n = 0;
    for i in 0..count.min(out.len()) {
        let idx = (start + i) % TRACE_RING;
        if let Some(t) = buf[idx] {
            out[n] = t;
            n += 1;
        }
    }
    n
}

pub fn clear_route_traces() {
    *TRACE_BUF.lock() = [None; TRACE_RING];
    TRACE_HEAD.store(0, Ordering::SeqCst);
    TRACE_COUNT.store(0, Ordering::SeqCst);
}

pub fn record_token_step(token_id: u16) {
    let step = TOKEN_STEPS.fetch_add(1, Ordering::SeqCst);
    if step >= TOKEN_RING {
        return;
    }
    let _ = with_arena(|arena| {
        crate::r3::record_token_id(arena, token_id, step as u16);
    });
}

pub fn token_steps() -> usize {
    TOKEN_STEPS.load(Ordering::SeqCst)
}

/// HUD/mesh seam — delega ao r3 sem duplicar estado.
pub fn trained_router_changed() -> bool {
    crate::r3::trained_router_changed()
}

pub fn router_delta_vs_seed(seed: &[i8], trained: &[i8]) -> alloc::vec::Vec<u8> {
    crate::r3::router_delta_vs_seed(seed, trained)
}
