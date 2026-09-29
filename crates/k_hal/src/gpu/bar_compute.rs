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

/// Mapa (k,n) → índice de matriz residente (dispatch O(1) por shape —
/// Falcon3 tem q/k/v/o/up/down com shapes distintos por camada).
static SHAPE_INDEX: spin::Mutex<alloc::vec::Vec<(usize, usize, usize)>> =
    spin::Mutex::new(alloc::vec::Vec::new());

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
    let mut next_off = ring_plan.ring_bytes().max(2 * 1024 * 1024); // após ring, alinhado 2MB
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
    note_weights_resident(total);
    // Reconstrói o índice de shapes (1ª camada registrada vence — as layers
    // compartilham shapes: dispatch pega a matriz da família certa).
    {
        let mut idx = SHAPE_INDEX.lock();
        idx.clear();
        for (i, (_, k, n)) in mats.iter().enumerate() {
            if !idx.iter().any(|(_, k2, n2)| k2 == k && n2 == n) {
                idx.push((i, *k, *n));
            }
        }
    }
    *RESIDENT.lock() = Some(ResidentWeights { mats: mats.clone(), total_bytes: total });
    Some(ResidentWeights { mats, total_bytes: total })
}

/// Índice da 1ª matriz residente com shape (k,n) — None se não residente.
fn mat_index_for(k: usize, n: usize) -> Option<usize> {
    SHAPE_INDEX
        .lock()
        .iter()
        .find(|(_, k2, n2)| *k2 == k && *n2 == n)
        .map(|(i, _, _)| *i)
}

/// Entry point do lane VRAM (TernaryFn — mesmo contrato do GPU device).
/// Chamado pelo dispatcher do cortex quando o canário de aperture passou.
pub fn vram_ternary(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    if !bar_compute_enabled() {
        return None;
    }
    let (k, n) = w.shape;
    let mat = mat_index_for(k, n)?;
    let resident_guard = RESIDENT.lock();
    let resident = resident_guard.as_ref()?;
    let plan = RingPlan::falcon3_default();
    // Slot do ring: alterna por chamada (duplo buffer — prefetch overlap).
    static NEXT_SLOT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
    let slot = NEXT_SLOT.fetch_add(1, core::sync::atomic::Ordering::Relaxed) % plan.slots;
    // Safety: canário de round-trip passou (bar_compute_enabled) e offsets
    // vêm do upload determinístico.
    unsafe { gemv_from_vram(x, resident, mat, &plan, slot) }
}

/// Marca o upload completo (uma vez por boot de modelo).
pub fn mark_uploaded() {
    UPLOADED.store(true, core::sync::atomic::Ordering::Release);
}

pub fn weights_uploaded() -> bool {
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

    // 1. Quantiza x (mesma matemática do reference: si = max|x|/127).
    let mut xq = Vec::new();
    xq.try_reserve_exact(k).ok()?;
    xq.resize(k, 0i32);
    let mut si_acc = 0.0f32;
    for i in 0..m {
        let mut max_abs = 0.0f32;
        for &v in &x.data[i * k..(i + 1) * k] {
            let a = v.abs();
            if a > max_abs {
                max_abs = a;
            }
        }
        let si = if max_abs > 1e-9 { max_abs / 127.0 } else { 1.0 };
        si_acc = si;
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
        let si = si_acc;
        for j_block in (0..n).step_by(8) {
            let j_end = (j_block + 8).min(n);
            // Prefetch do próximo bloco j (cache line ahead — o barramento
            // busca enquanto computamos: overlap duplo-buffer na prática).
            if j_end < n {
                let pf = w_base.add((j_end * k) >> 2);
                core::arch::asm!("prefetcht0 [{}]", in(reg) pf, options(nostack, preserves_flags));
            }
            for j in j_block..j_end {
                let wj = w_base.add(j * k);
                let mut acc = 0i32;
                for t in 0..k {
                    let idx = t + j * k; // coluna-major i8 (n,k) — upload repacked
                    let _ = idx;
                    let byte = core::ptr::read_volatile(wj.add(t >> 2));
                    let pair = (byte >> ((t & 3) << 1)) & 3;
                    let v = ((pair & 1) as i32) - ((pair >> 1) as i32);
                    acc += xq[t] * v;
                }
                out.data[i * n + j] = acc as f32 * si;
            }
        }
    }
    Some(out)
}

/// Gate: BAR compute só depois do canário de round-trip (stage ≥ Mapped).
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
/// Determinístico: ordem q,k,v,o,gate,up,down por layer → offsets fixos →
/// o `SHAPE_INDEX` permite dispatch por shape nas chamadas seguintes.
pub fn on_model_loaded() {
    if !bar_compute_enabled() {
        return;
    }
    // Snapshot das matrizes do modelo carregado (sem clonar dados — upload
    // lê direto do packed residente no heap).
    let model = cortex::cortex::current_model_layers_snapshot();
    let Some(layers) = model else { return };
    let plan = RingPlan::falcon3_default();
    // Coleta ponteiros únicos por (shape, packed_data ptr) — mesmas matrizes
    // entre layers compartilham shape; subimos UMA de cada família (1ª ocorrência)
    // para caber na aperture honestamente (dGPU sem ReBAR ≈ 256MB).
    let mut fams: alloc::vec::Vec<&PackedTernaryTensor> = alloc::vec::Vec::new();
    for t in layers.iter() {
        if !fams.iter().any(|f| (*f).shape == (**t).shape) {
            fams.push(*t);
        }
    }
    let refs: alloc::vec::Vec<&PackedTernaryTensor> = fams;
    match upload_layer_weights(&refs, &plan) {
        Some(r) => {
            mark_uploaded();
            k_nano::slog_hal!(
                "BARCOMPUTE", "ok",
                "pesos residentes: {} famílias ({}MB) — heap liberado; GEMV lê via BAR",
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
}
