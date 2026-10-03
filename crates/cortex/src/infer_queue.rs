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
    infer_guard_begin, infer_guard_end, infer_in_flight, logits_recycle, KvCache,
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
    /// Bullet 4: struct-máquina do job (None = texto puro).
    machine: Mutex<Option<MachineCtx>>,
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
            machine: Mutex::new(None),
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
/// SESSION_415: log 1x do fail-closed por episódio de heap crítico.
static HEAP_FULL_LOGGED: AtomicBool = AtomicBool::new(false);
/// Log `waiting for slot` emitido 1×/boot (retry Full é silencioso).
static A2_SLOT_WAIT_LOGGED: AtomicBool = AtomicBool::new(false);
/// Deadline wall (TSC absoluto, µs) da prova. 0 = desarmado. Se estourar, o
/// gate `a2_proof_pending` é liberado — a prova nunca muta o CortexAgent para sempre.
static A2_PROOF_DEADLINE_AT_US: AtomicU64 = AtomicU64::new(0);
static A2_PROOF_TIMEOUT_LOGGED: AtomicBool = AtomicBool::new(false);
/// Deadline da prova: 120s **NO-PROGRESS** (s429-lab). Cada slice concluído
/// re-arma o deadline (`A2_PROOF_DEADLINE_AT_US` refresh no fim do
/// poll_slice) — só NENHUM slice em 120s (wedge total) declara timeout.
/// Wall-clock puro gerou FALSO POSITIVO no 8c/WHPX: prova legítima de 58s
/// + espera de fila > 120s ⇒ "timeout" com matmuls progredindo e done
/// depois. Piso de progresso: idle real ~160ms/slice × 22 layers ≈ 8s;
/// host sob carga ~2×; 120s = 15× o pior caso POR SLICE.
const A2_PROOF_DEADLINE_US: u64 = 120_000_000;
/// s428: watchdog POR SLICE — stall real vs slice lento. Um slice DEVE
/// retornar em `SLICE_STALL_US` (30s: s429-lab, piso de 10s — decode m=1 em
/// 8c/WHPX demora ~11s LEGITIMAMENTE; 30s = 2,7× pior caso medido — nenhum slice
/// legítimo demora isso; camadas + OOM interno abortam antes). Estourou =
/// wedge (o poll_slice não voltou) → terminal honesto da prova SEM esperar
/// o deadline de 120s. Detectado no topo do `poll_slice` (roda mesmo com o
/// latch SLICE_BUSY preso noutro core).
/// s429-lab: piso 10s→**30s** — QEMU 8c/WHPX mediu slice LEGÍTIMO de decode
/// do Falcon3-1B em ~11s wall-clock (22 matmuls de ~0,5s num único slice;
/// T+4526→T+5183 a 60Hz = 11,0s = o elapsed do watchdog que disparou FALSO
/// progredindo depois — done id=2 e id=3 vieram na sequência). Premissa da
/// s428 ("nenhum slice legítimo >10s") não vale para decode m=1 em 8 cores.
/// 30s = 2,7× o pior caso medido, ainda 4× abaixo do deadline 120s.
const A2_SLICE_STALL_US: u64 = 30_000_000;
/// TSC do início do slice EM CURSO (0 = nenhum) + contador p/ telemetria.
static A2_SLICE_T0_US: AtomicU64 = AtomicU64::new(0);
static A2_SLICE_N: AtomicU64 = AtomicU64::new(0);
/// Chunked prefill (wedge WHPX 8c T+1910: 1 layer × 512 toks ≈ minutos em
/// soft-float e o watchdog declarava a prova — que nem rodava — wedge
/// terminal). O prompt é fatiado em blocos; cada slice faz ≤1 layer de ≤1
/// chunk (≈ segundos) e o yield entre chunks mostra progresso ao watchdog.
const PREFILL_CHUNK_TOKS: usize = 16;
/// Tunável sem mudar o default provado: `set_prefill_chunk_toks(n)` com clamp
/// 1..=64 (test-hook/calibragem futura). Runtime permanece em 16.
const PREFILL_CHUNK_TOKS_MIN: usize = 1;
const PREFILL_CHUNK_TOKS_MAX: usize = 64;
static PREFILL_CHUNK_TOKS_VAL: AtomicUsize = AtomicUsize::new(PREFILL_CHUNK_TOKS);

/// Lê o chunk efetivo (default 16).
#[inline]
pub fn prefill_chunk_toks() -> usize {
    PREFILL_CHUNK_TOKS_VAL.load(Ordering::Relaxed)
}

/// Test-hook/tuning: ajusta o chunk com clamp 1..=64; retorna o efetivo.
/// Default segue 16 — não subir em runtime sem revalidar o wedge WHPX 8c.
pub fn set_prefill_chunk_toks(n: usize) -> usize {
    let v = n.clamp(PREFILL_CHUNK_TOKS_MIN, PREFILL_CHUNK_TOKS_MAX);
    PREFILL_CHUNK_TOKS_VAL.store(v, Ordering::Relaxed);
    v
}
/// Streak p/ declarar stall em sandbox: 3 polls consecutivos sem progresso.
const SANDBOX_STALL_STREAK_N: u32 = 3;
/// Piso do budget sandbox (120s) e teto do auto-tune (1800s).
const SANDBOX_STALL_US: u64 = 120_000_000;
const SANDBOX_STALL_MAX_US: u64 = 1_800_000_000;
/// Custo do último slice de prefill (auto-tune do budget sandbox: 8×).
static PREFILL_LAST_SLICE_US: AtomicU64 = AtomicU64::new(0);
/// Streak atual sem progresso observável (só sandbox; HW usa terminal direto).
static SANDBOX_STALL_STREAK: AtomicU32 = AtomicU32::new(0);
/// Marca de progresso do job ativo p/ o watchdog: (job, fase+chunk+layer, step).
static WATCH_MARK_A: AtomicU64 = AtomicU64::new(0);
static WATCH_MARK_B: AtomicU64 = AtomicU64::new(0);
static WATCH_MARK_C: AtomicU64 = AtomicU64::new(0);

/// true em sandbox (WHPX/TCG/QEMU) ou probe ainda não rodou (leniente: nunca
/// declara wedge terminal sem evidência; HW real probado segue estrito).
fn eff_sandbox() -> bool {
    if !k_nano::platform_probe::probe_done() {
        return true;
    }
    k_nano::platform_probe::hypervisor().is_sandbox()
}

fn eff_hv_name() -> &'static str {
    if !k_nano::platform_probe::probe_done() {
        return "unknown";
    }
    k_nano::platform_probe::hypervisor().name()
}

/// Budget do watchdog de slice: HW real 500ms→stall 30s fixos; sandbox 120s
/// ou 8× o custo do último slice de prefill (teto 1800s) — TCG lento calibra
/// sozinho, sem declarar wedge no meio de matmul legítimo.
fn slice_stall_budget_us() -> u64 {
    if !eff_sandbox() {
        return A2_SLICE_STALL_US;
    }
    SANDBOX_STALL_US
        .max(PREFILL_LAST_SLICE_US.load(Ordering::Relaxed).saturating_mul(8))
        .min(SANDBOX_STALL_MAX_US)
}

/// Janela do próximo chunk: (offset, n). None = nada restante.
fn chunk_window(total: usize, off: usize) -> Option<(usize, usize)> {
    if off >= total {
        return None;
    }
    Some((off, (total - off).min(prefill_chunk_toks())))
}

/// Marca de progresso do ACTIVE (None = idle/sem lock — sem wedge possível).
fn active_progress_mark() -> Option<(u64, u64, u64)> {
    let g = ACTIVE.try_lock()?;
    let st = g.as_ref()?;
    if matches!(st.phase, Phase::Idle | Phase::Finishing) {
        return None;
    }
    let a = st.job_id;
    let b = ((st.phase as u64) << 56)
        | ((st.prefill_chunks_done & 0x00FF_FFFF) << 32)
        | ((st.prefill_chunk as u64 & 0xFFFF) << 16)
        | (st.prefill_layer as u64 & 0xFFFF);
    Some((a, b, st.step as u64))
}
/// Diagnóstico decisivo rate-limited no topo de `poll_slice`.
static SLICE_POLL_CALLS: AtomicU64 = AtomicU64::new(0);
/// Loga em n==1 (prova que `poll_slice` é chamado) e depois a cada N chamadas.
const SLICE_DIAG_EVERY: u64 = 100_000;
/// Instrumentação s-prefill: enter/exit dos primeiros N steps (máx ~40/boot).
static PREFILL_STEP_LOGGED: AtomicU64 = AtomicU64::new(0);
const PREFILL_STEP_LOG_CAP: u64 = 40;

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

/// Bullet 4 protótipo mínimo: contexto-máquina ao lado do texto.
/// Quando presente no job, o prefill usa os ids diretos (sem framed PT-BR);
/// quando ausente, o path texto-legível atual permanece byte-igual.
/// `slots` = pequena lista KV (cap 8); `ctx_ids` = tensor_ctx (cap 256).
#[derive(Clone, Debug, Default)]
pub struct MachineCtx {
    pub intent_id: u32,
    pub slots: Vec<(u32, u32)>,
    pub ctx_ids: Vec<u32>,
}

impl MachineCtx {
    pub const MAX_SLOTS: usize = 8;
    pub const MAX_CTX: usize = 256;

    /// Encode direto dos ids, sem framed/string: `[bos, intent, k0, v0, ...,
    /// ctx...]`, filtrado por `vocab_size` (mesmo gate `retain` do texto).
    /// Nunca vazio (cai em `[bos]`); nunca chama `bpe::encode`.
    pub fn to_tokens(&self, vocab_size: u32, bos: u32) -> Vec<u32> {
        let vs = vocab_size;
        let b = if vs > 0 { bos.min(vs.saturating_sub(1)) } else { bos };
        let mut out = Vec::new();
        out.push(b);
        if vs == 0 {
            return out;
        }
        if self.intent_id != b && self.intent_id < vs {
            out.push(self.intent_id);
        }
        for (k, v) in self.slots.iter().take(Self::MAX_SLOTS) {
            if *k < vs {
                out.push(*k);
            }
            if *v < vs {
                out.push(*v);
            }
        }
        for &id in self.ctx_ids.iter().take(Self::MAX_CTX) {
            if id < vs {
                out.push(id);
            }
        }
        if out.is_empty() {
            out.push(b);
        }
        out
    }
}

struct ActiveState {
    phase: Phase,
    job_id: u64,
    mode: InferMode,
    reply_topic: String,
    prompt: String,
    /// Bullet 4: struct-máquina opcional (None = texto-legível, byte-igual).
    machine: Option<MachineCtx>,
    tokens: Vec<u32>,
    prompt_len: usize,
    step: usize,
    max_gen: usize,
    max_seq: usize,
    use_bpe: bool,
    is_greeting: bool,
    eos: u32,
    eot: u32,
    recent_u16: Vec<u32>,
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
    /// Chunked prefill (wedge WHPX 8c): offset em `tokens` do chunk atual.
    prefill_chunk: usize,
    /// Chunks concluídos (marcador de progresso p/ o watchdog).
    prefill_chunks_done: u64,
    /// TSC do início do chunk atual.
    prefill_chunk_t0_us: u64,
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
/// (fora do tick). Entra ATRÁS da fila real (sem prioridade/evict): o
/// backpressure honesto vive dentro de submit() (Full CAP=8 / HeapPressure),
/// com retry silencioso no próximo slice. Silencioso sem modelo.
pub fn maybe_submit_a2_proof() -> bool {
    if A2_PROOF_DONE.load(Ordering::Acquire) || A2_PROOF_SUBMITTED.load(Ordering::Acquire) {
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
            // Deadline wall: now==0 (TSC não calibrada) = desarmado (honesto).
            let now = k_nano::tsc::now_us();
            A2_PROOF_DEADLINE_AT_US.store(
                if now == 0 {
                    0
                } else {
                    now.saturating_add(A2_PROOF_DEADLINE_US)
                },
                Ordering::Release,
            );
            k_nano::slog_cortex!("InferQ", "ok", "a2_proof submit id={}", id);
            // Lane Q6: evidência loss-proof (serial é racy sob SMP) — BOOT.LOG
            // via API existente, best-effort, sem eco serial. Só is_proof.
            k_nano::boot_logger::log_quiet(&alloc::format!("a2_proof submit id={}", id));
            true
        }
        // Fila cheia / HeapPressure — tenta de novo no próximo slice.
        Err(SubmitErr::Full) => {
            // Q1: 1× warn (fila real nunca esvazia no boot — prova espera a vez).
            if !A2_SLOT_WAIT_LOGGED.swap(true, Ordering::Relaxed) {
                k_nano::slog_cortex!("InferQ", "warn", "a2_proof waiting for slot (queue full)");
            }
            false
        }
        Err(_) => false,
    }
}

/// Q7: gate do CortexAgent — true só com prova submetida e inacabada.
/// Antes do submit ou após qualquer terminal → false (nunca bloqueia o LLM).
pub fn a2_proof_pending() -> bool {
    A2_PROOF_SUBMITTED.load(Ordering::Acquire) && !A2_PROOF_DONE.load(Ordering::Acquire)
}

/// Q7: todo caminho terminal da prova seta DONE (drop no claim incluído) —
/// sem isso o gate `a2_proof_pending` travaria o LLM para sempre (DoS 1/boot).
#[inline]
fn a2_note_terminal(id: u64) {
    if id == A2_PROOF_ID.load(Ordering::Acquire) {
        A2_PROOF_DONE.store(true, Ordering::Release);
    }
}

/// Enfileira job. Acorda APs via monitor flag.
pub fn submit(prompt: String, mode: InferMode, reply_topic: &str) -> Result<u64, SubmitErr> {
    // Fallback texto-legível intacto: sem struct = caminho idêntico ao anterior.
    submit_with_ctx(prompt, None, mode, reply_topic)
}

/// Bullet 4: enfileira job com struct-máquina opcional ao lado do texto.
/// `machine=Some` dispensa a string framed PT-BR (ids diretos no prefill);
/// `machine=None` = `submit` legado, byte-igual.
pub fn submit_with_ctx(
    prompt: String,
    machine: Option<MachineCtx>,
    mode: InferMode,
    reply_topic: &str,
) -> Result<u64, SubmitErr> {
    // Texto vazio só vale com struct (job-máquina puro); sem struct mantém o
    // `EmptyPrompt` legado.
    if prompt.is_empty() && machine.is_none() {
        return Err(SubmitErr::EmptyPrompt);
    }
    // Fail-closed ANTES de claim/encode: headroom=4MB + encode BPE = #UD/#PF (mesh A).
    let obs = k_nano::allocator::heap_observe();
    // s429-lab: janela ~2030MB é o TETO do bump — perto do teto, os allocs
    // internos (BPE/String/topic) podem estourar e devolver NULL → deref →
    // #PF no AP (heap-fail cr2 baixo pós-grow 2030MB). 48MB não cobre o
    // estado "janela quase cheia"; piso sobe para 128MB (= heap_headroom_low).
    if obs.headroom_mb < 128 {
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
    *slot.machine.lock() = machine;
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
    // SESSION_415 fail-closed: heap bump sem free — no piso crítico, recusar
    // claim novo é melhor que OOM no meio (alloc NULL → deref → #PF → AP hlt).
    // Job fica no slot para retomada quando o heap degradar/liberar.
    if k_nano::allocator::heap_headroom_critical() {
        if !HEAP_FULL_LOGGED.swap(true, Ordering::AcqRel) {
            k_nano::slog_cortex!(
                "InferQ",
                "warn",
                "heap crítico (<{}MB headroom) — claims recusados (fail-closed, job retido)",
                k_nano::allocator::HEAP_CRITICAL_HEADROOM_MB
            );
        }
        return false;
    }
    HEAP_FULL_LOGGED.store(false, Ordering::Release);
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
        // Bullet 4: struct viaja no slot; prompt vazio vale com struct.
        let machine = slot.machine.lock().take();
        if cancelled || (prompt.is_empty() && machine.is_none()) {
            emit_reply(&reply_topic, "[cancelled]");
            a2_note_terminal(id);
            continue;
        }
        // Re-check na claim: headroom pode ter caído desde o submit.
        // s429-lab: 128MB (mesmo piso do submit — janela ~2030MB quase cheia =
        // alloc NULL → #PF no AP durante prefill; 48MB era insuficiente).
        let obs = k_nano::allocator::heap_observe();
        if obs.headroom_mb < 128 {
            k_nano::slog_cortex!(
                "InferQ",
                "warn",
                "claim refuse HeapPressure id={} headroom={}MB",
                id,
                obs.headroom_mb
            );
            emit_reply(&reply_topic, crate::heap_aios::ESCALATE_STATIC_MSG);
            a2_note_terminal(id);
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
            machine,
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
            prefill_chunk: 0,
            prefill_chunks_done: 0,
            prefill_chunk_t0_us: 0,
            is_proof,
            job_t0_us: k_nano::tsc::now_us(),
            prefill_us: 0,
            heap_escalate: false,
        });
        infer_guard_begin();
        emit_msg_start();
        k_nano::slog_cortex!("InferQ", "ok", "claim id={} coarse={}", id, coarse as u8);
        // Wedge WHPX 8c: budget do watchdog por hypervisor na abertura do job —
        // sandbox (WHPX/TCG) tolera matmul legítimo de minutos; HW real 30s.
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "slice_budget hv={} budget_us={} sandbox={} id={}",
            eff_hv_name(),
            slice_stall_budget_us(),
            eff_sandbox() as u8,
            id
        );
        // s419: marco persistente pós-claim (a2_proof stall pós-teto).
        k_nano::boot_logger::log_quiet(&alloc::format!(
            "infer-claim id={} proof={}", id, is_proof as u8
        ));
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
    // P1 lane: logits do job voltam ao pool (cross-job reuse — o warmup do
    // próximo job não paga 512KB; mesmo padrão de generate_speculative:4506).
    if let Some(t) = st.last_logits.take() {
        logits_recycle(t.data);
    }
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
    // ora-1 bullet5: se a cauda gerada colapsou, a resposta NÃO leva gibberish
    // ao TTS/reply — honesto com escalate HITL (tts_buf já foi mutado no push).
    let gen_tail: &[u32] = if st.prompt_len < st.tokens.len() {
        &st.tokens[st.prompt_len..]
    } else {
        &st.tokens
    };
    let gib_final = st.use_bpe && crate::bpe::gibberish_stop(gen_tail);
    let mut out = if text.is_empty() {
        if st.acc_text.is_empty() {
            String::from(NO_MODEL_MSG)
        } else {
            st.acc_text.clone()
        }
    } else {
        String::from(text)
    };
    if gib_final {
        k_nano::slog_cortex!("InferQ", "warn",
            "stop=gibberish done id={} toks={} — HITL escalate (TTS abortado)",
            st.job_id, gen_tail.len());
        k_nano::boot_logger::log_quiet(&alloc::format!(
            "stop=gibberish done id={} toks={} (HITL escalate, TTS abortado)",
            st.job_id, gen_tail.len()
        ));
        st.tts_buf.clear();
        out = String::from("[stop=gibberish — HITL escalate]");
    }
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
        // Lane Q6: mesma evidência no BOOT.LOG (sobrevive ao serial racy).
        k_nano::boot_logger::log_quiet(&alloc::format!(
            "a2_proof done id={} toks={} prefill_us={} decode_us={} total_us={} out_len={}",
            st.job_id,
            job_toks,
            st.prefill_us,
            us,
            total_us,
            out.len()
        ));
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
    // s419 (stall pós-teto): marco persistente pós-`done` — se o boot morrer
    // logo após, a próxima linha do BOOT.LOG (ou a ausência dela) localiza o
    // caminho do stall (TTS/piper, mixer, reply, next tick).
    k_nano::boot_logger::log_quiet(&alloc::format!(
        "post-done id={} out_len={}",
        st.job_id,
        out.len()
    ));
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
    // ora-1 bullet5: gibberish nunca chega ao TTS (stream LLM segue p/ debug,
    // TTS parcial é abortado e o residual é descartado).
    let gib = st.use_bpe && crate::bpe::gibberish_stop(&st.tokens);
    if gib {
        k_nano::slog_cortex!("InferQ", "warn",
            "stop=gibberish step={} toks={} — HITL escalate (TTS parcial abortado)",
            st.step, st.tokens.len());
        st.tts_buf.clear();
        emit_msg_delta(piece);
        return;
    }
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
    // Bullet 4: struct-máquina nunca consulta cues PT-BR (greeting/weather);
    // texto sem struct mantém o comportamento legado byte-igual.
    let has_machine = st.machine.is_some();
    st.is_greeting = if has_machine {
        false
    } else {
        crate::bpe::prompt_is_greeting(&st.prompt)
    };

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

    let vs = model.vocab_size;
    // Bullet 4: com struct, encode direto dos ids (sem framed/string); sem
    // struct, encode de texto idêntico ao legado.
    let mut tokens: Vec<u32> = if let Some(m) = st.machine.as_ref() {
        let bos = if st.use_bpe {
            crate::bpe::bos_id()
        } else {
            crate::cortex::BOS as u32
        };
        m.to_tokens(vs, bos)
    } else if st.use_bpe {
        crate::bpe::encode(&st.prompt)
    } else {
        crate::cortex::Tokenizer::encode(&st.prompt)
            .into_iter()
            .map(|t| t as u32)
            .collect()
    };
    tokens.retain(|&t| t < vs);
    if tokens.is_empty() {
        tokens.push(if st.use_bpe {
            crate::bpe::bos_id().min(vs.saturating_sub(1))
        } else {
            crate::cortex::BOS as u32
        });
    }
    // Struct já é mínima: sem slim (amputaria intent/slots no tail-cut).
    if !has_machine && model.hidden >= 2048 && tokens.len() > 1 {
        // ora-1 bullet1: single-slim (o 2º slim abaixo foi removido; Falcon bypass
        // dentro do slim preserva prompt_len>=20, first=bos, last=assistant).
        tokens = crate::cortex::slim_prompt_tokens_for_heavy(&tokens, st.use_bpe);
    }
    st.prompt_len = tokens.len();

    let max_seq = plan.ctx_cap.min(if model.hidden >= 2048 {
        model.max_seq.min(512)
    } else {
        model.max_seq.min(64)
    });
    st.max_seq = max_seq;
    // ora-1: duplo-slim removido (single-slim acima; force_slim não re-slimma).
    if tokens.len() > max_seq {
        let keep = max_seq.max(1);
        if has_machine {
            // Struct: preserva a cabeça (bos/intent/slots), corta a cauda.
            tokens.truncate(keep);
        } else {
            tokens = tokens[tokens.len() - keep..].to_vec();
        }
    }
    st.prompt_len = tokens.len();
    st.max_gen = plan.max_gen;
    // Bullet 4: observabilidade — `machine_ctx=1` struct, `=0` texto-legível.
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "prefill_setup machine_ctx={} id={} prompt_len={} first={} last={}",
        has_machine as u8,
        st.job_id,
        st.prompt_len,
        tokens.first().copied().unwrap_or(0xFFFF),
        tokens.last().copied().unwrap_or(0xFFFF)
    );
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

    // Chunked prefill: o embed é por chunk dentro do slice (cada chunk mostra
    // progresso ao watchdog); aqui só ancora o cursor no chunk 0.
    st.tokens = tokens;
    st.cache = Some(cache);
    st.prefill_x = None;
    st.prefill_mask = None;
    st.prefill_layer = 0;
    st.prefill_new_len = 0;
    st.prefill_start_pos = 0;
    st.prefill_total_seq = 0;
    st.prefill_chunk = 0;
    st.prefill_chunks_done = 0;
    st.prefill_chunk_t0_us = 0;
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
    // SESSION_420 (residual s419): gate PROATIVO de headroom ANTES do slice.
    // Os gates críticos (64MB, SESSION_415) só disparam ENTRE slices (topo do
    // poll_slice); o auto-grow acontece DENTRO do slice (KV/mask/logits em
    // apply_one_layer) e cruza o teto ~2030MB (wrap 2^64 do bump) antes do
    // piso crítico ser re-checado → alloc NULL no meio do slice → #PF → hlt
    // no AP (BOOT.LOG T+27121: auto-grow 1792→2030MB em prefill id=2).
    // Piso 128MB = 1 slice de margem: termina o job honesto (HITL) antes do
    // heap esgotar — recusa custa um job, OOM custa um core.
    if k_nano::allocator::heap_headroom_low() {
        let obs = k_nano::allocator::heap_observe();
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "prefill refuse headroom_low id={} layer={} used={}MB headroom={}MB",
            st.job_id,
            st.prefill_layer,
            obs.used_mb,
            obs.headroom_mb
        );
        k_nano::boot_logger::log_quiet(&alloc::format!(
            "prefill refuse headroom_low id={} layer={} headroom_mb={}",
            st.job_id,
            st.prefill_layer,
            obs.headroom_mb
        ));
        a2_refuse(st, "headroom_low");
        finish_job(st, "[heap: headroom baixo no prefill — fail-closed HITL]");
        return;
    }
    // Lane D-cortex/Q4: fail-closed ANTES do slice pesado (só a prova; a
    // fila real nunca aborta aqui). Só sob risco real: com ap_pollable o
    // slice corre no AP idle (fora do BUSY/budget — lenta é inofensiva);
    // sem APs (fallback BSP) cada slice >500ms = 1 overrun do infer_worker
    // e o 4º pausa o agente, com o ACTIVE preso até Crashed.
    // Q4: ap_pollable() ao vivo — roteador fiel (hermes pula poll_slice no
    // BSP com APs vivos; try_infer_poll_slice exige ap_pollable no AP).
    let ap_live = k_nano::smp::ap_pollable();
    if st.is_proof
        && !ap_live
        && a2_should_abort_slice(
            A2_SLOW_SLICES.load(Ordering::Relaxed),
            k_nano::smp::ui_yield_infer(),
        )
    {
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "a2_proof refuse watchdog_would_pause id={} slow={} ui_yield={} ap={}",
            st.job_id,
            A2_SLOW_SLICES.load(Ordering::Relaxed),
            k_nano::smp::ui_yield_infer() as u8,
            ap_live as u8
        );
        finish_job(
            st,
            "[a2_proof abort watchdog_would_pause — BSP sem AP-IDT, slice>500ms pausaria infer_worker; HITL]",
        );
        return;
    }
    // SESSION_413: TSC DEPOIS de qualquer log — `t_slice0` é capturado após
    // o `prefill_step enter` abaixo (o slog serial ~1,4ms contaminava o
    // `slice_us` e o instrumento media a si mesmo). Lock-wait/model-absent
    // ficam fora da medida: ela cobre só o compute do slice.
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

    // Chunked prefill: sem x pendente = início do próximo chunk (embed de
    // ≤16 toks + yield; o watchdog vê 1 linha/chunk e nunca declara wedge no
    // meio de prefill legítimo). Roda ANTES dos reborrows x/mask/cache.
    if st.prefill_x.is_none() {
        let total = st.tokens.len();
        let Some((off, n)) = chunk_window(total, st.prefill_chunk) else {
            drop(guard);
            a2_refuse(st, "chunk_oob");
            finish_job(st, "[prefill chunk OOB — HITL escalate]");
            return;
        };
        let chunk_ids: Vec<u32> = st.tokens[off..off + n].to_vec();
        let cache_ref = match st.cache.as_ref() {
            Some(c) => c,
            None => {
                drop(guard);
                a2_refuse(st, "model_absent");
                finish_job(st, NO_MODEL_MSG);
                return;
            }
        };
        let (x0, mask0, sp, nl, ts) = model.embed_for_kv(&chunk_ids, cache_ref);
        if !x0.is_valid() || !mask0.is_valid() || mask0.shape != (nl, ts) || nl == 0 {
            k_nano::slog_cortex!(
                "InferQ",
                "fail",
                "embed/mask refuse id={} chunk_off={} x={:?} mask={:?}",
                st.job_id,
                off,
                x0.shape,
                mask0.shape
            );
            drop(guard);
            a2_refuse(st, "embed_mask");
            finish_job(st, "[heap: embed/mask refuse — HITL escalate]");
            return;
        }
        st.prefill_x = Some(x0);
        st.prefill_mask = Some(mask0);
        st.prefill_start_pos = sp;
        st.prefill_new_len = nl;
        st.prefill_total_seq = ts;
        st.prefill_layer = 0;
        st.prefill_chunk_t0_us = k_nano::tsc::now_us();
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "prefill_chunk id={} n={}/{} (chunk {}/{})",
            st.job_id,
            off + n,
            total,
            off / prefill_chunk_toks() + 1,
            total.div_ceil(prefill_chunk_toks())
        );
    }

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

    // Instrumentação s-prefill: prova em qual layer o slice ENTRA (o EXIT
    // correspondente vem depois do loop; ausência = hang dentro da layer).
    // Cap 40/boot — fora do hot path após o warmup (não precisa de 1/20).
    let step_log = PREFILL_STEP_LOGGED.fetch_add(1, Ordering::Relaxed) < PREFILL_STEP_LOG_CAP;
    if step_log {
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "prefill_step enter layer={}/{} proof={}",
            st.prefill_layer,
            n_layers,
            st.is_proof as u8
        );
    }
    // SESSION_413: TSC DEPOIS do log de entrada (ver comentário acima).
    let t_slice0 = k_nano::tsc::now_us();

    while st.prefill_layer < n_layers && applied < layers_per_slice {
        let li = st.prefill_layer;
        st.prefill_layer += 1;
        if soft_stride > 1 && (li % soft_stride) != 0 {
            // SESSION_351/359: pad KV — OOM aborta job (não desalinha silenciosamente).
            // P0: `checked_mul` fail-fast ANTES de alocar — `Tensor::new`
            // recusa sozinho (overflow → shape (0,0) + `is_valid()==false`),
            // mas o pre-check evita `try_reserve` em tamanho com wrap.
            let kd = cache.k_dim();
            if st.prefill_new_len.checked_mul(kd).is_none() {
                pad_oom = true;
                break;
            }
            let zk = Tensor::new((st.prefill_new_len, kd));
            let zv = Tensor::new((st.prefill_new_len, kd));
            if zk.is_valid() && zv.is_valid() {
                if !cache.append(li, &zk, &zv) {
                    // s439: página TALC indisponível — refuse honesto do pad
                    // (mesmo fail-closed do OOM: não desalinha silenciosamente).
                    pad_oom = true;
                    break;
                }
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
    // Auto-tune do budget sandbox (8× o último slice; piso 120s no leitor).
    PREFILL_LAST_SLICE_US.store(slice_us, Ordering::Relaxed);
    if step_log {
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "prefill_step exit layer={} applied={} us={}",
            st.prefill_layer,
            applied,
            slice_us
        );
    }
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
    // P0 hot path: gating 1/20 via TELEM_PREFILL_SLICES (mesmo padrão do
    // `log_it` em parallel_matmul) — refuses honestos (headroom/pad_oom)
    // acima NUNCA são gated. TELEM já contou este slice (+1).
    if slice_us > 2_000_000 && TELEM_PREFILL_SLICES.load(Ordering::Relaxed) % 20 == 1 {
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

    // Chunk completo (todas as layers): commit do KV do chunk. Com mais
    // chunks, recicla o x e cede — o próximo slice embebe o próximo chunk
    // (progresso visível ao watchdog a cada chunk).
    cache.advance(st.prefill_new_len);
    st.prefill_chunks_done += 1;
    st.prefill_chunk += st.prefill_new_len;
    if st.prefill_chunk < st.tokens.len() {
        let dead = core::mem::replace(&mut *x, Tensor::zero((0, 0)));
        logits_recycle(dead.data);
        st.prefill_x = None;
        st.prefill_mask = None;
        drop(guard);
        return;
    }

    // Finalize (último chunk)
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
    // P3 (ADR-0111): consumo REAL do KV INT8 vs o que seria em f32 (a evidencia
    // do ADR: 604 MB no 1B ctx 4096). 1x/job, no fim do prefill.
    let kv_f32 = crate::cortex::kv_bytes_f32(n_layers, new_len, cache.k_dim());
    k_nano::slog_cortex!(
        "KV",
        "ok",
        "kv_mem id={} ctx={} int8_used={}KB int8_alloc={}KB f32_would_be={}KB",
        st.job_id,
        new_len,
        cache.bytes_used() / 1024,
        cache.bytes_allocated() / 1024,
        kv_f32 / 1024
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
            // ora-1 bullet2: u32 (u16 truncava ids>64k).
            st.recent_u16.push(last);
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
    // SESSION_420: mesma classe no decode — o forward_with_kv anexa KV (cresce
    // o bump dentro do slice). Sob piso proativo, terminar honesto com o que
    // já foi gerado (payload parcial) em vez de OOM no meio do forward.
    if k_nano::allocator::heap_headroom_low() {
        k_nano::boot_logger::log_quiet(&alloc::format!(
            "decode refuse headroom_low id={} step={} headroom_mb={}",
            st.job_id,
            st.step,
            k_nano::allocator::heap_observe().headroom_mb
        ));
        a2_refuse(st, "headroom_low");
        let acc = st.acc_text.clone();
        finish_job(st, if acc.is_empty() { "[heap: headroom baixo no decode — fail-closed HITL]" } else { &acc });
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

    // Lane A2: decode_us por token. TSC DEPOIS de qualquer log (SESSION_413:
    // lock-wait/model-absent ficam fora da medida — ela cobre argmax+forward)
    // e AMOSTRADO fora do hot path: a prova mede todo token (1 token); jobs
    // regulares só 1/32 ((step & 0x1F)==0) — o tok/s oficial é job-level
    // (DECODE_T0_US → finish_job), não este instrumento.
    let sample = st.is_proof || (st.step & 0x1F) == 0;
    let t_d0 = if sample { k_nano::tsc::now_us() } else { 0 };

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

    let next: u32 = if st.use_bpe {
        // ora-1 bullet3: argmax puro (sem score_piece/weather/coherence).
        crate::cortex::argmax_row_hf_vocab(last_logits, 0, &st.recent_u16)
    } else {
        let prev_u16 = st.recent_u16.last().copied().map(|v| v as u16);
        crate::cortex::argmax_row_char_vocab(last_logits, 0, prev_u16)
    };

    if next == st.eos || next == st.eot {
        let acc = st.acc_text.clone();
        drop(guard);
        finish_job(st, &acc);
        return;
    }

    st.tokens.push(next);
    // ora-1 bullet2: recent em u32, sem `as u16` (truncava 131000→65464).
    st.recent_u16.push(next);
    if st.recent_u16.len() > 4 {
        st.recent_u16.remove(0);
    }
    st.step += 1;
    TELEM_DECODE_TOKENS.fetch_add(1, Ordering::Relaxed);
    DECODE_JOB_TOKS.fetch_add(1, Ordering::Relaxed);

    let piece = if st.use_bpe {
        crate::bpe::decode(&[next])
    } else {
        let nu = next as u16; // char vocab <256, trunc seguro
        crate::cortex::Tokenizer::decode(&[nu])
    };

    if st.step < st.max_gen && st.tokens.len() < st.max_seq {
        // P1 lane: backing do token anterior volta ao pool ANTES do forward
        // (o unembed tenta `logits_take` primeiro — pool vazio = 512KB/token
        // no bump, OOM SESSION_415-417). Zero alloc/decode após o warmup.
        logits_recycle(core::mem::take(&mut last_logits.data));
        let (new_hidden, new_logits) = model.forward_with_kv(&[next], cache);
        st.last_hidden = Some(new_hidden);
        *last_logits = new_logits;
    }
    drop(guard);

    // TSC DEPOIS do forward e ANTES do push_delta (EventBus/TTS fora da
    // medida — SESSION_413: o instrumento não se mede).
    let t_dend = if t_d0 != 0 { k_nano::tsc::now_us() } else { 0 };

    push_delta(st, &piece);
    // Lane A2: decode_us explícito (argmax + forward do token).
    if st.is_proof {
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "a2_proof decode_one id={} step={} tok={} decode_us={} piece_len={}",
            st.job_id,
            st.step,
            next,
            t_dend.saturating_sub(t_d0),
            piece.len()
        );
    } else if sample && t_dend != 0 {
        // Telemetria amortizada 1/32 — única linha serial por N tokens.
        k_nano::slog_cortex!(
            "InferQ",
            "ok",
            "decode_sample id={} step={} tok={} us={}",
            st.job_id,
            st.step,
            next,
            t_dend.saturating_sub(t_d0)
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
    // SESSION_359: coarse = generate() bloqueante sem yield — barge-in só
    // observável após o retorno. Log honesto; se cancelou durante generate, descarta.
    k_nano::slog_cortex!(
        "InferQ",
        "warn",
        "coarse generate id={} (uncancellable mid-call)",
        st.job_id
    );
    // TSC DEPOIS do log (SESSION_413) — o warn serial não entra no wall do job.
    DECODE_T0_US.store(k_nano::tsc::now_us(), Ordering::Release);
    // Bullet 4: coarse (modelo não-Transformer) só fala texto — struct vira
    // fallback de texto, honesto e explícito (nunca silencioso).
    if st.machine.is_some() {
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "coarse machine_ctx=1 id={} (ids ignorados, texto fallback — HITL)",
            st.job_id
        );
    }
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

/// Diagnóstico decisivo rate-limited + deadline wall da prova. Roda no topo de
/// `poll_slice` (antes do latch), então o deadline dispara mesmo com o slice
/// preso noutro core. Não bloqueia em `ACTIVE` (try_lock — o lock pode estar
/// segurado por outro core no meio do slice).
fn slice_diag_and_proof_deadline() {
    // Deadline wall da prova: nunca deixa `a2_proof_pending` preso para sempre.
    if a2_proof_pending() {
        let at = A2_PROOF_DEADLINE_AT_US.load(Ordering::Acquire);
        let now = k_nano::tsc::now_us();
        if at != 0 && now != 0 && now >= at {
            let id = A2_PROOF_ID.load(Ordering::Acquire);
            if !A2_PROOF_TIMEOUT_LOGGED.swap(true, Ordering::Relaxed) {
                k_nano::slog_cortex!(
                    "InferQ",
                    "warn",
                    "a2_proof timeout id={} elapsed_us={}",
                    id,
                    now.saturating_sub(at.saturating_sub(A2_PROOF_DEADLINE_US))
                );
            }
            a2_note_terminal(id);
            return; // terminal já fecha tudo — watchdog não precisa rodar
        }
        // s428: watchdog POR SLICE — stall real vs slice lento. O T0 do slice
        // em curso foi marcado ANTES do latch; se `poll_slice` não voltou em
        // >STALL, é wedge (lost wakeup/lock preso/#PF silencioso),
        // não slice lento. HW real: terminal honesto imediato.
        // Sandbox (WHPX/TCG): progress-aware e nunca terminal da fila —
        // matmul legítimo de minutos (Falcon3-3B 512×3072×3072 ≈14s cada)
        // não pode matar a prova que nem estava rodando (wedge T+1910).
        let t0 = A2_SLICE_T0_US.load(Ordering::Acquire);
        if t0 != 0 && now != 0 {
            let budget = slice_stall_budget_us();
            let slice_elapsed = now.saturating_sub(t0);
            if slice_elapsed > budget {
                if eff_sandbox() {
                    // Progresso desde a última checagem = prefill/decode vivo:
                    // re-arma e nunca declara wedge no meio de trabalho legítimo.
                    let mark = active_progress_mark();
                    let last = (
                        WATCH_MARK_A.load(Ordering::Relaxed),
                        WATCH_MARK_B.load(Ordering::Relaxed),
                        WATCH_MARK_C.load(Ordering::Relaxed),
                    );
                    let progressed = match (mark, last) {
                        (Some(m), l) => m != l,
                        // Idle/sem lock = nada preso: sem wedge possível.
                        (None, _) => true,
                    };
                    if progressed {
                        if let Some((a, b, c)) = mark {
                            WATCH_MARK_A.store(a, Ordering::Relaxed);
                            WATCH_MARK_B.store(b, Ordering::Relaxed);
                            WATCH_MARK_C.store(c, Ordering::Relaxed);
                        }
                        SANDBOX_STALL_STREAK.store(0, Ordering::Relaxed);
                        A2_SLICE_T0_US.store(now, Ordering::Release);
                        return;
                    }
                    let streak =
                        SANDBOX_STALL_STREAK.fetch_add(1, Ordering::Relaxed) + 1;
                    if streak < SANDBOX_STALL_STREAK_N {
                        if streak == 1 {
                            k_nano::slog_cortex!(
                                "InferQ",
                                "warn",
                                "slice_stall hv={} elapsed_us={} (budget={}us) action=watch streak={}/{} — fila intacta",
                                eff_hv_name(),
                                slice_elapsed,
                                budget,
                                streak,
                                SANDBOX_STALL_STREAK_N
                            );
                        }
                        return;
                    }
                    // Stall persistente após N checagens sem progresso: aborta
                    // o JOB com stop honesto (sem TTS), nunca a fila/prova.
                    let mut jid = 0u64;
                    let mut aborted = false;
                    if let Some(mut g) = ACTIVE.try_lock() {
                        if let Some(st) = g.as_mut() {
                            if !matches!(st.phase, Phase::Idle | Phase::Finishing) {
                                jid = st.job_id;
                                finish_job(st, "[stop=slice_budget — HITL escalate]");
                                aborted = true;
                            }
                        }
                    }
                    k_nano::slog_cortex!(
                        "InferQ",
                        "fail",
                        "slice_stall hv={} id={} elapsed_us={} (budget={}us) stop=slice_budget action={} — fila intacta",
                        eff_hv_name(),
                        jid,
                        slice_elapsed,
                        budget,
                        if aborted { "abort_job" } else { "watch_locked" }
                    );
                    if aborted {
                        k_nano::boot_logger::log_quiet(&alloc::format!(
                            "stop=slice_budget id={} (stall sandbox, job abortado, fila intacta)",
                            jid
                        ));
                    }
                    SANDBOX_STALL_STREAK.store(0, Ordering::Relaxed);
                    A2_SLICE_T0_US.store(now, Ordering::Release);
                    return;
                }
                let id = A2_PROOF_ID.load(Ordering::Acquire);
                let n = A2_SLICE_N.load(Ordering::Relaxed);
                if !A2_PROOF_TIMEOUT_LOGGED.swap(true, Ordering::Relaxed) {
                    k_nano::slog_cortex!(
                        "InferQ",
                        "fail",
                        "a2_proof slice_stall id={} slice_n={} elapsed_us={} (budget={}us) — wedge, terminal",
                        id,
                        n,
                        slice_elapsed,
                        A2_SLICE_BUDGET_US
                    );
                }
                A2_SLICE_T0_US.store(0, Ordering::Release);
                a2_note_terminal(id);
                return;
            }
        }
        // Slices fluindo: zera o streak sandbox.
        SANDBOX_STALL_STREAK.store(0, Ordering::Relaxed);
    }
    // Diagnóstico: prova qual gate travou no próximo run. n==1 sempre.
    let n = SLICE_POLL_CALLS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    if n == 1 || n % SLICE_DIAG_EVERY == 0 {
        let active = match ACTIVE.try_lock() {
            Some(g) => g.is_some(),
            None => true, // lock segurado = slice em curso noutro core
        };
        k_nano::slog_cortex!(
            "InferQ",
            "warn",
            "poll_slice diag n={} busy={} ui_yield={} ap={} active={}",
            n,
            SLICE_BUSY.load(Ordering::Relaxed) as u8,
            k_nano::smp::ui_yield_infer() as u8,
            k_nano::smp::ap_pollable() as u8,
            active as u8
        );
    }
}

/// Executa no máximo 1 slice. Seguro chamar do InferWorker ou AP idle (sem AGENT_TICK_BUSY).
/// Retorna true se havia trabalho (claim ou decode).
pub fn poll_slice() -> bool {
    // Deadline/diagnóstico antes do latch: independe do SLICE_BUSY.
    slice_diag_and_proof_deadline();

    if SLICE_BUSY.swap(true, Ordering::AcqRel) {
        return false;
    }
    // RAII: nenhum early-return (ou panic contido) deixa o latch setado.
    struct SliceGuard;
    impl Drop for SliceGuard {
        fn drop(&mut self) {
            SLICE_BUSY.store(false, Ordering::Release);
        }
    }
    let _busy = SliceGuard;

    // s428: marca o T0 do slice em curso — o watchdog (no topo do próximo
    // poll_slice) mede contra ISTO. Limpo no fim do poll_slice (retorno =
    // slice terminou; slice lento é contabilizado no budget, não aqui).
    A2_SLICE_T0_US.store(k_nano::tsc::now_us(), Ordering::Release);
    A2_SLICE_N.fetch_add(1, Ordering::Relaxed);

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
            // SESSION_415 fail-closed no job EM CURSO: bump heap sem free —
            // decode/prefill sob headroom crítico termina o job honestamente
            // (payload parcial) em vez de alocar NULL → deref → #PF → AP hlt
            // (2 boots: cr2=0x50 em MemoryStore::tick_advance pós-teto).
            if !matches!(st.phase, Phase::Finishing | Phase::Idle)
                && k_nano::allocator::heap_headroom_critical()
            {
                if !HEAP_FULL_LOGGED.swap(true, Ordering::AcqRel) {
                    k_nano::slog_cortex!(
                        "InferQ",
                        "warn",
                        "heap crítico no job em curso — finalizando {} id={} (fail-closed, payload parcial)",
                        "job",
                        ACTIVE_ID.load(Ordering::Relaxed)
                    );
                }
                st.phase = Phase::Finishing;
            }
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

    // s428: slice terminou — limpa o T0 (stall = só existe com slice EM CURSO).
    A2_SLICE_T0_US.store(0, Ordering::Release);

    // s429-lab: o deadline da prova é NO-PROGRESS, não wall-clock — cada slice
    // concluído é progresso e re-arma o deadline (QEMU 8c/WHPX mediu prova
    // LEGÍTIMA de 58s+fila > 120s wall-clock: "a2_proof timeout id=2" com    // matmuls progredindo e `a2_proof done id=2` depois). Só NENHUM slice em    // 120s (wedge total, nem poll roda) é falha real.
    if did && a2_proof_pending() {
        let now = k_nano::tsc::now_us();
        if now != 0 {
            A2_PROOF_DEADLINE_AT_US
                .store(now.saturating_add(A2_PROOF_DEADLINE_US), Ordering::Release);
        }
    }

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
        A2_SLOT_WAIT_LOGGED.store(false, Ordering::Release);
        A2_PROOF_DEADLINE_AT_US.store(0, Ordering::Release);
        A2_PROOF_TIMEOUT_LOGGED.store(false, Ordering::Release);
        A2_SLICE_T0_US.store(0, Ordering::Release);
        A2_SLICE_N.store(0, Ordering::Release);
        SLICE_POLL_CALLS.store(0, Ordering::Release);
        PREFILL_LAST_SLICE_US.store(0, Ordering::Release);
        PREFILL_CHUNK_TOKS_VAL.store(PREFILL_CHUNK_TOKS, Ordering::Release);
        SANDBOX_STALL_STREAK.store(0, Ordering::Release);
        WATCH_MARK_A.store(0, Ordering::Release);
        WATCH_MARK_B.store(0, Ordering::Release);
        WATCH_MARK_C.store(0, Ordering::Release);
        for i in 0..QUEUE_CAP {
            slots()[i].occupied.store(false, Ordering::Release);
            *slots()[i].machine.lock() = None;
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
    fn a2_proof_happy_path_host() {
        // Lane Q6 host-repro: submit→claim(is_proof)→slices→finish com modelo
        // demo minúsculo (TransformerModel::new, 64 hidden/4 layers — nunca o
        // 3B). Se FALHAR, o bug é na função (não no serial).
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        // AP-path simulado (abort jamais armado): restaura ao fim.
        let prev_ap = k_nano::smp::ap_pollable();
        k_nano::smp::set_ap_pollable(true);
        crate::cortex::set_model(alloc::boxed::Box::new(crate::cortex::TransformerModel::new()));
        let rx = k_nano::EVENT_BUS.subscribe(TOPIC_LLM_RESPONSE);
        while rx.try_receive().is_some() {}
        assert!(maybe_submit_a2_proof(), "submit da prova com modelo demo");
        assert!(A2_PROOF_SUBMITTED.load(Ordering::Relaxed));
        let pid = A2_PROOF_ID.load(Ordering::Relaxed);
        assert!(pid != 0, "id da prova");
        assert!(try_claim_into_active(), "claim acha o job");
        {
            let g = ACTIVE.lock();
            let st = g.as_ref().expect("active após claim");
            assert!(st.is_proof, "claim marca is_proof pelo id");
            assert_eq!(st.job_id, pid);
        }
        let mut n = 0u32;
        while !A2_PROOF_DONE.load(Ordering::Acquire) && n < 500 {
            poll_slice();
            n += 1;
        }
        assert!(A2_PROOF_DONE.load(Ordering::Acquire), "prova termina no host");
        // Abort jamais armado no path AP (slices de µs, sem slow).
        assert_eq!(
            A2_SLOW_SLICES.load(Ordering::Relaxed),
            0,
            "sem slow_slice no host"
        );
        // Reply final existe e NÃO é abort/refuse.
        let mut last: Vec<u8> = Vec::new();
        while let Some(evt) = rx.try_receive() {
            last = evt.payload;
        }
        let text = String::from_utf8_lossy(&last);
        assert!(!text.contains("watchdog_would_pause"), "abort não disparou: {}", text);
        assert!(!text.contains("heap escalate"), "sem escalate no host: {}", text);
        // Limpa globals (outros testes esperam defaults).
        crate::cortex::clear_model();
        crate::cortex::GENERATION_GAPS_RESOLVED
            .store(false, core::sync::atomic::Ordering::Release);
        k_nano::smp::set_ap_pollable(prev_ap);
        drain_infer_queue_statics();
    }

    #[test]
    fn a2_proof_pending_gate_never_sticks() {
        // Q7: gate só true com prova submetida e inacabada; todo terminal libera.
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        assert!(!a2_proof_pending(), "antes do submit nunca bloqueia");
        A2_PROOF_SUBMITTED.store(true, Ordering::Release);
        A2_PROOF_ID.store(42, Ordering::Release);
        assert!(a2_proof_pending(), "submetida e inacabada bloqueia");
        a2_note_terminal(999);
        assert!(a2_proof_pending(), "id estranho nao libera");
        a2_note_terminal(42);
        assert!(!a2_proof_pending(), "drop terminal libera");
        A2_PROOF_DONE.store(false, Ordering::Release);
        assert!(a2_proof_pending(), "re-armado bloqueia de novo");
        A2_PROOF_DONE.store(true, Ordering::Release);
        assert!(!a2_proof_pending(), "DONE libera");
        drain_infer_queue_statics();
    }

    #[test]
    fn a2_proof_deadline_releases_gate() {
        // Patch 4: deadline wall nunca deixa a prova mutar o CortexAgent para sempre.
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        assert!(k_nano::tsc::now_us() != 0, "host TSC calibrada");
        A2_PROOF_SUBMITTED.store(true, Ordering::Release);
        A2_PROOF_DONE.store(false, Ordering::Release);
        A2_PROOF_ID.store(7, Ordering::Release);
        A2_PROOF_DEADLINE_AT_US.store(1, Ordering::Release); // deadline no passado
        assert!(a2_proof_pending(), "armado bloqueia antes do check");
        slice_diag_and_proof_deadline();
        assert!(!a2_proof_pending(), "deadline estourado libera o gate");
        assert!(A2_PROOF_DONE.load(Ordering::Acquire));
        drain_infer_queue_statics();
    }

    #[test]
    fn headroom_low_gate_above_critical() {
        // SESSION_420: piso proativo do prefill > piso crítico — o gate do
        // slice dispara ANTES do crítico (margem de 1 slice), e low ⊇ critical.
        assert!(
            k_nano::allocator::HEAP_PREFILL_HEADROOM_MB
                > k_nano::allocator::HEAP_CRITICAL_HEADROOM_MB
        );
        if k_nano::allocator::heap_headroom_critical() {
            assert!(k_nano::allocator::heap_headroom_low());
        }
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

    #[test]
    fn s428_deadline_120s_e_stall_10s() {
        // Piso documentado: idle ~160ms/slice × 22 layers ≈ 8s (SESSION_420);
        // s429-lab: stall 30s — slice LEGÍTIMO de decode m=1 em 8c/WHPX mediu
        // ~11s (falso positivo em 10s); 30s = 2,7× pior caso. Deadline 120s
        // virou NO-PROGRESS (re-armado a cada slice concluído).
        assert_eq!(A2_PROOF_DEADLINE_US, 120_000_000);
        assert_eq!(A2_SLICE_STALL_US, 30_000_000);
        // Ordem: stall dispara ANTES do deadline (por slice, não global).
        assert!(A2_SLICE_STALL_US < A2_PROOF_DEADLINE_US);
    }

    #[test]
    fn s428_slice_stall_terminal_no_host() {
        // Wedge WHPX 8c: no host (probe ausente = leniente/sandbox) o piso é
        // 120s — T0 40s estagnado NÃO é stall (era terminal nos 30s do HW).
        // Fila/prova intactas; terminal HW segue byte-idêntico no metal.
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        assert!(k_nano::tsc::now_us() != 0, "host TSC calibrada");
        assert!(eff_sandbox(), "host sem probe = leniente");
        A2_PROOF_SUBMITTED.store(true, Ordering::Release);
        A2_PROOF_DONE.store(false, Ordering::Release);
        A2_PROOF_ID.store(9, Ordering::Release);
        // deadline no FUTURO (prova NÃO estourou o prazo global)
        let now = k_nano::tsc::now_us();
        A2_PROOF_DEADLINE_AT_US.store(now + A2_PROOF_DEADLINE_US, Ordering::Release);
        // slice "em curso" há 40s (> 30s HW, < 120s sandbox = legítimo)
        A2_SLICE_T0_US.store(now.saturating_sub(40_000_000), Ordering::Release);
        assert!(a2_proof_pending(), "armado antes do watchdog");
        slice_diag_and_proof_deadline();
        assert!(a2_proof_pending(), "sandbox: 40s < 120s, prova continua");
        assert!(!A2_PROOF_DONE.load(Ordering::Acquire));
        // T0 200s estagnado com ACTIVE vazio (=idle): re-arma, sem terminal.
        let now2 = k_nano::tsc::now_us();
        A2_SLICE_T0_US.store(now2.saturating_sub(200_000_000), Ordering::Release);
        slice_diag_and_proof_deadline();
        assert!(a2_proof_pending(), "idle nunca é wedge — fila intacta");
        assert!(!A2_PROOF_DONE.load(Ordering::Acquire));
        drain_infer_queue_statics();
    }

    #[test]
    fn sandbox_stall_persistente_aborta_job_nao_fila() {
        // Stall persistente após N=3 checagens sem progresso: aborta o JOB com
        // `stop=slice_budget` (sem TTS); prova/fila seguem intactas.
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        assert!(k_nano::tsc::now_us() != 0, "host TSC calibrada");
        let prev_ap = k_nano::smp::ap_pollable();
        k_nano::smp::set_ap_pollable(true);
        crate::cortex::set_model(alloc::boxed::Box::new(crate::cortex::TransformerModel::new()));
        let rx = k_nano::EVENT_BUS.subscribe(TOPIC_LLM_RESPONSE);
        while rx.try_receive().is_some() {}
        A2_PROOF_SUBMITTED.store(true, Ordering::Release);
        A2_PROOF_DONE.store(false, Ordering::Release);
        A2_PROOF_ID.store(999, Ordering::Release);
        let now = k_nano::tsc::now_us();
        A2_PROOF_DEADLINE_AT_US.store(now + A2_PROOF_DEADLINE_US, Ordering::Release);
        let jid = submit(String::from("ola"), InferMode::Plain, TOPIC_LLM_RESPONSE)
            .expect("submit");
        assert!(try_claim_into_active(), "claim");
        // Trava o job em Prefilling sem progresso (chunk/layer congelados).
        {
            let mut g = ACTIVE.lock();
            let st = g.as_mut().expect("active");
            st.phase = Phase::Prefilling;
        }
        // 1ª checagem com T0 200s estagnado: marca nova = progresso → re-arma.
        A2_SLICE_T0_US.store(k_nano::tsc::now_us().saturating_sub(200_000_000), Ordering::Release);
        slice_diag_and_proof_deadline();
        assert!(a2_proof_pending(), "1ª fire registra a marca, prova intacta");
        // Mais 3 checagens sem progresso (T0 re-estagnado de propósito).
        for _ in 0..3 {
            A2_SLICE_T0_US.store(k_nano::tsc::now_us().saturating_sub(200_000_000), Ordering::Release);
            slice_diag_and_proof_deadline();
        }
        // Job abortado com stop honesto; prova segue pendente (fila intacta).
        assert!(a2_proof_pending(), "prova nunca terminal em sandbox");
        assert!(!A2_PROOF_DONE.load(Ordering::Acquire));
        {
            let g = ACTIVE.lock();
            assert!(
                g.as_ref().map(|s| s.phase == Phase::Idle).unwrap_or(false),
                "job abortado"
            );
        }
        let mut last: Vec<u8> = Vec::new();
        while let Some(evt) = rx.try_receive() {
            last = evt.payload;
        }
        let text = String::from_utf8_lossy(&last);
        assert!(text.contains("stop=slice_budget"), "stop honesto no reply: {}", text);
        let _ = jid;
        crate::cortex::clear_model();
        k_nano::smp::set_ap_pollable(prev_ap);
        drain_infer_queue_statics();
    }

    #[test]
    fn s428_slice_lento_nao_dispara_stall() {
        // Distinguir os dois fenômenos: slice EM CURSO dentro do stall (mesmo
        // que lento) NÃO terminal a prova — o deadline global cuida do resto.
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        A2_PROOF_SUBMITTED.store(true, Ordering::Release);
        A2_PROOF_DONE.store(false, Ordering::Release);
        A2_PROOF_ID.store(11, Ordering::Release);
        let now = k_nano::tsc::now_us();
        A2_PROOF_DEADLINE_AT_US.store(now + A2_PROOF_DEADLINE_US, Ordering::Release);
        // slice em curso há 1s (> budget 500ms = lento, < stall 10s = não wedge)
        A2_SLICE_T0_US.store(now - 1_000_000, Ordering::Release);
        slice_diag_and_proof_deadline();
        assert!(a2_proof_pending(), "slice lento ≠ wedge — prova continua");
        assert!(!A2_PROOF_DONE.load(Ordering::Acquire));
        drain_infer_queue_statics();
    }

    /// Wedge WHPX 8c: matemática do chunking (8–32 toks/bloco, 16 canônico).
    #[test]
    fn prefill_chunk_window_math() {
        let _g = TEST_LOCK.lock();
        set_prefill_chunk_toks(PREFILL_CHUNK_TOKS);
        assert_eq!(prefill_chunk_toks(), 16);
        assert_eq!(PREFILL_CHUNK_TOKS, 16);
        assert_eq!(chunk_window(512, 0), Some((0, 16)));
        assert_eq!(chunk_window(512, 16), Some((16, 16)));
        assert_eq!(chunk_window(512, 496), Some((496, 16)));
        assert_eq!(chunk_window(512, 500), Some((500, 12)));
        assert_eq!(chunk_window(512, 512), None);
        assert_eq!(chunk_window(10, 0), Some((0, 10)));
        // 512 toks → 32 chunks de 16.
        assert_eq!(512usize.div_ceil(prefill_chunk_toks()), 32);
        drain_infer_queue_statics();
    }

    /// Chunk tunável: exercita 32 na matemática pura (sem modelo); o clamp
    /// 1..=64 segura abusos e o default 16 é restaurado ao fim.
    #[test]
    fn prefill_chunk_tunable_32_math_only() {
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        assert_eq!(prefill_chunk_toks(), 16, "default provado");
        // Clamp 1..=64.
        assert_eq!(set_prefill_chunk_toks(0), 1);
        assert_eq!(set_prefill_chunk_toks(999), 64);
        // 32: window math + ordinal do log `prefill_chunk id=.. n=../..`.
        assert_eq!(set_prefill_chunk_toks(32), 32);
        assert_eq!(prefill_chunk_toks(), 32);
        assert_eq!(chunk_window(512, 0), Some((0, 32)));
        assert_eq!(chunk_window(512, 32), Some((32, 32)));
        assert_eq!(chunk_window(512, 480), Some((480, 32)));
        assert_eq!(chunk_window(512, 500), Some((500, 12)));
        assert_eq!(chunk_window(512, 512), None);
        assert_eq!(512usize.div_ceil(prefill_chunk_toks()), 16);
        // n=toks_done/total e chunk ord/total coerentes em 32.
        let (off, n) = chunk_window(512, 64).expect("janela");
        assert_eq!((off + n, 512), (96, 512));
        assert_eq!((off / prefill_chunk_toks() + 1, 512usize.div_ceil(prefill_chunk_toks())), (3, 16));
        // Restaura o default provado — runtime nunca sai de 16.
        assert_eq!(set_prefill_chunk_toks(PREFILL_CHUNK_TOKS), 16);
        assert_eq!(prefill_chunk_toks(), 16);
        assert_eq!(chunk_window(512, 0), Some((0, 16)));
        drain_infer_queue_statics();
    }

    /// Budget por hypervisor: HW 30s fixo; sandbox ≥120s (piso), auto-tune por slice.
    #[test]
    fn slice_budget_sandbox_floor_and_hw_const() {
        assert_eq!(A2_SLICE_STALL_US, 30_000_000);
        assert_eq!(SANDBOX_STALL_US, 120_000_000);
        assert_eq!(SANDBOX_STALL_STREAK_N, 3);
        // Host sem probe = leniente (sandbox): nunca wedge terminal no teste.
        assert!(eff_sandbox());
        assert!(slice_stall_budget_us() >= SANDBOX_STALL_US);
        PREFILL_LAST_SLICE_US.store(60_000_000, Ordering::Relaxed);
        assert_eq!(slice_stall_budget_us(), 480_000_000);
        PREFILL_LAST_SLICE_US.store(0, Ordering::Relaxed);
    }

    /// Bullet 4: struct-máquina vira ids diretos, sem framed/string.
    #[test]
    fn machine_ctx_to_tokens_direct_no_framed() {
        let m = MachineCtx {
            intent_id: 42,
            slots: alloc::vec![(1, 2), (3, 4)],
            ctx_ids: alloc::vec![10, 20, 30],
        };
        // bos=10 (Falcon), vs ampla: cabeça bos+intent+slots+ctx, sem framed.
        assert_eq!(m.to_tokens(131072, 10), alloc::vec![10, 42, 1, 2, 3, 4, 10, 20, 30]);
        // Filtro vocab: ids >= vs caem (mesmo gate `retain` do texto).
        let m2 = MachineCtx {
            intent_id: 200_000,
            slots: alloc::vec![(5, 300_000)],
            ctx_ids: alloc::vec![7, 400_000],
        };
        assert_eq!(m2.to_tokens(131072, 10), alloc::vec![10, 5, 7]);
        // Cap de slots (8 pares): o 9º par é descartado.
        let mut slots = Vec::new();
        for i in 0..10u32 {
            slots.push((100 + i, 200 + i));
        }
        let m3 = MachineCtx { intent_id: 42, slots, ctx_ids: Vec::new() };
        let t3 = m3.to_tokens(131072, 10);
        assert_eq!(t3.len(), 1 + 1 + MachineCtx::MAX_SLOTS * 2);
        assert_eq!(t3[0], 10);
        // Nunca vazio: sem nada válido, cai em [bos].
        let m4 = MachineCtx { intent_id: 999_999, slots: Vec::new(), ctx_ids: Vec::new() };
        assert_eq!(m4.to_tokens(131072, 10), alloc::vec![10]);
    }

    /// Bullet 4: submit sem struct = legado (EmptyPrompt p/ texto vazio);
    /// com struct, texto vazio é job-máquina válido.
    #[test]
    fn machine_ctx_submit_empty_text_rules() {
        let _g = TEST_LOCK.lock();
        drain_infer_queue_statics();
        assert!(submit(String::from(""), InferMode::Plain, TOPIC_LLM_RESPONSE).is_err());
        let m = MachineCtx { intent_id: 7, slots: Vec::new(), ctx_ids: alloc::vec![11, 12] };
        let id = submit_with_ctx(String::from(""), Some(m), InferMode::Plain, TOPIC_LLM_RESPONSE)
            .expect("struct dispensa texto");
        assert!(cancel(id));
        drain_infer_queue_statics();
    }
}
