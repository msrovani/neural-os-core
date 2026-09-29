//! Neural Render Hints (ADR-0047-HMI Pilar H3, reaberto pela ADR-0112).
//!
//! O descarte original do H3 ("diffusion 263M+ inviável soft-float") apoiava-se
//! na premissa de que compute neural exige executar na GPU. A ADR-0112 derrubou
//! a premissa: pesos residem na VRAM (BAR aperture, vendor-agnostic), o host
//! computa lendo via BAR. O MLP de hints (~10⁵ params W2A8 ≈ 100KB packed)
//! cabe de sobra na menor aperture honesta (256MB) e NÃO toca o heap bump.
//!
//! Contrato (§6.4 da ADR-0047-HMI): hints são AUMENTATIVOS — o compositor
//! clássico continua desenhando tudo; os hints só colorir/energizar regiões
//! (orb, splats, avatar). Modo clássico é o default; neural só com modelo
//! carregado E canário BAR passado.
//!
//! Honesty (mesma regra do bar_compute): stage `Mapped` = GEMV no HOST lendo
//! VRAM — `note_gpu_compute` NÃO é chamado, SYS_HEALTH gpu segue UNKNOWN.
//! O claim é "hints por modelo com pesos em VRAM", nunca "GPU renderiza".

use alloc::vec::Vec;
use cortex::tensor::Tensor;
use crate::gpu::bar_compute::{bar_compute_enabled, gemv_from_vram, ResidentWeights};
use crate::gpu::vram::vram_status_aperture_bytes;
use crate::gpu::vram_stream::{stage, RingPlan, StreamStage};

/// Dims do MLP de hints (ADR-0047-HMI §6.2, reduzido p/ W2A8):
/// entrada = estado da UI compactado, hidden, saída = hint por região.
pub const HINT_IN: usize = 64;
pub const HINT_HIDDEN: usize = 128;
pub const HINT_OUT: usize = 16;

/// Hints por região do frame (não 80×80 chunks — regioes semanticas):
/// orb, header, dock, e as 4 janelas de card + HUD.
pub const HINT_REGIONS: usize = 8;

/// Pack de pesos ternários compacto (mesma matemática do bitnet_w2a8:
/// signed ternário {-1,0,1} empacotado 4 pesos/byte, 2 bits por peso).
/// Layout: packed[(i*n + j) >> 2], 2 bits low→high dentro do byte.
pub struct HintWeights {
    /// 3 matrizes: (HIDDEN×IN), (OUT×HIDDEN), (IN→OUT residual opcional).
    pub w1: Vec<u8>, // (HIDDEN, IN) packed
    pub b1: [i32; HINT_HIDDEN],
    pub w2: Vec<u8>, // (OUT, HIDDEN) packed
    pub b2: [i32; HINT_OUT],
}

impl HintWeights {
    /// Bytes packed de uma matriz (k,n).
    fn packed_len(k: usize, n: usize) -> usize {
        k * n / 4
    }

    /// Aloca zerado (modelo "ausente" — hints zerados = comportamento clássico).
    pub fn zeroed() -> Self {
        HintWeights {
            w1: alloc::vec![0u8; Self::packed_len(HINT_HIDDEN, HINT_IN)],
            b1: [0; HINT_HIDDEN],
            b2: [0; HINT_OUT],
            w2: alloc::vec![0u8; Self::packed_len(HINT_OUT, HINT_HIDDEN)],
        }
    }

    /// Bytes totais packed (p/ upload VRAM).
    pub fn total_bytes() -> usize {
        Self::packed_len(HINT_HIDDEN, HINT_IN) + Self::packed_len(HINT_OUT, HINT_HIDDEN)
    }
}

/// Pack ternário: 2 bits por peso: 01=+1, 10=-1, 00/11=0.
#[inline]
fn pack_weight(v: i8) -> u8 {
    match v {
        1 => 0b01,
        -1 => 0b10,
        _ => 0b00,
    }
}

/// Unpack de 2 bits → peso ternário.
#[inline]
fn unpack_weight(bits: u8) -> i8 {
    match bits & 3 {
        0b01 => 1,
        0b10 => -1,
        _ => 0,
    }
}

/// Quant signed round-half de f32 → ternário {-1,0,1} com limiar 0.33
/// (distribuição típica de pesos de MLP treinado — H3 usa o mesmo pack do
/// BitNet para reusar a GEMV W2A8 sem segunda implementação).
#[inline]
fn quant_ternary(v: f32) -> i8 {
    if v > 0.33 {
        1
    } else if v < -0.33 {
        -1
    } else {
        0
    }
}

/// Estado do renderer de hints (global — setado no boot/por agente).
pub struct HintRenderer {
    pub weights: HintWeights,
    /// Regiões monitoradas (x, y, w, h em q8 de 720p — escala com o frame).
    pub regions: [(u16, u16, u16, u16); HINT_REGIONS],
}

/// (estado da UI, região) → entrada do MLP (HINT_IN f32, já quantizada i8 no call).
/// O agente display chama `set_ui_state` no tick de amostragem (2 Hz), nunca
/// no paint (higiene: zero alloc no hot path).
static UI_STATE: spin::Mutex<[f32; HINT_IN]> = spin::Mutex::new([0.0; HINT_IN]);

/// Nota o estado da UI p/ o próximo forward (2 Hz, agente display).
/// `v` = features compactadas: [0]=carga de infer (0..1), [1]=tok/s norm,
/// [2]=lane VRAM ativa, [3]=audio RMS, [4..16]=FFT bins norm (12), resto 0.
pub fn set_ui_state(v: [f32; HINT_IN]) {
    *UI_STATE.lock() = v;
}

/// Stage atual do renderer de hints (honesto):
/// 0 = Off (sem BAR), 1 = Ready (BAR ok, pesos zero = pass-through clássico),
/// 2 = Resident (pesos uploaded, hints neurais ativos).
pub fn hint_stage() -> u8 {
    match stage() {
        StreamStage::Off => 0,
        StreamStage::Mapped | StreamStage::StreamsW2a8 => {
            if HINT_RESIDENT.load(core::sync::atomic::Ordering::Acquire) {
                2
            } else {
                1
            }
        }
        StreamStage::ComputeDevice => 3,
    }
}

static HINT_RESIDENT: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
static HINT_RESIDENT_BYTES: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
/// Telemetria de hints gerados (observabilidade sem custo no paint).
pub static HINTS_GENERATED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
/// Última latência de forward (µs) — o MLP é minúsculo, deve ficar <100µs.
pub static HINT_FORWARD_US: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Snapshot residente p/ o forward (offsets VRAM — sem clone de bytes).
/// (vram_off_w1, vram_off_w2) + aperture guard implícito no bar_compute.
static HINT_OFFSETS: spin::Mutex<Option<(u64, u64)>> = spin::Mutex::new(None);

/// Sobe os pesos do renderer para a VRAM (stage Mapped). Idempotente.
/// Chamado no boot após `init_bar_compute` (pode subir antes de modelo LLM —
/// os hints não dependem do LLM, dependem só da aperture).
///
/// Honesty: o upload usa o MESMO mecanismo do LLM (`upload_layer_weights`) e
/// entra no MESMO índice de shapes — sem segunda implementação de BAR.
pub fn upload_hint_weights(w: &HintWeights) -> bool {
    if stage() == StreamStage::Off {
        return false;
    }
    let Some(aperture) = vram_status_aperture_bytes() else {
        return false;
    };
    // Espaço: total_hint + ring + LLM (se já residente) deve caber. O upload
    // do LLM acontece depois e recalcula next_off a partir do fim do hint.
    if (RingPlan::falcon3_default().ring_bytes() as usize + HintWeights::total_bytes()) as u64
        > aperture
    {
        return false;
    }
    // Espelha o mecanismo do LLM: monta PackedTernaryTensor temporários
    // (shape (k,n), mesmo pack 4 pesos/byte) e usa o upload determinístico.
    let w1_t = cortex::tensor::PackedTernaryTensor {
        shape: (HINT_HIDDEN, HINT_IN),
        packed_data: w.w1.clone(),
    };
    let w2_t = cortex::tensor::PackedTernaryTensor {
        shape: (HINT_OUT, HINT_HIDDEN),
        packed_data: w.w2.clone(),
    };
    let layers = [&w1_t, &w2_t];
    let Some(resident) = crate::gpu::bar_compute::upload_layer_weights(&layers, &RingPlan::falcon3_default()) else {
        return false;
    };
    // Matriz 0 = w1 (HIDDEN,IN), matriz 1 = w2 (OUT,HIDDEN) — offsets da VRAM.
    let (Some((off1, _, _)), Some((off2, _, _))) =
        (resident.mats.first().copied(), resident.mats.get(1).copied())
    else {
        return false;
    };
    *HINT_OFFSETS.lock() = Some((off1, off2));
    HINT_RESIDENT.store(true, core::sync::atomic::Ordering::Release);
    HINT_RESIDENT_BYTES.store(resident.total_bytes, core::sync::atomic::Ordering::Release);
    true
}

/// Forward: gera HINT_REGIONS hints a partir do estado da UI corrente.
/// Cada hint = (reservado, energia 0..255, matiz 0..255). Zero alloc.
///
/// Retorna None se o estágio não suporta (Off/sem residente) — caller usa o
/// modo clássico (comportamento idêntico ao atual).
pub fn render_hints() -> Option<[(u8, u8, u8); HINT_REGIONS]> {
    let t0 = k_nano::tsc::now_us();
    if !bar_compute_enabled() {
        return None;
    }
    let (off1, off2) = {
        let g = HINT_OFFSETS.lock();
        let Some(v) = *g else { return None };
        v
    };
    let x = *UI_STATE.lock();

    // h = x·W1ᵀ (1×HIDDEN): h[j] = Σ_t x[t]·W1[j*IN + t] — leitura sequencial
    // dentro de cada linha j do pack (coalesced no BAR).
    let mut h = [0.0f32; HINT_HIDDEN];
    for j in 0..HINT_HIDDEN {
        let mut acc = 0.0f32;
        for t in 0..HINT_IN {
            let idx = j * HINT_IN + t;
            // Safety: idx < HIDDEN*IN = packed*4 < aperture (upload validou).
            let byte = unsafe {
                core::ptr::read_volatile(
                    (off1 as *const u8)
                        .wrapping_add(k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed) as usize)
                        .add(idx >> 2),
                )
            };
            let w = unpack_weight(byte >> ((idx & 3) * 2));
            acc += x[t] * w as f32;
        }
        h[j] = acc;
    }
    // ReLU — padrão MLP do ADR §6.2.
    for v in h.iter_mut() {
        if *v < 0.0 {
            *v = 0.0;
        }
    }
    // y = h·W2ᵀ (1×OUT): y[j] = Σ_i h[i]·W2[j*HIDDEN + i].
    let mut y = [0.0f32; HINT_OUT];
    for j in 0..HINT_OUT {
        let mut acc = 0.0f32;
        for t in 0..HINT_HIDDEN {
            let idx = j * HINT_HIDDEN + t;
            let byte = unsafe {
                core::ptr::read_volatile(
                    (off2 as *const u8)
                        .wrapping_add(k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed) as usize)
                        .add(idx >> 2),
                )
            };
            let w = unpack_weight(byte >> ((idx & 3) * 2));
            acc += h[t] * w as f32;
        }
        y[j] = acc;
    }
    // 16 saídas → 8 hints: (energia, matiz) por região.
    let mut hints = [(0u8, 0u8, 0u8); HINT_REGIONS];
    for r in 0..HINT_REGIONS {
        let e = (y[r * 2] as i32).clamp(0, 255) as u8;
        let hue = (y[r * 2 + 1] as i32).clamp(0, 255) as u8;
        hints[r] = (0u8, e, hue);
    }
    HINTS_GENERATED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    HINT_FORWARD_US.store(
        k_nano::tsc::now_us().saturating_sub(t0),
        core::sync::atomic::Ordering::Relaxed,
    );
    Some(hints)
}

/// Tenta carregar `HINT.BIN` do root do volume de dados (FAT32/exFAT via ATA)
/// e sobe os pesos para a VRAM. Idempotente; fail-closed honesto (log + false)
/// em qualquer divergência de contrato — sem HINT.BIN o estágio fica Ready
/// (pass-through clássico), nunca erro.
///
/// Formato v1 (tools/train_hint_mlp.py — docstring canônica):
/// magic b"HINT", version u16, flags u16, IN/HIDDEN/OUT/REGIONS u16×4,
/// len_w1/len_w2 u32, w1 packed, w2 packed, b1 f32[128], b2 f32[16].
/// Header = 24 bytes; total esperado = 24 + len_w1 + len_w2 + 4·(HIDDEN+OUT).
pub fn try_load_from_fat() -> bool {
    let Some(data) = crate::fat_assets::read_root_file("HINT.BIN") else {
        k_nano::slog_hal!("HINT", "info", "HINT.BIN ausente no volume — hints ready (pass-through clássico)");
        return false;
    };
    if !load_packed(&data) {
        return false;
    }
    k_nano::slog_hal!("HINT", "ok", "HINT.BIN carregado bytes={} upload={}", data.len(), upload_hint_weights(&last_loaded_weights()));
    true
}

/// Pesos do último `load_packed` (intermediário p/ o upload pós-validação).
static LAST_LOADED: spin::Mutex<Option<HintWeights>> = spin::Mutex::new(None);

fn last_loaded_weights() -> HintWeights {
    let g = LAST_LOADED.lock();
    match g.as_ref() {
        Some(w) => HintWeights {
            w1: w.w1.clone(),
            b1: w.b1,
            w2: w.w2.clone(),
            b2: w.b2,
        },
        None => HintWeights::zeroed(),
    }
}

/// Valida e aceita um HINT.BIN (bytes) — retorna false honesto se divergir.
pub fn load_packed(data: &[u8]) -> bool {
    let Some(w) = parse_hint_bin(data) else {
        k_nano::slog_hal!("HINT", "fail", "HINT.BIN inválido (magic/dims/tamanho) — ignorado (fail-closed)");
        return false;
    };
    *LAST_LOADED.lock() = Some(w);
    true
}

/// Parser do formato v1. `None` = contrato violado (fail-closed).
fn parse_hint_bin(data: &[u8]) -> Option<HintWeights> {
    if data.len() < 24 || &data[0..4] != b"HINT" {
        return None;
    }
    let ver = u16::from_le_bytes([data[4], data[5]]);
    if ver != 1 {
        return None;
    }
    let dims = u16x4(data, 8);
    let (i_n, h_n, o_n, r_n) = dims;
    if (i_n, h_n, o_n, r_n) != (HINT_IN as u16, HINT_HIDDEN as u16, HINT_OUT as u16, HINT_REGIONS as u16) {
        return None;
    }
    let l1 = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let l2 = u32::from_le_bytes([data[20], data[21], data[22], data[23]]) as usize;
    if l1 != HINT_HIDDEN * HINT_IN / 4 || l2 != HINT_OUT * HINT_HIDDEN / 4 {
        return None;
    }
    let w1_end = 24 + l1;
    let w2_end = w1_end + l2;
    let total = w2_end + 4 * (HINT_HIDDEN + HINT_OUT);
    if data.len() != total {
        return None;
    }
    let mut w = HintWeights::zeroed();
    w.w1.copy_from_slice(&data[24..w1_end]);
    w.w2.copy_from_slice(&data[w1_end..w2_end]);
    for (n, chunk) in data[w2_end..].chunks_exact(4).enumerate() {
        let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if n < HINT_HIDDEN {
            w.b1[n] = v;
        } else {
            w.b2[n - HINT_HIDDEN] = v;
        }
    }
    Some(w)
}

#[inline]
fn u16x4(d: &[u8], off: usize) -> (u16, u16, u16, u16) {
    (
        u16::from_le_bytes([d[off], d[off + 1]]),
        u16::from_le_bytes([d[off + 2], d[off + 3]]),
        u16::from_le_bytes([d[off + 4], d[off + 5]]),
        u16::from_le_bytes([d[off + 6], d[off + 7]]),
    )
}

/// Linha de status para o HUD/serial (padrão honesto n/a).
pub fn status_line() -> alloc::string::String {
    match hint_stage() {
        0 => alloc::string::String::from("hints off"),
        1 => alloc::string::String::from("hints ready (no weights)"),
        2 => alloc::format!(
            "hints on vram={}KB fwd={}us n={}",
            HINT_RESIDENT_BYTES.load(core::sync::atomic::Ordering::Relaxed) / 1024,
            HINT_FORWARD_US.load(core::sync::atomic::Ordering::Relaxed),
            HINTS_GENERATED.load(core::sync::atomic::Ordering::Relaxed)
        ),
        _ => alloc::string::String::from("hints device"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_ternario_idempotente() {
        for v in [-1i8, 0, 1] {
            assert_eq!(unpack_weight(pack_weight(v)), v);
        }
    }

    #[test]
    fn quant_limiar_033() {
        assert_eq!(quant_ternary(0.5), 1);
        assert_eq!(quant_ternary(-0.5), -1);
        assert_eq!(quant_ternary(0.1), 0);
    }

    #[test]
    fn hint_weights_tamanho_packed() {
        let w = HintWeights::zeroed();
        assert_eq!(w.w1.len(), HINT_HIDDEN * HINT_IN / 4);
        assert_eq!(w.w2.len(), HINT_OUT * HINT_HIDDEN / 4);
        // Total packed < 100KB — cabe na menor aperture.
        assert!(HintWeights::total_bytes() < 100 * 1024);
    }

    #[test]
    fn hints_off_sem_bar_e_honesto() {
        if stage() == StreamStage::Off {
            assert_eq!(hint_stage(), 0);
            assert_eq!(status_line(), "hints off");
            assert!(render_hints().is_none());
        }
    }

    /// Réplica host do export do train_hint_mlp.py (contrato v1) — valida o
    /// parser bit-a-bit contra o mesmo layout que o tool emite.
    #[test]
    fn parse_hint_bin_v1_roundtrip_tool() {
        let mut d = alloc::vec::Vec::new();
        d.extend_from_slice(b"HINT");
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&(HINT_IN as u16).to_le_bytes());
        d.extend_from_slice(&(HINT_HIDDEN as u16).to_le_bytes());
        d.extend_from_slice(&(HINT_OUT as u16).to_le_bytes());
        d.extend_from_slice(&(HINT_REGIONS as u16).to_le_bytes());
        d.extend_from_slice(&((HINT_HIDDEN * HINT_IN / 4) as u32).to_le_bytes());
        d.extend_from_slice(&((HINT_OUT * HINT_HIDDEN / 4) as u32).to_le_bytes());
        // Padrões dirigidos no pack (low→high): 0b00011000 = [0,-1,1,0].
        let p1 = alloc::vec![0b00_01_10_00u8; HINT_HIDDEN * HINT_IN / 4];
        let p2 = alloc::vec![0b01_01_10_10u8; HINT_OUT * HINT_HIDDEN / 4];
        d.extend_from_slice(&p1);
        d.extend_from_slice(&p2);
        for n in 0..HINT_HIDDEN {
            d.extend_from_slice(&(n as i32).to_le_bytes());
        }
        for n in 0..HINT_OUT {
            d.extend_from_slice(&((n as i32) * 10).to_le_bytes());
        }
        assert!(load_packed(&d), "formato v1 do tool deve parsear");
        let w = last_loaded_weights();
        assert_eq!(w.w1, p1);
        assert_eq!(w.w2, p2);
        assert_eq!(w.b1[5], 5);
        assert_eq!(w.b2[7], 70);
        // Contrato violado (dims trocadas) → None/fail-closed.
        let mut bad = d.clone();
        bad[8] = 0xFF;
        assert!(!load_packed(&bad), "dims erradas recusadas");
    }

    #[test]
    fn ui_state_default_neutro() {
        set_ui_state([0.0; HINT_IN]);
        // Sem residente: None (modo clássico).
        if stage() == StreamStage::Off {
            assert!(render_hints().is_none());
        }
    }
}
