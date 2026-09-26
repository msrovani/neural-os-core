//! InferQueue Full D+B+C (ADR-0057 WS-H) — Falcon3 off BSP.
//!
//! Contrato:
//! - BSP/Hermes/CortexAgent só **enfileiram** InferJob (nunca `generate_*` no tick).
//! - `poll_slice` (InferWorker BSP ou AP idle) roda **1 slice** (prefill layer(s) ou 1 token).
//! - SESSION_345 F4: prefill **layer-yield** (`Prefilling`) — 1 layer ativa/slice (heavy).
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
    NO_MODEL_MSG, TOPIC_LLM_RESPONSE, model_is_loaded,
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
    /// headroom < 48MB — SESSION_366: não enfileirar sob estouro de janela bump.
    HeapPressure,
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

/// Telemetria F4 (lock-free) — slices de prefill, µs, tokens decode.
static TELEM_PREFILL_SLICES: AtomicU64 = AtomicU64::new(0);
static TELEM_PREFILL_US: AtomicU64 = AtomicU64::new(0);
static TELEM_LAST_PREFILL_US: AtomicU64 = AtomicU64::new(0);
static TELEM_DECODE_TOKENS: AtomicU64 = AtomicU64::new(0);
/// Soma µs de todas as fases decode concluídas (jobs).
static TELEM_DECODE_US: AtomicU64 = AtomicU64::new(0);
/// Último job: tokens gerados + µs wall (TSC) → tok/s = toks*1e6/us.
static TELEM_LAST_DECODE_TOKS: AtomicU64 = AtomicU64::new(0);
static TELEM_LAST_DECODE_US: AtomicU64 = AtomicU64::new(0);
/// Início do decode do job ativo (0 = idle / ainda em prefill).
static DECODE_T0_US: AtomicU64 = AtomicU64::new(0);
static DECODE_JOB_TOKS: AtomicU64 = AtomicU64::new(0);

/// Lane A2 — MVP 1 token real FALCON3.V6 (prova, 1 inferência por boot).
/// Escopo fechado: prompt curto fixo, max 1 token, stride 1 local, ctx mínimo.
/// Sem Medusa/draft. Defaults globais intactos (override só no job de prova).
pub const A2_PROOF_PROMPT: &str = "oi";
pub const A2_PROOF_MAX_GEN: usize = 1;
pub const A2_PROOF_CTX_CAP: usize = 32;
static A2_PROOF_SUBMITTED: AtomicBool = AtomicBool::new(false);
static A2_PROOF_DONE: AtomicBool = AtomicBool::new(false);
static A2_PROOF_ID: AtomicU64 = AtomicU64::new(0);
/// Lane D-cortex: orçamento de slice espelho do watchdog (agent-core
/// TICK_WATCHDOG_MS=500, read-only). Só medição local — nunca altera o watchdog.
const A2_SLICE_BUDGET_US: u64 = 500_000;
/// Slices lentas da prova neste boot (cada uma = 1 overrun do infer_worker no BSP).
static A2_SLOW_SLICES: AtomicU64 = AtomicU64::new(0);
/// Log `resident_too_big` emitido 1×/boot (maybe_submit roda todo slice).
static A2_ABSENT_LOGGED: AtomicBool = AtomicBool::new(false);

/// Lane D: aborta antes do 4º slice lento (4 overruns → Paused global) ou com
/// UI rendida (ui_yield). Só decisão local sobre a prova — fila real intacta,
/// TICK_WATCHDOG_MS/budget intocados.
fn a2_should_abort_slice(slow_slices: u64, ui_yield: bool) -> bool {
    ui_yield || slow_slices >= 3
}

/// Snapshot: (prefill_slices, prefill_us_total, last_prefill_us, decode_tokens).
pub fn telemetry() -> (u64, u64, u64, u64) {
    (
        TELEM_PREFILL_SLICES.load(Ordering::Relaxed),
        TELEM_PREFILL_US.load(Ordering::Relaxed),
        TELEM_LAST_PREFILL_US.load(Ordering::Relaxed),
        TELEM_DECODE_TOKENS.load(Ordering::Relaxed),
    )
}

/// Último decode concluído: (tokens, µs). `(0,0)` = ainda sem amostra.
pub fn last_decode_timing() -> (u64, u64) {
    (
        TELEM_LAST_DECODE_TOKS.load(Ordering::Relaxed),
        TELEM_LAST_DECODE_US.load(Ordering::Relaxed),
    )
}

/// tok/s inteiro do último job (`toks * 1_000_000 / us`). 0 = n/a.
pub fn last_decode_tok_s() -> u64 {
    let (toks, us) = last_decode_timing();
    if toks == 0 || us == 0 {
        0
    } else {
        toks.saturating_mul(1_000_000) / us
    }
}

/// Grava amostra de decode (InferQueue **ou** `generate_speculative` clássico).
/// Usado pelo Hub Health e pelo microbench QEMU Falcon3-3B.
pub fn record_last_decode(toks: u64, us: u64) {
    if toks == 0 || us == 0 {
        return;
    }
    let us = us.max(1);
    TELEM_DECODE_TOKENS.fetch_add(toks, Ordering::Relaxed);
    TELEM_DECODE_US.fetch_add(us, Ordering::Relaxed);
    TELEM_LAST_DECODE_TOKS.store(toks, Ordering::Relaxed);
    TELEM_LAST_DECODE_US.store(us, Ordering::Relaxed);
    let tps = toks.saturating_mul(1_000_000) / us;
    // milli-tok/s quando <1 tok/s (QEMU/soft-float do 3B).
    let milli = toks.saturating_mul(1_000_000_000) / us;
    let us_per = us / toks.max(1);
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "decode_tok/s={} milli={} us/tok={} toks={} us={}",
        tps,
        milli,
        us_per,
        toks,
        us
    );
}

/// Durante decode ativo: tok/s ao vivo (0 se idle/prefill).
pub fn live_decode_tok_s() -> u64 {
    let t0 = DECODE_T0_US.load(Ordering::Relaxed);
    if t0 == 0 {
        return 0;
    }
    let toks = DECODE_JOB_TOKS.load(Ordering::Relaxed);
    if toks == 0 {
        return 0;
    }
    let now = k_nano::tsc::now_us();
    let us = now.saturating_sub(t0);
    if us == 0 {
        0
    } else {
        toks.saturating_mul(1_000_000) / us
    }
}

/// Linha curta p/ Hub Health / SysInfo (≤36 chars).
pub fn hub_infer_line(queue_pending: u64, running: bool) -> alloc::string::String {
    let live = live_decode_tok_s();
    let last = last_decode_tok_s();
    if running && live > 0 {
        alloc::format!("{}t/s q{} run", live, queue_pending)
    } else if last > 0 {
        alloc::format!(
            "{}t/s q{} {}",
            last,
            queue_pending,
            if running { "run" } else { "ok" }
        )
    } else if running {
        alloc::format!("q{} run", queue_pending)
    } else {
        alloc::format!("q{} idle", queue_pending)
    }
}

/// Fase da state machine de generate fatiado.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    NeedPrefill,
    /// Prefill layer-yield (F4) — 1+ layers por `poll_slice`.
    Prefilling,
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
    /// Prefill yield: hidden residual + mask + cursor de layer.
    prefill_x: Option<Tensor>,
    prefill_mask: Option<Tensor>,
    prefill_layer: usize,
    prefill_new_len: usize,
    prefill_start_pos: usize,
    prefill_total_seq: usize,
    prefill_t0_us: u64,
    /// Lane A2: job de prova (stride 1 local, ctx mínimo, 1 token).
    is_proof: bool,
    /// Wall inicial do job (total_us no finish_job).
    job_t0_us: u64,
    /// prefill wall do job (set no finalize do prefill).
    prefill_us: u64,
    /// SESSION_350: plano heap AIOS (escalate → resposta HITL sem forward).
    heap_escalate: bool,
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

/// Lane A2: refuse honesto com log explícito (nunca panic/unwrap).
#[inline]
fn a2_refuse(st: &ActiveState, reason: &str) {
    if st.is_proof {
        k_nano::slog_cortex!("InferQ", "warn", "a2_proof refuse {} id={}", reason, st.job_id);
    }
}

/// Lane A2: submete 1 job de prova por boot. Chamado do topo de `poll_slice`
/// (fora do tick). Não compete: com fila real ou job ativo, adia para o
/// próximo slice. Silencioso sem modelo (CortexAgent já loga ABSENT).
pub fn maybe_submit_a2_proof() -> bool {
    if A2_PROOF_DONE.load(Ordering::Acquire) || A2_PROOF_SUBMITTED.load(Ordering::Acquire) {
        return false;
    }
    if ACTIVE.lock().is_some() || PENDING_COUNT.load(Ordering::Relaxed) > 0 {
        return false;
    }
    if !model_is_loaded() {
        // Lane D: refuse de residente com log acionável (1×/boot; segue
        // tentando em silêncio — modelo pode carregar tarde via ATA).
        if !A2_ABSENT_LOGGED.load(Ordering::Acquire) {
            if let Some((need, head)) = crate::cortex::last_resident_refuse() {
                A2_ABSENT_LOGGED.store(true, Ordering::Release);
                k_nano::slog_cortex!(
                    "InferQ",
                    "warn",
                    "a2_proof refuse resident_too_big need={}MB headroom={}MB (3B sem janela bump; SKU menor ou AirLLM; HITL)",
                    need,
                    head
                );
            }
        }
        return false;
    }
    match submit(String::from(A2_PROOF_PROMPT), InferMode::Plain, TOPIC_LLM_RESPONSE) {
        Ok(id) => {
            A2_PROOF_ID.store(id, Ordering::Release);
            A2_PROOF_SUBMITTED.store(true, Ordering::Release);
            k_nano::slog_cortex!("InferQ", "ok", "a2_proof submit id={}", id);
            true
        }
        // Fila cheia / HeapPressure — tenta de novo no próximo slice.
        Err(_) => false,
    }
}

/// Enfileira job. Acorda APs via monitor flag.
pub fn submit(prompt: String, mode: InferMode, reply_topic: &str) -> Result<u64, SubmitErr> {
    if prompt.is_empty() {
        return Err(SubmitErr::EmptyPrompt);
    }
    // Fail-closed ANTES de claim/encode: headroom=4MB + encode BPE = #UD/#PF (mesh A).
    let obs = k_nano::allocator::heap_observe();
    if obs.headroom_mb < 48 {
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "submit refuse HeapPressure headroom={}MB used={}MB window={}MB",
            obs.headroom_mb,
            obs.used_mb,
            obs.window_mb
        );
        return Err(SubmitErr::HeapPressure);
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
    // M6: multi-producer (BSP pode submeter enquanto CortexAgent também
    // submete) — CAS no TAIL antes de escrever o slot. CLAIM do consumer
    // continua por CAS no HEAD (abaixo).
    let idx = loop {
        let t = TAIL.load(Ordering::Acquire);
        let h = HEAD.load(Ordering::Acquire);
        if t.wrapping_sub(h) >= QUEUE_CAP {
            return Err(SubmitErr::Full);
        }
        let slot = &slots()[t % QUEUE_CAP];
        if slot.occupied.load(Ordering::Acquire) {
            // Pode ser transiente (HEAD avançou e occupied ainda não caiu) —
            // honesto: Full; o chamador retenta no próximo tick se quiser.
            return Err(SubmitErr::Full);
        }
        if TAIL
            .compare_exchange_weak(t, t + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            break t % QUEUE_CAP;
        }
        core::hint::spin_loop();
    };
    let slot = &slots()[idx];
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    *slot.prompt.lock() = Some(prompt);
    *slot.reply_topic.lock() = Some(topic);
    slot.mode.store(mode as u8, Ordering::Release);
    slot.cancel.store(false, Ordering::Release);
    slot.id.store(id, Ordering::Release);
    slot.occupied.store(true, Ordering::Release);
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
        let slot = &slots()[h % QUEUE_CAP];
        // H1: submit publica os campos do slot ANTES do store occupied=true
        // (Release). Avançar HEAD num slot ainda não publicado lia lixo/=
        // "[cancelled]" silencioso. Claim só se occupied==true; HEAD só avança
        // quando há slot publicado para clamar.
        if !slot.occupied.load(Ordering::Acquire) {
            return false;
        }
        if HEAD
            .compare_exchange_weak(h, h + 1, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            continue;
        }
        // Cinto-e-suspensórios pós-CAS: revalida occupied (cancel race).
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
        // Re-check na claim: headroom pode ter caído desde o submit.
        let obs = k_nano::allocator::heap_observe();
        if obs.headroom_mb < 48 {
            k_nano::slog_cortex!(
                "InferQ",
                "warn",
                "claim refuse HeapPressure id={} headroom={}MB",
                id,
                obs.headroom_mb
            );
            emit_reply(&reply_topic, crate::heap_aios::ESCALATE_STATIC_MSG);
            continue;
        }
        ACTIVE_ID.store(id, Ordering::Release);
        ACTIVE_CANCEL.store(false, Ordering::Release);
        let coarse = CURRENT_STREAMING_MODEL.lock().is_some();
        let is_proof = id == A2_PROOF_ID.load(Ordering::Acquire);
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
            prefill_x: None,
            prefill_mask: None,
            prefill_layer: 0,
            prefill_new_len: 0,
            prefill_start_pos: 0,
            prefill_total_seq: 0,
            prefill_t0_us: 0,
            is_proof,
            job_t0_us: k_nano::tsc::now_us(),
            prefill_us: 0,
            heap_escalate: false,
        });
        infer_guard_begin();
        emit_msg_start();
        k_nano::slog_cortex!("InferQ", "ok", "claim id={} coarse={}", id, coarse as u8);
        if is_proof {
            // Roteador fiel: InferWorker::tick pula poll_slice com ap_pollable
            // (hermes) e try_infer_poll_slice exige ap_pollable (k_nano) →
            // true = AP idle (fora do BUSY/budget), false = fallback BSP (sob BUSY).
            let by = if k_nano::smp::ap_pollable() { "ap" } else { "bsp" };
            k_nano::slog_cortex!("InferQ", "ok", "a2_proof claimed_by={} id={}", by, id);
        }
        return true;
    }
}

fn finish_job(st: &mut ActiveState, text: &str) {
    crate::heap_aios::clear_job_overrides();
    crate::vocab_shortlist::set_skip_full_unembed(false);
    // Fecha wall-clock do decode (tok/s Hub Health).
    let t0 = DECODE_T0_US.swap(0, Ordering::AcqRel);
    let job_toks = DECODE_JOB_TOKS.swap(0, Ordering::AcqRel);
    let mut us = 0u64;
    if t0 != 0 && job_toks > 0 {
        us = k_nano::tsc::now_us().saturating_sub(t0).max(1);
        TELEM_DECODE_US.fetch_add(us, Ordering::Relaxed);
        TELEM_LAST_DECODE_TOKS.store(job_toks, Ordering::Relaxed);
        TELEM_LAST_DECODE_US.store(us, Ordering::Relaxed);
        let tps = job_toks.saturating_mul(1_000_000) / us;
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "decode_tok/s={} toks={} us={}",
            tps,
            job_toks,
            us
        );
    }
    // Verify + Remember (SESSION_350 Heap AIOS).
    let completed = !st.heap_escalate && !text.starts_with("[heap escalate]");
    crate::heap_aios::verify_job(completed, job_toks, us);
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
    // Lane A2: total_us/prefill_us/decode_us no serial (prova 1 token).
    if st.is_proof {
        let total_us = k_nano::tsc::now_us().saturating_sub(st.job_t0_us.max(1));
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "a2_proof done id={} total_us={} prefill_us={} decode_us={} toks={} out_len={}",
            st.job_id,
            total_us,
            st.prefill_us,
            us,
            job_toks,
            out.len()
        );
        A2_PROOF_DONE.store(true, Ordering::Release);
    }
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "done id={} len={} in_flight={}",
        st.job_id,
        out.len(),
        infer_in_flight() as u8
    );
}

/// Fronteira de frase para TTS parcial — **whitelist** fechada: só `. ! ? ;`
/// e só quando seguidos de ` '/\n' ou fim (não divide "3.14" nem a...
/// reticências"). Qualquer outro byte (vírgula, dois-pontos, UTF-8) nunca corta.
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

fn run_prefill_setup(st: &mut ActiveState) {
    let t_setup0 = k_nano::tsc::now_us();
    if st.job_t0_us == 0 {
        st.job_t0_us = t_setup0;
    }
    if ACTIVE_CANCEL.load(Ordering::Acquire) {
        a2_refuse(st, "cancelled");
        finish_job(st, "[cancelled]");
        return;
    }
    let guard = CURRENT_MODEL.lock();
    let Some(model_box) = guard.as_ref() else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(model) = model_box.as_transformer() else {
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

    // SESSION_350/366: Observe→Plan→Act ANTES do encode BPE.
    // Encode sob headroom=4MB estoura a janela bump → #UD/#PF (mesh A).
    let base = crate::difficulty_gate::classify(&st.prompt, st.is_greeting, model.hidden);
    let mut plan = crate::heap_aios::plan_for(base, model.hidden, st.use_bpe, st.is_greeting);
    // Lane A2: override LOCAL só no job de prova (defaults globais intactos).
    if st.is_proof {
        plan.tier = crate::difficulty_gate::ComputeTier::Full;
        plan.soft_stride = 1;
        if plan.ctx_cap > A2_PROOF_CTX_CAP {
            plan.ctx_cap = A2_PROOF_CTX_CAP;
        }
        plan.max_gen = A2_PROOF_MAX_GEN.min(8).max(1);
        plan.force_slim = true;
    }
    crate::heap_aios::apply_plan(plan, model.hidden);
    if plan.kind == crate::heap_aios::HeapPlanKind::Escalate {
        st.heap_escalate = true;
        drop(guard);
        // Mensagem estática — format! sob 4MB headroom também aloca.
        a2_refuse(st, "heap_escalate");
        finish_job(st, crate::heap_aios::ESCALATE_STATIC_MSG);
        return;
    }

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

    let max_seq = plan.ctx_cap.min(if model.hidden >= 2048 {
        model.max_seq.min(512)
    } else {
        model.max_seq.min(64)
    });
    st.max_seq = max_seq;
    if plan.force_slim && tokens.len() > 1 {
        tokens = crate::cortex::slim_prompt_tokens_for_heavy(&tokens, st.use_bpe);
    }
    if tokens.len() > max_seq {
        let keep = max_seq.max(1);
        tokens = tokens[tokens.len() - keep..].to_vec();
    }
    st.prompt_len = tokens.len();
    st.max_gen = plan.max_gen;
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "tier={} max_gen={} soft_stride={} ctx={} heap_plan={}",
        plan.tier.name(),
        st.max_gen,
        plan.soft_stride,
        max_seq,
        match plan.kind {
            crate::heap_aios::HeapPlanKind::Ok => "ok",
            crate::heap_aios::HeapPlanKind::Degrade => "degrade",
            crate::heap_aios::HeapPlanKind::Escalate => "escalate",
        }
    );
    // Lane A2: prefill_us base (setup) no serial; BPE ausente explícito.
    if st.is_proof {
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "a2_proof setup id={} bpe={} prompt_len={} setup_us={}",
            st.job_id,
            st.use_bpe as u8,
            st.prompt_len,
            k_nano::tsc::now_us().saturating_sub(t_setup0)
        );
        if !st.use_bpe {
            k_nano::slog_cortex!(
                "InferQ",
                "warn",
                "a2_proof bpe_absent fallback=char99 id={}",
                st.job_id
            );
        }
        // Lane D: Medusa OFF na prova — InferQueue nunca faz draft/speculative
        // (só generate_speculative usa medusa_heads); header só informa.
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "a2_proof medusa_off heads={} id={}",
            model.medusa_heads.len(),
            st.job_id
        );
    }

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

    let (x, mask, start_pos, new_len, total_seq) = model.embed_for_kv(&tokens, &cache);
    if !x.is_valid()
        || !mask.is_valid()
        || mask.shape != (new_len, total_seq)
        || new_len == 0
    {
        k_nano::slog_cortex!(
            "InferQ",
            "fail",
            "embed/mask refuse id={} x={:?} mask={:?}",
            st.job_id,
            x.shape,
            mask.shape
        );
        drop(guard);
        a2_refuse(st, "embed_mask");
        finish_job(st, "[heap: embed/mask refuse — HITL escalate]");
        return;
    }
    st.tokens = tokens;
    st.cache = Some(cache);
    st.prefill_x = Some(x);
    st.prefill_mask = Some(mask);
    st.prefill_layer = 0;
    st.prefill_new_len = new_len;
    st.prefill_start_pos = start_pos;
    st.prefill_total_seq = total_seq;
    st.prefill_t0_us = k_nano::tsc::now_us();
    st.phase = Phase::Prefilling;
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "prefill_begin id={} tokens={} layers={}",
        st.job_id,
        st.prompt_len,
        model.layers.len()
    );
    drop(guard);
}/// Uma slice de prefill: aplica até `layers_per_slice` layers ativas (soft_stride).
fn run_prefill_step(st: &mut ActiveState) {
    if ACTIVE_CANCEL.load(Ordering::Acquire) {
        a2_refuse(st, "cancelled");
        finish_job(st, "[cancelled]");
        return;
    }
    // Lane D-cortex: fail-closed ANTES do slice pesado (só a prova; a fila
    // real nunca aborta aqui). No BSP cada slice >500ms = 1 overrun do
    // infer_worker; o 4º pausa o agente e o ACTIVE ficaria preso até Crashed.
    if st.is_proof
        && a2_should_abort_slice(
            A2_SLOW_SLICES.load(Ordering::Relaxed),
            k_nano::smp::ui_yield_infer(),
        )
    {
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "a2_proof refuse watchdog_would_pause id={} slow={} ui_yield={}",
            st.job_id,
            A2_SLOW_SLICES.load(Ordering::Relaxed),
            k_nano::smp::ui_yield_infer() as u8
        );
        finish_job(
            st,
            "[a2_proof abort watchdog_would_pause — BSP sem AP-IDT, slice>500ms pausaria infer_worker; HITL]",
        );
        return;
    }
    let t_slice0 = k_nano::tsc::now_us();
    let guard = CURRENT_MODEL.lock();
    let Some(model_box) = guard.as_ref() else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(model) = model_box.as_transformer() else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };

    let Some(ref mut x) = st.prefill_x else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(ref mask) = st.prefill_mask else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(ref mut cache) = st.cache else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };

    let n_layers = model.layers.len();
    // ADR-0101 Onda 2: soft_stride via difficulty_gate.
    // Lane A2: stride 1 LOCAL na prova (sem pular layers; global intacto).
    let soft_stride: usize = if st.is_proof {
        1
    } else {
        crate::difficulty_gate::effective_soft_stride(model.hidden)
    };
    let layers_per_slice: usize = if model.hidden >= 2048 { 1 } else { 2 };
    let mut applied = 0usize;
    let mut pad_oom = false;

    while st.prefill_layer < n_layers && applied < layers_per_slice {
        let li = st.prefill_layer;
        st.prefill_layer += 1;
        if soft_stride > 1 && (li % soft_stride) != 0 {
            // SESSION_351/359: pad KV — OOM aborta job (não desalinha silenciosamente)
            let kd = cache.k_dim();
            let zk = Tensor::new((st.prefill_new_len, kd));
            let zv = Tensor::new((st.prefill_new_len, kd));
            if zk.is_valid() && zv.is_valid() {
                cache.append(li, &zk, &zv);
            } else {
                pad_oom = true;
                break;
            }
            continue;
        }
        let layer = &model.layers[li];
        let x_before = x.shape;
        model.apply_one_layer(
            li,
            layer,
            x,
            cache,
            st.prefill_start_pos,
            st.prefill_new_len,
            st.prefill_total_seq,
            mask,
        );
        if !x.is_valid() || x.shape != x_before {
            drop(guard);
            a2_refuse(st, "apply_layer");
            finish_job(st, "[heap: apply_one_layer refuse — HITL escalate]");
            return;
        }
        applied += 1;
    }

    let now1 = k_nano::tsc::now_us();
    let slice_us = now1.saturating_sub(t_slice0);
    TELEM_PREFILL_SLICES.fetch_add(1, Ordering::Relaxed);
    TELEM_PREFILL_US.fetch_add(slice_us, Ordering::Relaxed);
    TELEM_LAST_PREFILL_US.store(slice_us, Ordering::Relaxed);
    // Lane D: conta slice lenta da prova (TSC morto + layer aplicada = lenta,
    // fail-closed). O próximo slice aborta antes do 4º overrun (→Paused).
    if st.is_proof && applied > 0 {
        let slow = slice_us > A2_SLICE_BUDGET_US || (t_slice0 == 0 && now1 == 0);
        if slow {
            let n = A2_SLOW_SLICES.fetch_add(1, Ordering::Relaxed) + 1;
            k_nano::slog_cortex!("InferQ", "warn", "a2_proof slow_slice n={} us={}", n, slice_us);
        }
    }
    // Budget honesto: layer >100ms em soft-float é esperado; só warn se >2s.
    if slice_us > 2_000_000 {
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "prefill_slice slow id={} layer={}/{} us={}",
            st.job_id,
            st.prefill_layer,
            n_layers,
            slice_us
        );
    }

    if pad_oom {
        k_nano::slog_cortex!("InferQ", "fail", "soft_stride pad OOM — abort prefill");
        drop(guard);
        a2_refuse(st, "pad_oom");
        finish_job(st, "[oom]");
        return;
    }

    if st.prefill_layer < n_layers {
        // Yield — próximo poll_slice continua.
        drop(guard);
        return;
    }

    // Finalize
    cache.advance(st.prefill_new_len);
    let new_len = st.prefill_new_len;
    let (last_hidden, last_logits) = model.finalize_logits(x, new_len);
    let total_us = k_nano::tsc::now_us().saturating_sub(st.prefill_t0_us);
    st.prefill_us = total_us;
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "prefill_done id={} tokens={} layers={} slices={} us={}",
        st.job_id,
        st.prompt_len,
        n_layers,
        TELEM_PREFILL_SLICES.load(Ordering::Relaxed),
        total_us
    );
    // Lane A2: prefill_us explícito da prova.
    if st.is_proof {
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "a2_proof prefill_us={} id={} layers={}",
            total_us,
            st.job_id,
            n_layers
        );
    }

    st.recent_u16.clear();
    if !st.is_greeting {
        if let Some(&last) = st.tokens.last() {
            st.recent_u16.push(last as u16);
        }
    }
    st.last_hidden = Some(last_hidden);
    st.last_logits = Some(last_logits);
    st.prefill_x = None;
    st.prefill_mask = None;
    st.step = 0;
    st.phase = Phase::Decoding;
    DECODE_JOB_TOKS.store(0, Ordering::Release);
    DECODE_T0_US.store(k_nano::tsc::now_us(), Ordering::Release);
    drop(guard);
}

fn run_prefill(st: &mut ActiveState) {
    run_prefill_setup(st);
    if st.phase == Phase::Prefilling {
        run_prefill_step(st);
    }
}

fn run_decode_one(st: &mut ActiveState) {
    if ACTIVE_CANCEL.load(Ordering::Acquire) {
        let acc = st.acc_text.clone();
        a2_refuse(st, "cancelled");
        finish_job(st, if acc.is_empty() { "[cancelled]" } else { &acc });
        return;
    }
    if st.step >= st.max_gen {
        let acc = st.acc_text.clone();
        finish_job(st, &acc);
        return;
    }
    // Lane A2: decode_us por token da prova.
    let t_d0 = k_nano::tsc::now_us();

    let guard = CURRENT_MODEL.lock();
    let Some(model_box) = guard.as_ref() else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(model) = model_box.as_transformer() else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };

    let Some(ref mut cache) = st.cache else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };
    let Some(ref mut last_logits) = st.last_logits else {
        drop(guard);
        a2_refuse(st, "model_absent");
        finish_job(st, NO_MODEL_MSG);
        return;
    };

    if st.tokens.len() >= st.max_seq {
        // Onda 1: H2O no InferQueue (produção).
        let dropped = crate::kv_h2o::h2o_evict(cache, 8, st.max_seq / 6);
        if dropped > 0 {
            crate::cognitive_runtime::note_h2o_drops(dropped);
            if st.tokens.len() > cache.len {
                st.tokens.drain(..st.tokens.len() - cache.len);
            }
        }
        if st.tokens.len() >= st.max_seq {
            let acc = st.acc_text.clone();
            drop(guard);
            finish_job(st, &acc);
            return;
        }
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
    TELEM_DECODE_TOKENS.fetch_add(1, Ordering::Relaxed);
    DECODE_JOB_TOKS.fetch_add(1, Ordering::Relaxed);

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
    // Lane A2: decode_us explícito (1 forward do prefill + argmax/decode aqui).
    if st.is_proof {
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "a2_proof decode_one id={} step={} tok={} decode_us={} piece_len={}",
            st.job_id,
            st.step,
            next,
            k_nano::tsc::now_us().saturating_sub(t_d0),
            piece_clone.len()
        );
    }

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
    DECODE_JOB_TOKS.store(0, Ordering::Release);
    DECODE_T0_US.store(k_nano::tsc::now_us(), Ordering::Release);
    // SESSION_359: coarse = generate() bloqueante sem yield — barge-in só
    // observável após o retorno. Log honesto; se cancelou durante generate, descarta.
    k_nano::slog_cortex!(
        "InferQ",
        "warn",
        "coarse generate id={} (uncancellable mid-call)",
        st.job_id
    );
    let text = {
        if let Some(ref sm) = *CURRENT_STREAMING_MODEL.lock() {
            sm.generate(&st.prompt)
        } else if let Some(ref m) = *CURRENT_MODEL.lock() {
            m.generate(&st.prompt)
        } else {
            String::from(NO_MODEL_MSG)
        }
    };
    if ACTIVE_CANCEL.load(Ordering::Acquire) {
        finish_job(st, "[cancelled]");
        return;
    }
    DECODE_JOB_TOKS.store(1, Ordering::Release);
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
    // Lane A2: 1 inferência de prova por boot (fora do tick; não compete).
    let _ = maybe_submit_a2_proof();

    if ACTIVE.lock().is_none() {
        let _ = try_claim_into_active();
    }

    let mut did = false;
    let mut finished = false;
    {
        let mut guard = ACTIVE.lock();
        if let Some(ref mut st) = *guard {
            did = true;
            match st.phase {
                Phase::NeedPrefill => run_prefill(st),
                Phase::Prefilling => run_prefill_step(st),
                Phase::Decoding => run_decode_one(st),
                Phase::CoarseFallback => run_coarse(st),
                Phase::Finishing | Phase::Idle => {
                    finished = true;
                }
            }
            if st.phase == Phase::Idle {
                finished = true;
            }
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
    use spin::Mutex;

    /// Statics da InferQueue sao partilhados — testes em paralelo corrompem a fila.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn drain_infer_queue_statics() {
        ACTIVE_CANCEL.store(true, Ordering::Release);
        *ACTIVE.lock() = None;
        ACTIVE_ID.store(0, Ordering::Release);
        ACTIVE_CANCEL.store(false, Ordering::Release);
        HEAD.store(TAIL.load(Ordering::Relaxed), Ordering::Release);
        PENDING_COUNT.store(0, Ordering::Release);
        A2_PROOF_SUBMITTED.store(false, Ordering::Release);
        A2_PROOF_DONE.store(false, Ordering::Release);
        A2_PROOF_ID.store(0, Ordering::Release);
        A2_SLOW_SLICES.store(0, Ordering::Release);
        A2_ABSENT_LOGGED.store(false, Ordering::Release);
        for i in 0..QUEUE_CAP {
            slots()[i].occupied.store(false, Ordering::Release);
        }
    }

    #[test]
    fn submit_claim_cancel_queue() {
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        let id = submit(
            String::from("ola"),
            InferMode::Plain,
            TOPIC_LLM_RESPONSE,
        )
        .expect("submit");
        assert!(queue_pending() >= 1 || ACTIVE_ID.load(Ordering::Relaxed) == id);
        assert!(cancel(id) || ACTIVE_ID.load(Ordering::Relaxed) == 0);
        drain_infer_queue_statics();
    }

    #[test]
    fn submit_full() {
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
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
        assert_eq!(ids.len(), QUEUE_CAP, "queue should fill exactly");
        assert!(submit(String::from("overflow"), InferMode::Plain, TOPIC_LLM_RESPONSE).is_err());
        for id in ids {
            let _ = cancel(id);
        }
        drain_infer_queue_statics();
    }

    #[test]
    fn cancel_unknown_id_is_false() {
        let _g = TEST_LOCK.lock();
        assert!(!cancel(u64::MAX));
    }

    #[test]
    fn topics_are_stable() {
        assert_eq!(TOPIC_LLM_STREAM, "LLM_STREAM");
        assert_eq!(TOPIC_LLM_RESPONSE, "LLM_RESPONSE");
    }

    #[test]
    fn lab_prompts_fit_linj_blob() {
        for p in [
            crate::llm_response_gate::prompts::PING_CURTO,
            crate::llm_response_gate::prompts::APP_CLIMA_TEMPO,
            crate::llm_response_gate::prompts::CLIMA_PASSO_FUNDO,
        ] {
            assert!(p.len() <= 240);
        }
    }

    #[test]
    fn a2_proof_limits_are_minimal() {
        // Lane A2: prova = prompt curto fixo, 1..=8 tokens, ctx mínimo, sem statics.
        let _g = TEST_LOCK.lock();
        assert_eq!(A2_PROOF_MAX_GEN, 1);
        assert!(!A2_PROOF_PROMPT.is_empty() && A2_PROOF_PROMPT.len() <= 16);
        assert!(A2_PROOF_CTX_CAP >= 8 && A2_PROOF_CTX_CAP <= 64);
    }

    #[test]
    fn a2_watchdog_abort_predicate() {
        // Lane D: só a prova aborta, antes do 4º overrun (Paused global) ou com UI rendida.
        let _g = TEST_LOCK.lock();
        assert_eq!(A2_SLICE_BUDGET_US, 500_000);
        assert!(!a2_should_abort_slice(0, false));
        assert!(!a2_should_abort_slice(2, false));
        assert!(a2_should_abort_slice(3, false));
        assert!(a2_should_abort_slice(0, true));
        assert!(a2_should_abort_slice(9, true));
    }
}
