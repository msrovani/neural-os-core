//! InferQueue Full D+B+C (ADR-0057 WS-H) — Falcon3 off BSP.
//!
//! Contrato:
//! - BSP/Hermes/CortexAgent só **enfileiram** InferJob (nunca `generate_*` no tick).
//! - `poll_slice` (InferWorker BSP ou AP idle) roda **1 slice** (prefill ou 1 token).
//! - Stream: `LLM_STREAM` MessageDelta; fim: `LLM_RESPONSE` (+ healing topic).
//! - 1 job in-flight; fila profundidade 8; cancel no próximo yield.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use event_bus::{CapabilityToken, Event};
use spin::Mutex;

use crate::cortex::{
    infer_guard_begin, infer_guard_end, infer_in_flight, KvCache,
    CURRENT_MODEL, CURRENT_STREAMING_MODEL, global_kv_cache_take, global_kv_cache_store,
    NO_MODEL_MSG, TOPIC_LLM_RESPONSE,
};
use crate::tensor::Tensor;

/// Tópico stream (mesmo wire que hermes::stream_packet — cortex sem dep hermes).
pub const TOPIC_LLM_STREAM: &str = "LLM_STREAM";

const QUEUE_CAP: usize = 8;
const MAX_REPLY_TOPIC: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InferMode {
    Plain = 0,
    Healing = 1,
}

#[derive(Clone, Debug)]
pub struct InferJob {
    pub id: u64,
    pub prompt: String,
    pub mode: InferMode,
    pub reply_topic: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitErr {
    Full,
    EmptyPrompt,
}

struct QueueSlot {
    occupied: AtomicBool,
    id: AtomicU64,
    mode: AtomicU8,
    cancel: AtomicBool,
    prompt: Mutex<Option<String>>,
    reply_topic: Mutex<Option<String>>,
}

use core::sync::atomic::AtomicU8;

impl QueueSlot {
    const fn new() -> Self {
        Self {
            occupied: AtomicBool::new(false),
            id: AtomicU64::new(0),
            mode: AtomicU8::new(0),
            cancel: AtomicBool::new(false),
            prompt: Mutex::new(None),
            reply_topic: Mutex::new(None),
        }
    }
}

struct SyncSlots(UnsafeCell<[QueueSlot; QUEUE_CAP]>);
unsafe impl Sync for SyncSlots {}

static SLOTS: SyncSlots = SyncSlots(UnsafeCell::new([
    QueueSlot::new(),
    QueueSlot::new(),
    QueueSlot::new(),
    QueueSlot::new(),
    QueueSlot::new(),
    QueueSlot::new(),
    QueueSlot::new(),
    QueueSlot::new(),
]));
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVE_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_CANCEL: AtomicBool = AtomicBool::new(false);
static SLICE_BUSY: AtomicBool = AtomicBool::new(false);
static PENDING_COUNT: AtomicU32 = AtomicU32::new(0);

/// Fase da state machine de generate fatiado.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    NeedPrefill,
    Decoding,
    CoarseFallback,
    Finishing,
}

struct ActiveState {
    phase: Phase,
    job_id: u64,
    mode: InferMode,
    reply_topic: String,
    prompt: String,
    tokens: Vec<u32>,
    prompt_len: usize,
    step: usize,
    max_gen: usize,
    max_seq: usize,
    use_bpe: bool,
    is_greeting: bool,
    eos: u32,
    eot: u32,
    recent_u16: Vec<u16>,
    cache: Option<KvCache>,
    last_logits: Option<Tensor>,
    last_hidden: Option<Tensor>,
    acc_text: String,
    /// Texto ainda não enviado ao TTS parcial (acúmulo de deltas).
    tts_buf: String,
    /// Fallback: modelo não-Transformer — roda generate inteiro numa slice.
    coarse: bool,
}

static ACTIVE: Mutex<Option<ActiveState>> = Mutex::new(None);

fn slots() -> &'static [QueueSlot; QUEUE_CAP] {
    unsafe { &*SLOTS.0.get() }
}

fn publish_bytes(topic: &str, payload: Vec<u8>) {
    let _ = k_nano::EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(topic),
        payload,
        token: CapabilityToken::Legacy(1),
    });
}

fn emit_stream_raw(line: &str) {
    publish_bytes(TOPIC_LLM_STREAM, line.as_bytes().to_vec());
}

fn emit_msg_start() {
    emit_stream_raw("MSG_START|\n");
}

fn emit_msg_delta(content: &str) {
    if content.is_empty() {
        return;
    }
    emit_stream_raw(&alloc::format!("MSG_DELTA|{}\n", content));
}

fn emit_stop() {
    emit_stream_raw("STOP|\n");
}

fn emit_reply(topic: &str, text: &str) {
    publish_bytes(topic, text.as_bytes().to_vec());
}

/// Enfileira job. Acorda APs via monitor flag.
pub fn submit(prompt: String, mode: InferMode, reply_topic: &str) -> Result<u64, SubmitErr> {
    if prompt.is_empty() {
        return Err(SubmitErr::EmptyPrompt);
    }
    let topic = if reply_topic.is_empty() {
        String::from(TOPIC_LLM_RESPONSE)
    } else {
        let mut t = String::from(reply_topic);
        if t.len() > MAX_REPLY_TOPIC {
            t.truncate(MAX_REPLY_TOPIC);
        }
        t
    };
    let t = TAIL.load(Ordering::Relaxed);
    let h = HEAD.load(Ordering::Acquire);
    if t.wrapping_sub(h) >= QUEUE_CAP {
        return Err(SubmitErr::Full);
    }
    let idx = t % QUEUE_CAP;
    let slot = &slots()[idx];
    if slot.occupied.load(Ordering::Acquire) {
        return Err(SubmitErr::Full);
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    *slot.prompt.lock() = Some(prompt);
    *slot.reply_topic.lock() = Some(topic);
    slot.mode.store(mode as u8, Ordering::Release);
    slot.cancel.store(false, Ordering::Release);
    slot.id.store(id, Ordering::Release);
    slot.occupied.store(true, Ordering::Release);
    TAIL.store(t + 1, Ordering::Release);
    PENDING_COUNT.fetch_add(1, Ordering::Release);
    k_nano::smp::ap_work::notify_idle_wake();
    unsafe {
        k_nano::smp::wake_aps();
    }
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "submit id={} mode={} pending={}",
        id,
        mode as u8,
        PENDING_COUNT.load(Ordering::Relaxed)
    );
    Ok(id)
}

pub fn cancel(id: u64) -> bool {
    if ACTIVE_ID.load(Ordering::Acquire) == id {
        ACTIVE_CANCEL.store(true, Ordering::Release);
        return true;
    }
    for i in 0..QUEUE_CAP {
        let slot = &slots()[i];
        if slot.occupied.load(Ordering::Acquire) && slot.id.load(Ordering::Acquire) == id {
            slot.cancel.store(true, Ordering::Release);
            return true;
        }
    }
    false
}

/// Cancela o job ativo (barge-in / force_wake_open).
pub fn cancel_active() {
    let id = ACTIVE_ID.load(Ordering::Acquire);
    if id != 0 {
        ACTIVE_CANCEL.store(true, Ordering::Release);
        let _ = cancel(id);
    }
    // Também cancela o head da fila se ainda não claimed.
    let h = HEAD.load(Ordering::Acquire);
    let t = TAIL.load(Ordering::Acquire);
    if h < t {
        let slot = &slots()[h % QUEUE_CAP];
        if slot.occupied.load(Ordering::Acquire) {
            slot.cancel.store(true, Ordering::Release);
        }
    }
}

pub fn queue_pending() -> u32 {
    PENDING_COUNT.load(Ordering::Relaxed)
}

pub fn active_job_id() -> u64 {
    ACTIVE_ID.load(Ordering::Acquire)
}

pub fn has_work() -> bool {
    ACTIVE.lock().is_some() || PENDING_COUNT.load(Ordering::Relaxed) > 0
}

fn try_claim_into_active() -> bool {
    if ACTIVE.lock().is_some() {
        return false;
    }
    loop {
        let h = HEAD.load(Ordering::Relaxed);
        let t = TAIL.load(Ordering::Acquire);
        if h >= t {
            return false;
        }
        if HEAD
            .compare_exchange_weak(h, h + 1, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            continue;
        }
        let slot = &slots()[h % QUEUE_CAP];
        if !slot.occupied.swap(false, Ordering::AcqRel) {
            continue;
        }
        PENDING_COUNT.fetch_sub(1, Ordering::AcqRel);
        let id = slot.id.load(Ordering::Acquire);
        let cancelled = slot.cancel.load(Ordering::Acquire);
        let mode = match slot.mode.load(Ordering::Acquire) {
            1 => InferMode::Healing,
            _ => InferMode::Plain,
        };
        let prompt = slot.prompt.lock().take().unwrap_or_default();
        let reply_topic = slot
            .reply_topic
            .lock()
            .take()
            .unwrap_or_else(|| String::from(TOPIC_LLM_RESPONSE));
        if cancelled || prompt.is_empty() {
            emit_reply(&reply_topic, "[cancelled]");
            continue;
        }
        ACTIVE_ID.store(id, Ordering::Release);
        ACTIVE_CANCEL.store(false, Ordering::Release);
        let coarse = CURRENT_STREAMING_MODEL.lock().is_some();
        *ACTIVE.lock() = Some(ActiveState {
            phase: if coarse {
                Phase::CoarseFallback
            } else {
                Phase::NeedPrefill
            },
            job_id: id,
            mode,
            reply_topic,
            prompt,
            tokens: Vec::new(),
            prompt_len: 0,
            step: 0,
            max_gen: 0,
            max_seq: 64,
            use_bpe: crate::bpe::is_loaded(),
            is_greeting: false,
            eos: 0,
            eot: 0,
            recent_u16: Vec::new(),
            cache: None,
            last_logits: None,
            last_hidden: None,
            acc_text: String::new(),
            tts_buf: String::new(),
            coarse,
        });
        infer_guard_begin();
        emit_msg_start();
        k_nano::slog_cortex!("InferQ", "ok", "claim id={} coarse={}", id, coarse as u8);
        return true;
    }
}

fn finish_job(st: &mut ActiveState, text: &str) {
    let out = if text.is_empty() {
        if st.acc_text.is_empty() {
            String::from(NO_MODEL_MSG)
        } else {
            st.acc_text.clone()
        }
    } else {
        String::from(text)
    };
    // Flush TTS residual as final delta already in acc; reply once.
    emit_reply(&st.reply_topic, &out);
    emit_stop();
    if let Some(cache) = st.cache.take() {
        global_kv_cache_store(cache);
    }
    ACTIVE_ID.store(0, Ordering::Release);
    ACTIVE_CANCEL.store(false, Ordering::Release);
    infer_guard_end();
    st.phase = Phase::Idle;
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "done id={} len={} in_flight={}",
        st.job_id,
        out.len(),
        infer_in_flight() as u8
    );
}

fn sentence_boundary(buf: &str) -> Option<usize> {
    let b = buf.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'.' | b'!' | b'?' | b';' => {
                if i + 1 < b.len() && (b[i + 1] == b' ' || b[i + 1] == b'\n') {
                    return Some(i + 1);
                }
                if i + 1 == b.len() {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn push_delta(st: &mut ActiveState, piece: &str) {
    if piece.is_empty() {
        return;
    }
    st.acc_text.push_str(piece);
    st.tts_buf.push_str(piece);
    emit_msg_delta(piece);
    // TTS parcial: publicar frase fechada em HERMES_RESPONSE path via tópico dedicado.
    while let Some(end) = sentence_boundary(&st.tts_buf) {
        let sentence: String = st.tts_buf.chars().take(end).collect();
        let rest: String = st.tts_buf.chars().skip(end).collect();
        st.tts_buf = rest;
        let trimmed = sentence.trim();
        if !trimmed.is_empty() {
            publish_bytes(
                "INFER_TTS_PARTIAL",
                alloc::format!("[JARBAS] {}", trimmed).into_bytes(),
            );
        }
    }
}

fn run_prefill(st: &mut ActiveState) {
    if ACTIVE_CANCEL.load(Ordering::Acquire) {
        finish_job(st, "[cancelled]");
        return;
    }
    let guard = CURRENT_MODEL.lock();
    let Some(model_box) = guard.as_ref() else {
        drop(guard);
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(model) = model_box.as_transformer() else {
        // Não-Transformer: coarse numa slice.
        st.coarse = true;
        st.phase = Phase::CoarseFallback;
        drop(guard);
        return;
    };

    st.use_bpe = crate::bpe::is_loaded();
    st.eos = if st.use_bpe {
        crate::bpe::eos_id() as u32
    } else {
        crate::cortex::EOS as u32
    };
    st.eot = if st.use_bpe {
        crate::bpe::eot_id() as u32
    } else {
        crate::cortex::EOS as u32
    };
    st.is_greeting = crate::bpe::prompt_is_greeting(&st.prompt);

    let max_seq = if model.hidden >= 2048 {
        model.max_seq.min(512)
    } else {
        model.max_seq.min(64)
    };
    st.max_seq = max_seq;

    let mut tokens: Vec<u32> = if st.use_bpe {
        crate::bpe::encode(&st.prompt)
    } else {
        crate::cortex::Tokenizer::encode(&st.prompt)
            .into_iter()
            .map(|t| t as u32)
            .collect()
    };
    let vs = model.vocab_size;
    tokens.retain(|&t| t < vs);
    if tokens.is_empty() {
        tokens.push(if st.use_bpe {
            crate::bpe::bos_id().min(vs.saturating_sub(1))
        } else {
            crate::cortex::BOS as u32
        });
    }
    if model.hidden >= 2048 && tokens.len() > 1 {
        tokens = crate::cortex::slim_prompt_tokens_for_heavy(&tokens, st.use_bpe);
    }
    st.prompt_len = tokens.len();
    st.max_gen = if model.hidden >= 2048 {
        if st.use_bpe {
            if st.is_greeting {
                8
            } else {
                6
            }
        } else {
            4
        }
    } else {
        max_seq.saturating_sub(st.prompt_len).min(16)
    };

    let kv_dim = model.kv_dim;
    let k_dim = if model.layers.is_empty() {
        kv_dim
    } else {
        model.layers[0].k.shape.1
    };
    let mut cache = global_kv_cache_take().unwrap_or_else(|| KvCache::new(model.layers.len(), k_dim, kv_dim));
    if cache.k.len() != model.layers.len() || cache.k_dim() != k_dim {
        cache = KvCache::new(model.layers.len(), k_dim, kv_dim);
    } else {
        cache.len = 0;
        for layer in cache.k.iter_mut() {
            layer.clear();
        }
        for layer in cache.v.iter_mut() {
            layer.clear();
        }
    }

    let t0 = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let (last_hidden, last_logits) = model.forward_with_kv(&tokens, &mut cache);
    let t1 = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "prefill id={} tokens={} ticks={}",
        st.job_id,
        tokens.len(),
        t1 - t0
    );

    st.recent_u16.clear();
    if !st.is_greeting {
        if let Some(&last) = tokens.last() {
            st.recent_u16.push(last as u16);
        }
    }
    st.tokens = tokens;
    st.cache = Some(cache);
    st.last_hidden = Some(last_hidden);
    st.last_logits = Some(last_logits);
    st.step = 0;
    st.phase = Phase::Decoding;
}

fn run_decode_one(st: &mut ActiveState) {
    if ACTIVE_CANCEL.load(Ordering::Acquire) {
        let acc = st.acc_text.clone();
        finish_job(st, if acc.is_empty() { "[cancelled]" } else { &acc });
        return;
    }
    if st.step >= st.max_gen {
        let acc = st.acc_text.clone();
        finish_job(st, &acc);
        return;
    }

    let guard = CURRENT_MODEL.lock();
    let Some(model_box) = guard.as_ref() else {
        drop(guard);
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(model) = model_box.as_transformer() else {
        drop(guard);
        finish_job(st, NO_MODEL_MSG);
        return;
    };

    let Some(ref mut cache) = st.cache else {
        drop(guard);
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(ref mut last_logits) = st.last_logits else {
        drop(guard);
        finish_job(st, NO_MODEL_MSG);
        return;
    };

    if st.tokens.len() >= st.max_seq {
        let acc = st.acc_text.clone();
        drop(guard);
        finish_job(st, &acc);
        return;
    }

    let next = if st.use_bpe {
        crate::cortex::argmax_row_hf_vocab(last_logits, 0, &st.recent_u16)
    } else {
        crate::cortex::argmax_row_char_vocab(last_logits, 0, st.recent_u16.last().copied())
    };
    let next_u16 = next as u16;

    if next == st.eos || next == st.eot {
        let acc = st.acc_text.clone();
        drop(guard);
        finish_job(st, &acc);
        return;
    }

    st.tokens.push(next);
    st.recent_u16.push(next_u16);
    if st.recent_u16.len() > 4 {
        st.recent_u16.remove(0);
    }
    st.step += 1;

    let piece = if st.use_bpe {
        crate::bpe::decode(&[next])
    } else {
        crate::cortex::Tokenizer::decode(&[next_u16])
    };
    // Emit delta before next forward (UI respira).
    // Model lock still held — emit is EventBus only.
    // Actually we should drop model lock before heavy TTS publish — EventBus is fine under lock briefly.
    let piece_clone = piece.clone();

    if st.step < st.max_gen && st.tokens.len() < st.max_seq {
        let (new_hidden, new_logits) = model.forward_with_kv(&[next], cache);
        st.last_hidden = Some(new_hidden);
        *last_logits = new_logits;
    }
    drop(guard);

    push_delta(st, &piece_clone);

    if st.use_bpe && st.is_greeting && crate::bpe::text_is_greetingish(&st.acc_text) {
        let acc = st.acc_text.clone();
        finish_job(st, &acc);
    }
}

fn run_coarse(st: &mut ActiveState) {
    if ACTIVE_CANCEL.load(Ordering::Acquire) {
        finish_job(st, "[cancelled]");
        return;
    }
    // Uma slice = generate completo (AirLLM / GGUF) — ainda fora de AGENT_TICK_BUSY.
    let text = {
        if let Some(ref sm) = *CURRENT_STREAMING_MODEL.lock() {
            sm.generate(&st.prompt)
        } else if let Some(ref m) = *CURRENT_MODEL.lock() {
            m.generate(&st.prompt)
        } else {
            String::from(NO_MODEL_MSG)
        }
    };
    emit_msg_delta(&text);
    st.acc_text = text.clone();
    finish_job(st, &text);
}

/// Executa no máximo 1 slice. Seguro chamar do InferWorker ou AP idle (sem AGENT_TICK_BUSY).
/// Retorna true se havia trabalho (claim ou decode).
pub fn poll_slice() -> bool {
    if SLICE_BUSY.swap(true, Ordering::AcqRel) {
        return false;
    }

    let mut did = false;
    if ACTIVE.lock().is_none() {
        did = try_claim_into_active();
    } else {
        did = true;
    }

    let mut finished = false;
    {
        let mut guard = ACTIVE.lock();
        if let Some(ref mut st) = *guard {
            match st.phase {
                Phase::NeedPrefill => run_prefill(st),
                Phase::Decoding => run_decode_one(st),
                Phase::CoarseFallback => run_coarse(st),
                Phase::Finishing | Phase::Idle => {
                    finished = true;
                }
            }
            if st.phase == Phase::Idle {
                finished = true;
            }
            did = true;
        } else {
            did = false;
        }
        if finished {
            *guard = None;
        }
    }

    SLICE_BUSY.store(false, Ordering::Release);
    did
}

/// Helper para testes host da fila.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_claim_cancel_queue() {
        // Reset indices (best-effort; statics shared).
        while queue_pending() > 0 || ACTIVE.lock().is_some() {
            ACTIVE_CANCEL.store(true, Ordering::Release);
            poll_slice();
            if queue_pending() == 0 && ACTIVE.lock().is_none() {
                break;
            }
            // Drain queue slots
            let h = HEAD.load(Ordering::Relaxed);
            let t = TAIL.load(Ordering::Acquire);
            if h < t {
                HEAD.store(t, Ordering::Release);
                PENDING_COUNT.store(0, Ordering::Release);
                for i in 0..QUEUE_CAP {
                    slots()[i].occupied.store(false, Ordering::Release);
                }
            }
            *ACTIVE.lock() = None;
            break;
        }

        let id = submit(
            String::from("ola"),
            InferMode::Plain,
            TOPIC_LLM_RESPONSE,
        )
        .expect("submit");
        assert!(queue_pending() >= 1);
        assert!(cancel(id));
    }

    #[test]
    fn submit_full() {
        // Fill without claiming
        let mut ids = Vec::new();
        for i in 0..QUEUE_CAP {
            match submit(
                alloc::format!("p{}", i),
                InferMode::Plain,
                TOPIC_LLM_RESPONSE,
            ) {
                Ok(id) => ids.push(id),
                Err(SubmitErr::Full) => break,
                Err(_) => panic!("unexpected"),
            }
        }
        assert!(submit(String::from("overflow"), InferMode::Plain, TOPIC_LLM_RESPONSE).is_err());
        // cleanup
        for id in ids {
            let _ = cancel(id);
        }
        HEAD.store(TAIL.load(Ordering::Relaxed), Ordering::Release);
        PENDING_COUNT.store(0, Ordering::Release);
        for i in 0..QUEUE_CAP {
            slots()[i].occupied.store(false, Ordering::Release);
        }
    }
}
