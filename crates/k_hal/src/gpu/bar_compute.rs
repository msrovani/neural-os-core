//! BAR Compute — motor W2A8 vendor-agnostic (ADR-0112).
//!
//! ## Por que "fora da caixa" funciona
//! O ganho do Falcon3 decode não está em executar shader na GPU (isso exige
//! ACR/PSP/GuC + ISA por geração — a muralha). Está em ** onde a memória
//! mora **: com os pesos residentes na VRAM (via BAR, `vram_stream`), a GEMV
//! roda no host lendo através do barramento PCIe — e isso destrava:
//!
//! 1. **RAM liberada**: ~544MB (1B) a ~1.5GB (3B W2) saem do heap bump
//!    (janela ~2030MB, SESSION_415-417 OOM) → decode vive mais sem OOM.
//! 2. **Prefetch overlap**: enquanto o host computa o tile j, o prefetch do
//!    tile j+1 já está em voo no barramento (duplo buffer) — a latência PCIe
//!    (~1µs) se esconde atrás do compute (~50µs/tile).
//! 3. **Caminho único vendor-agnostic**: mesma matemática, qualquer GPU — o
//!    device path específico (CE/SDMA/BCS) pluga DEPOIS como upgrade, com o
//!    mesmo contrato de buffers (`W2a8DeviceBuffers`).
//!
//! Honesty: enquanto não há motor device, o compute é HOST lendo VRAM. O
//! `note_gpu_compute` NÃO é chamado (SYS_HEALTH gpu segue UNKNOWN — a GPU
//! não fez a conta; o ganho é de memória, não de FLOPS). O WIN medido é
//! RAM liberada + estabilidade (sem OOM no decode longo), não tok/s.

use alloc::vec::Vec;
use cortex::tensor::{PackedTernaryTensor, Tensor};
use crate::gpu::detect::GpuInfo;
use crate::gpu::vram_stream::{
    note_weights_resident, stage, stream_x_slot, RingPlan, StreamStage,
};

/// Plano residente: quais matrizes de peso já foram copiadas para VRAM.
pub struct ResidentWeights {
    /// (offset VRAM físico, k, n) por matriz, na ordem de upload.
    pub mats: alloc::vec::Vec<(u64, usize, usize)>,
    /// Bytes totais residentes.
    pub total_bytes: u64,
}

static RESIDENT: spin::Mutex<Option<ResidentWeights>> = spin::Mutex::new(None);
static UPLOADED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// ADR-0112 s421 — lane por SEQUÊNCIA (layer-major): o cortex chama com o
/// índice absoluto da matriz na ordem canônica (layer*7 + [q,k,v,o,gate,up,
/// down]). Shape NÃO resolve q/k/v/o (mesma (h,h)) e colidiria com experts
/// do MoE — só a sequência é chave confiável. O upload sobe TODAS as
/// matrizes na MESMA ordem do snapshot; se a aperture não comporta o total
/// (1B ≈ 369MB, 3B ≈ 675MB; BAR1 sem ReBAR ≈ 256MB), o lane fica off
/// honesto (CPU ladder) — nunca parcial/ambíguo.
static SEQ_MATS: spin::Mutex<alloc::vec::Vec<(u64, usize, usize)>> =
    spin::Mutex::new(alloc::vec::Vec::new());

/// Nº de matrizes por layer na sequência canônica do TransformerModel.
/// Deve casar com os call sites de `dispatch_vram_seq` em cortex.rs.
pub const SEQ_SLOTS_PER_LAYER: usize = 7;

/// Copia a matriz de pesos packed (k,n) para a VRAM no offset dado.
/// Retorna bytes escritos. Fail-closed sem aperture.
///
/// # Safety
/// Aperture UC mapeada (`init_vram_tier`) e offset+len dentro da aperture
/// (verificado aqui contra o tamanho do buddy).
unsafe fn upload_weights_at(
    w: &PackedTernaryTensor,
    vram_off: u64,
    aperture_size: u64,
) -> Option<usize> {
    let packed = &w.packed_data;
    if packed.is_empty() || vram_off.saturating_add(packed.len() as u64) > aperture_size {
        return None;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let dst = (vram_off as *mut u8).wrapping_add(pmoff as usize);
    core::ptr::copy_nonoverlapping(packed.as_ptr(), dst, packed.len());
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    Some(packed.len())
}

/// Sobe a camada de pesos para a VRAM (stage `Mapped`): aloca offsets
/// sequenciais a partir do fim do ring, copia, registra. Idempotente por
/// camada (re-upload substitui no mesmo offset — determinístico pela ordem).
///
/// Retorna o plano com os offsets (o caller guarda por layer).
pub fn upload_layer_weights(layers: &[&PackedTernaryTensor], ring_plan: &RingPlan) -> Option<ResidentWeights> {
    if stage() == StreamStage::Off {
        return None;
    }
    // Aperture size do buddy (limite de segurança).
    let aperture = crate::gpu::vram::vram_status_aperture_bytes()?;
    // s421: cursor PERSISTENTE de pesos — o hint (boot, antes do LLM) e o LLM
    // (set_model, depois) dividem a aperture SEM colidir: cada upload começa
    // onde o anterior terminou (alinhado 4KB).
    static NEXT_WEIGHT_OFF: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    let mut next_off = {
        let cur = NEXT_WEIGHT_OFF.load(core::sync::atomic::Ordering::Acquire);
        if cur == 0 {
            ring_plan.ring_bytes().max(2 * 1024 * 1024)
        } else {
            cur
        }
    };
    let mut mats = alloc::vec::Vec::new();
    let mut total = 0u64;
    for w in layers {
        let packed_len = w.packed_data.len() as u64;
        // Alinha a 4KB (bus master friendly, huge-page safe).
        next_off = (next_off + 4095) & !4095;
        let written = unsafe { upload_weights_at(w, next_off, aperture) }?;
        mats.push((next_off, w.shape.0, w.shape.1));
        next_off += written as u64;
        total += written as u64;
    }
    NEXT_WEIGHT_OFF.store(next_off, core::sync::atomic::Ordering::Release);
    // s427: publica o cursor para o loader-VRAM continuar depois (sem colisão).
    NEXT_WEIGHT_CURSOR.store(next_off, core::sync::atomic::Ordering::Release);
    note_weights_resident(total);
    // s421: SEQ_MATS é do LLM (setado só no on_model_loaded) — upload de
    // hints NÃO toca o índice do lane (ordem boot: hint antes do LLM).
    *RESIDENT.lock() = Some(ResidentWeights { mats: mats.clone(), total_bytes: total });
    Some(ResidentWeights { mats, total_bytes: total })
}

/// Entry point do lane VRAM POR SEQUÊNCIA (s421). `seq` = índice absoluto
/// layer*7+slot na ordem canônica; `w` tem que bater com a matriz residente
/// daquele índice (proteção contra dessincronia de sequência — divergiu →
/// None honesto → CPU ladder, nunca peso errado).
pub fn vram_ternary_seq(seq: usize, w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    if !bar_compute_enabled() {
        return None;
    }
    let (vram_off, k, n) = {
        let g = SEQ_MATS.lock();
        let (off, k, n) = *g.get(seq)?;
        (off, k, n)
    };
    // Proteção de identidade: shape divergente = sequência dessincronizada.
    if w.shape != (k, n) {
        return None;
    }
    let resident_guard = RESIDENT.lock();
    let resident = resident_guard.as_ref()?;
    let plan = RingPlan::falcon3_default();
    // Slot do ring: alterna por chamada (duplo buffer — prefetch overlap).
    static NEXT_SLOT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
    let slot = NEXT_SLOT.fetch_add(1, core::sync::atomic::Ordering::Relaxed) % plan.slots;
    // Safety: canário de round-trip passou (bar_compute_enabled) e offsets
    // vêm do upload determinístico.
    unsafe { gemv_from_vram(x, resident, seq, &plan, slot) }
}

/// Marca o upload completo (uma vez por boot de modelo).
pub fn mark_uploaded() {
    UPLOADED.store(true, core::sync::atomic::Ordering::Release);
}

pub fn weights_uploaded() -> bool {
    UPLOADED.load(core::sync::atomic::Ordering::Acquire)
}

// ── s427: loader-VRAM (FAT → BAR sem heap) ────────────────────────────────
//
// O `upload_layer_weights` copia do HEAP (os pesos já passaram pelo bump via
// `load_llm_v6`). O loader-VRAM é o caminho onde os pesos NUNCA tocam o heap:
// o FAT chunked lê o .bitnet v6 e cada tensor vai direto da RAM-de-disco para
// a aperture BAR (bounce de cluster, zero Vec do blob). O heap passa a segurar
// APENAS norms/embed/KV (o 1B cai de ~970MB → ~300MB; o 3B cabe onde não cabia).

/// Reserva o próximo trecho alinhado da aperture (mesmo cursor do upload,
/// sem copiar nada). Retorna (offset, bytes_disponíveis) — o caller compara
/// com o tamanho esperado do tensor ANTES de escrever.
pub fn reserve_vram_span(len: u64) -> Option<(u64, u64)> {
    if !bar_compute_enabled() {
        return None;
    }
    let aperture = crate::gpu::vram::vram_status_aperture_bytes()?;
    static NEXT_LOADER_OFF: core::sync::atomic::AtomicU64 =
        core::sync::atomic::AtomicU64::new(0);
    let mut off = NEXT_LOADER_OFF.load(core::sync::atomic::Ordering::Acquire);
    if off == 0 {
        // Inicializa com o cursor do upload (hint já reservou o dele).
        off = NEXT_WEIGHT_CURSOR.load(core::sync::atomic::Ordering::Acquire);
        if off == 0 {
            off = RingPlan::falcon3_default().ring_bytes().max(2 * 1024 * 1024);
        }
    }
    let aligned = (off + 4095) & !4095;
    if aligned.saturating_add(len) > aperture {
        return None; // não cabe — recusa INTEIRA (parcial é proibido)
    }
    NEXT_LOADER_OFF.store(aligned + len, core::sync::atomic::Ordering::Release);
    Some((aligned, aperture.saturating_sub(aligned)))
}

/// Cursor compartilhado: o `upload_layer_weights` publica o fim do último
/// upload aqui para o loader começar DEPOIS do hint (evita colisão).
pub static NEXT_WEIGHT_CURSOR: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Copia um bloco de bytes do arquivo FAT direto para o offset da VRAM.
/// `len` tem que caber (reserve_vram_span garantiu). Zero-alloc: src é o
/// bounce buffer do leitor chunked.
///
/// # Safety
/// Aperture UC mapeada; offset..offset+len dentro da aperture (reservado).
pub unsafe fn write_vram_bytes(vram_off: u64, src: &[u8]) -> bool {
    let aperture = crate::gpu::vram::vram_status_aperture_bytes().unwrap_or(0);
    if aperture == 0 || vram_off.saturating_add(src.len() as u64) > aperture {
        return false;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let dst = (vram_off as *mut u8).wrapping_add(pmoff as usize);
    core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
    true
}

/// Registro pós-loader: publica os offsets por sequência (MESMO contrato do
/// `on_model_loaded`) e marca o lane ativo. `mats` = (off, k, n) na ordem
/// canônica layer*7+slot.
pub fn register_loader_resident(mats: alloc::vec::Vec<(u64, usize, usize)>, total_bytes: u64) {
    *SEQ_MATS.lock() = mats;
    *RESIDENT.lock() = Some(ResidentWeights {
        mats: SEQ_MATS.lock().clone(),
        total_bytes,
    });
    mark_uploaded();
    note_weights_resident(total_bytes);
    k_nano::slog_hal!(
        "BARCOMPUTE", "ok",
        "loader-VRAM: pesos FAT→BAR sem heap ({} matrizes, {}MB) — lane ativo",
        SEQ_MATS.lock().len(),
        total_bytes / (1024 * 1024)
    );
}

/// True quando o loader-VRAM já registrou os pesos (o upload pós-load do heap
/// vira no-op honesto — não copiar 2×).
pub fn loader_resident() -> bool {
    UPLOADED.load(core::sync::atomic::Ordering::Acquire)
}

/// GEMV W2A8 com ativações streaming: quantiza x → i8, streama para o slot
/// do ring na VRAM, computa no host lendo os pesos DIRETO da VRAM (offsets
/// do `ResidentWeights`). **O loop interno lê via BAR** — é aqui que a VRAM
/// vira "memória principal" do compute.
///
/// Contrato: mesmo resultado de `w2a8_reference_quantized` (golden host) —
/// os bytes são os mesmos, só o endereço de origem muda.
///
/// # Safety (caller garante)
/// - `resident` veio de `upload_layer_weights` (offsets válidos).
/// - `bar_compute_enabled() == true` (canário de round-trip passou).
pub unsafe fn gemv_from_vram(
    x: &Tensor,
    resident: &ResidentWeights,
    mat_idx: usize,
    ring_plan: &RingPlan,
    slot: usize,
) -> Option<Tensor> {
    if !bar_compute_enabled() {
        return None;
    }
    let (_, k, n) = *resident.mats.get(mat_idx)?;
    let (m, k2) = x.shape;
    if k != k2 || m == 0 || k == 0 || n == 0 {
        return None;
    }
    let (vram_off, _, _) = *resident.mats.get(mat_idx)?;
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

    // 1. Quantiza x (mesma matemática do reference: si = max|x|/127) —
    // ESCALA POR LINHA (fix s421: a escala era única sobrescrita por linha,
    // errada para m>1).
    let mut xq = Vec::new();
    xq.try_reserve_exact(k).ok()?;
    xq.resize(k, 0i32);
    let mut si_row = [1.0f32; 8]; // m ≤ 8 (RingPlan 8×K)
    if m > si_row.len() {
        return None;
    }
    for i in 0..m {
        let mut max_abs = 0.0f32;
        for &v in &x.data[i * k..(i + 1) * k] {
            let a = v.abs();
            if a > max_abs {
                max_abs = a;
            }
        }
        let si = if max_abs > 1e-9 { max_abs / 127.0 } else { 1.0 };
        si_row[i] = si;
        let inv = 1.0 / si;
        for (t, xd) in x.data[i * k..(i + 1) * k].iter().enumerate() {
            // round-half-away (mesma regra do reference quantized)
            let r = if *xd >= 0.0 { *xd * inv + 0.5 } else { *xd * inv - 0.5 };
            xq[t] = r as i32;
        }
    }

    // 2. Streama xq i8 para o slot do ring (a VRAM "vê" a ativação).
    let xi8: Vec<i8> = xq.iter().map(|&q| q.clamp(-127, 127) as i8).collect();
    if stream_x_slot(&xi8, slot, ring_plan).is_none() {
        return None;
    }

    // 3. GEMV no host lendo pesos DIRETO da aperture (offsets residentes).
    let w_base = (vram_off as *const u8).wrapping_add(pmoff as usize);
    let mut out = Tensor::new((m, n));
    if !out.is_valid() {
        return None;
    }
    for i in 0..m {
        let si = si_row[i];
        for j_block in (0..n).step_by(8) {
            let j_end = (j_block + 8).min(n);
            // Bytes em VRAM = MESMO pack row-major do heap: peso (t,j) no
            // offset flat t*n+j (fix s421 — o heap usa get_weight(t*n+j) e o
            // bitnet_sse lê w_idx = t*n+j; qualquer outro layout corrompe).
            let mut acc = [0i32; 8];
            for t in 0..k {
                let inp = xq[t];
                if inp == 0 {
                    continue; // peso × 0 = 0 (skip honesto, economiza BAR reads)
                }
                // Prefetch do bloco seguinte de linhas (mesmo t, j+8) —
                // o barramento busca enquanto computamos.
                if j_end < n && (t & 63) == 63 {
                    let pf = w_base.add(((t * n + j_end) >> 2) as usize);
                    core::arch::asm!("prefetcht0 [{}]", in(reg) pf, options(nostack, preserves_flags));
                }
                for (jj, j) in (j_block..j_end).enumerate() {
                    // Bytes em VRAM = MESMO pack row-major do heap: peso
                    // (t,j) no offset flat t*n+j (get_weight(t*n+j) do heap).
                    let idx = t * n + j;
                    let byte = core::ptr::read_volatile(w_base.add(idx >> 2));
                    let pair = (byte >> ((idx & 3) << 1)) & 3;
                    let v = ((pair & 1) as i32) - ((pair >> 1) as i32);
                    acc[jj] += inp * v;
                }
            }
            for (jj, j) in (j_block..j_end).enumerate() {
                out.data[i * n + j] = acc[jj] as f32 * si;
            }
        }
    }
    Some(out)
}

/// Gate: BAR compute só depois do canário de round-trip (stage ≥ Mapped).
/// Honestidade ADR-0112: `Mapped` = pesos residentes + GEMV no HOST lendo a
/// aperture — este gate NÃO afirma compute no device. `note_gpu_compute`
/// (SYS_HEALTH gpu) é chamado SÓ no backend pós-canário `Pass`
/// (`backend::gpu_matmul` com `COMPUTE_STATE == Ready`); stage `Mapped`
/// sozinho mantém gpu UNKNOWN. O WIN medido aqui é RAM liberada
/// (`total_bytes` fora da janela ~2030MB), nunca tok/s.
pub fn bar_compute_enabled() -> bool {
    matches!(stage(), StreamStage::Mapped | StreamStage::StreamsW2a8 | StreamStage::ComputeDevice)
}

/// Inicializa BAR compute para a GPU compute (após init_vram_tier + ring).
pub fn init_bar_compute(gpu: &GpuInfo) -> bool {
    let plan = RingPlan::falcon3_default();
    unsafe { crate::gpu::vram_stream::init_stream_ring(gpu, &plan) }
}

/// Callback pós-load de modelo (seam `cortex::register_vram_upload_hook`):
/// sobe TODAS as matrizes ternárias das layers para a VRAM (stage Mapped).
/// Determinístico: ordem canônica layer*7+[q,k,v,o,gate,up,down] → offsets
/// sequenciais → o SEQ_MATS permite dispatch por sequência nas chamadas
/// seguintes (mesma ordem do snapshot `current_model_layers_snapshot`).
pub fn on_model_loaded() {
    if !bar_compute_enabled() {
        return;
    }
    // s427: loader-VRAM já subiu os pesos (FAT→BAR sem heap) — o lane já está
    // registrado (SEQ_MATS/UPLOADED); copiar do heap de novo seria desperdício
    // e sobrescreveria com os MESMOS bytes via caminho mais caro. No-op honesto.
    if loader_resident() {
        k_nano::slog_hal!(
            "BARCOMPUTE", "info",
            "upload pós-load skip: loader-VRAM já residente (FAT→BAR sem heap)"
        );
        return;
    }
    // Snapshot das matrizes do modelo carregado (sem clonar dados — upload
    // lê direto do packed residente no heap).
    let model = cortex::cortex::current_model_layers_snapshot();
    let Some(layers) = model else { return };
    // s421: pré-checagem de capacidade — o total tem que caber INTEIRO
    // (parcial/ambíguo não: peso errado corrompe logits silenciosamente).
    let aperture = crate::gpu::vram::vram_status_aperture_bytes().unwrap_or(0);
    let plan = RingPlan::falcon3_default();
    let need: u64 = layers.iter().map(|t| t.packed_data.len() as u64).sum();
    // Disponível = aperture − ring − o que já residente (hint, etc.).
    let used = crate::gpu::vram_stream::BYTES_RESIDENT.load(core::sync::atomic::Ordering::Relaxed);
    let avail = aperture.saturating_sub(plan.ring_bytes()).saturating_sub(used);
    if need > avail {
        k_nano::slog_hal!(
            "BARCOMPUTE", "warn",
            "lane VRAM off honesto: need={}MB > aperture disponível={}MB (sem ReBAR) — CPU ladder",
            need / (1024 * 1024), avail / (1024 * 1024)
        );
        return;
    }
    match upload_layer_weights(&layers, &plan) {
        Some(r) => {
            mark_uploaded();
            // s421: registra o índice sequencial (layer-major) do lane —
            // SEM dedupe; shape não resolve q/k/v/o (mesma (h,h)).
            *SEQ_MATS.lock() = r.mats.clone();
            k_nano::slog_hal!(
                "BARCOMPUTE", "ok",
                "pesos residentes POR SEQUÊNCIA: {} matrizes ({}MB) — GEMV host lê via BAR (lane ativo)",
                r.mats.len(),
                r.total_bytes / (1024 * 1024)
            );
        }
        None => {
            k_nano::slog_hal!("BARCOMPUTE", "warn", "upload VRAM falhou — pesos seguem no heap (honesto)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_off_no_host_e_honesto() {
        // Host sem init: gate fechado — gemv nem tenta.
        if !crate::gpu::vram_stream::STREAM_READY.load(core::sync::atomic::Ordering::Acquire) {
            assert!(!bar_compute_enabled());
        }
    }

    #[test]
    fn upload_sem_stage_off_devolve_none() {
        if stage() == StreamStage::Off {
            let w = PackedTernaryTensor { packed_data: alloc::vec![0x55u8; 64], shape: (16, 16) };
            let plan = RingPlan::falcon3_default();
            assert!(upload_layer_weights(&[&w], &plan).is_none());
        }
    }

    /// s421: lane por sequência SEM VRAM real (host) — vram_ternary_seq tem
    /// que devolver None honesto (gate fechado / seq ausente / shape divergente).
    #[test]
    fn vram_seq_sem_residente_nunca_mente() {
        let w = PackedTernaryTensor { packed_data: alloc::vec![0u8; 4096], shape: (128, 128) };
        let x = Tensor::zero((1, 128));
        // Sem SEQ_MATS preenchido (host) → None em qualquer slot.
        assert!(vram_ternary_seq(0, &w, &x).is_none());
    }

    /// ADR-0112 honesty: sem aperture o lane NUNCA ativa — `on_model_loaded`
    /// é no-op (sem panic, sem registro parcial) e o seq segue None.
    /// Stage `Mapped` jamais afirma compute no device: `note_gpu_compute`
    /// vive SÓ no backend pós-canário `Pass` (verificado por inspeção —
    /// este módulo não referencia `sys_health::note_gpu_compute`).
    #[test]
    fn on_model_loaded_sem_stage_e_noop_honesto() {
        if stage() == StreamStage::Off {
            assert!(!bar_compute_enabled());
            on_model_loaded(); // no-op honesto: sem aperture, nada a subir
            assert!(!bar_compute_enabled());
            assert!(!weights_uploaded());
            let w = PackedTernaryTensor { packed_data: alloc::vec![0u8; 4096], shape: (128, 128) };
            let x = Tensor::zero((1, 128));
            assert!(vram_ternary_seq(0, &w, &x).is_none());
        }
    }
}
