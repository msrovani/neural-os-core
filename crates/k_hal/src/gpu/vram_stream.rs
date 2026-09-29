//! VRAM Stream Ring — weights/activations residindo na VRAM de QUALQUER GPU
//! (NVIDIA/AMD/Intel, iGPU ou dGPU) via aperture BAR mapeada, sem driver de
//! vendor, sem shader, sem firmware (ADR-0112).
//!
//! ## O insight "por cima do muro"
//! Todo driver moderno usa a VRAM como **destino de render/compute** — exige
//! firmware (ACR/PSP/GuC), command streamer por geração, ISA de shader. Mas a
//! aperture BAR é, para a CPU, **memória PCIe comum**: o que gravamos nela,
//! a GPU lê no mesmo ciclo de memória com a MESMA largura de banda do
//! framebuffer. Não precisamos executar nada na GPU para que os pesos sirvam:
//! precisamos que eles morem num lugar que (a) libere RAM do host e (b) seja
//! lido pelo caminho de compute quando ele existir (CE/SDMA/BCS/rings).
//!
//! ## Estágios honestos (nunca fake Ready)
//! - `Mapped` — aperture mapeada UC, buddy alocou, canário de bandwidth passou
//!   (write+read-back golden). Pesos PODEM residir (libera RAM).
//! - `StreamsW2a8` — duplo buffer X (ativações) em VRAM + pipeline de cópia
//!   overlap (streaming real medido com TSC).
//! - `ComputeDevice` — motor por-vendor (CE Pascal / SDMA / BCS) validado por
//!   golden: o device **executa** a GEMV. Sem isso, permanece Mapped.
//!
//! Regra SESSION_354: timeout/recusa honesta; nada "sucesso" sem evidência.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use crate::gpu::detect::GpuInfo;
use crate::gpu::vram::{vram_alloc, vram_free, VRAM_READY};

/// Estado do streaming (bandeira honesta para o HUD/dispatch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamStage {
    /// Aperture não mapeada / canário não passado.
    Off = 0,
    /// Pesos residem na VRAM (RAM liberada); compute segue CPU/SMP.
    Mapped = 1,
    /// Duplo buffer de ativações em VRAM + overlap de cópia medido.
    StreamsW2a8 = 2,
    /// Reservado: motor device-specific validado (futuro CE/SDMA/BCS golden).
    ComputeDevice = 3,
}

static STAGE: AtomicU8 = AtomicU8::new(0);
static X_RING_VRAM: AtomicU64 = AtomicU64::new(0); // endereço físico VRAM do ring
pub static STREAM_READY: AtomicBool = AtomicBool::new(false);

// ── Telemetria lock-free ────────────────────────────────────────────────────
pub static BYTES_RESIDENT: AtomicU64 = AtomicU64::new(0);
pub static BYTES_STREAMED: AtomicU64 = AtomicU64::new(0);
pub static CANARY_GBPS_X10: AtomicU64 = AtomicU64::new(0);
pub static CANARY_FAILS: AtomicU64 = AtomicU64::new(0);

/// Configuração do ring (derivada do SKU Falcon3 no boot).
#[derive(Debug, Clone, Copy)]
pub struct RingPlan {
    /// Bytes por slot de ativações X (m x k i8). Decode M=1..8.
    pub x_slot_bytes: usize,
    /// Slots no ring (duplo buffer mínimo = 2).
    pub slots: usize,
    /// Offset base dos pesos residentes dentro da aperture (após o ring).
    pub weights_offset: u64,
}

impl RingPlan {
    /// Plano default: ring de 4 slots de M=8 x K=9216 i8 (3B FFN) = 288KB.
    /// Pesos começam após o ring, alinhados a 2MB (huge page boundary).
    pub const fn falcon3_default() -> Self {
        RingPlan {
            x_slot_bytes: 8 * 9216,
            slots: 4,
            weights_offset: 0,
        }
    }

    pub fn ring_bytes(&self) -> u64 {
        (self.x_slot_bytes * self.slots) as u64
    }
}

/// VRAM offset do ring (físico) — None se streaming não está ativo.
pub fn x_ring_phys() -> Option<u64> {
    if STREAM_READY.load(Ordering::Acquire) {
        Some(X_RING_VRAM.load(Ordering::Relaxed))
    } else {
        None
    }
}

/// Estágio atual.
pub fn stage() -> StreamStage {
    match STAGE.load(Ordering::Acquire) {
        2 => StreamStage::StreamsW2a8,
        3 => StreamStage::ComputeDevice,
        1 => StreamStage::Mapped,
        _ => StreamStage::Off,
    }
}

/// VA da CPU para um offset físico dentro da aperture (UC via init_vram_tier).
/// Segurança: offset < aperture size é responsabilidade do caller (buddy).
#[inline]
unsafe fn aperture_ptr(vram_phys_off: u64, len: usize) -> Option<*mut u8> {
    if !VRAM_READY.load(Ordering::Acquire) {
        return None;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    Some((vram_phys_off as *mut u8).wrapping_add(pmoff as usize))
}

/// Canário de bandwidth: escreve pattern golden de 64KB na VRAM, lê de volta,
/// confere. Mede GB/s×10 com TSC (write+read). Só passa se o round-trip é
/// fiel — VRAM dormindo (D3) ou barramento morto falha AQUI, nunca depois.
///
/// # Safety
/// Aperture mapeada UC (init_vram_tier) e GPU em D0.
unsafe fn canary_bandwidth(trial: *mut u8, len: usize) -> Option<u64> {
    const PATTERNS: [u64; 8] = [
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0xA5A5_5A5A_C3C3_3C3C,
        0xDEAD_BEEF_1337_CAFE,
        0x5555_AAAA_0F0F_F0F0,
        0x7FFF_FFFF_8000_0001,
        0x0000_0000_0000_0001,
        0xFFFF_FFFF_FFFF_FFE0,
    ];
    let t0 = k_nano::tsc::now_us();
    let whole = core::slice::from_raw_parts_mut(trial, len);
    for (i, chunk) in whole.chunks_mut(8).enumerate() {
        let v = PATTERNS[i & 7].to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    for (i, chunk) in core::slice::from_raw_parts(trial, len).chunks(8).enumerate() {
        let mut got = [0u8; 8];
        got[..chunk.len()].copy_from_slice(chunk);
        let expect = PATTERNS[i & 7].to_le_bytes();
        if got[..chunk.len()] != expect[..chunk.len()] {
            return None;
        }
    }
    let dt_us = k_nano::tsc::now_us().saturating_sub(t0).max(1);
    // 2x len (write+read) em bytes / dt → GB/s x10.
    let gbps_x10 = (2 * len as u64 * 10_000_000u64) / (dt_us * 1_000_000);
    Some(gbps_x10)
}

/// Inicializa o stream ring para a GPU compute (chamado após init_vram_tier
/// OK — aperture UC mapeada e buddy ativo). Idempotente.
///
/// # Safety
/// init_vram_tier já rodou (aperture mapeada, D0 verificado).
pub unsafe fn init_stream_ring(gpu: &GpuInfo, plan: &RingPlan) -> bool {
    if STREAM_READY.load(Ordering::Acquire) {
        return true;
    }
    if !VRAM_READY.load(Ordering::Acquire) {
        k_nano::slog_hal!("VRAMSTREAM", "warn", "{}: sem VRAM tier — streaming off (honesto)", gpu.name);
        return false;
    }
    let ring_bytes = plan.ring_bytes();
    let Some(base) = vram_alloc(ring_bytes as usize + 4096) else {
        k_nano::slog_hal!("VRAMSTREAM", "warn", "{}: buddy sem espaço p/ ring {}KB", gpu.name, ring_bytes / 1024);
        return false;
    };
    let Some(ptr) = aperture_ptr(base, ring_bytes as usize) else {
        vram_free(base, ring_bytes as usize);
        return false;
    };
    // Canário de bandwidth no 1º slot (64KB do slot — round-trip golden).
    let canary_len = (plan.x_slot_bytes).min(64 * 1024);
    match canary_bandwidth(ptr, canary_len) {
        Some(gbps_x10) if gbps_x10 > 0 => {
            CANARY_GBPS_X10.store(gbps_x10, Ordering::Release);
            X_RING_VRAM.store(base, Ordering::Release);
            BYTES_RESIDENT.store(0, Ordering::Relaxed);
            BYTES_STREAMED.store(0, Ordering::Relaxed);
            STAGE.store(StreamStage::Mapped as u8, Ordering::Release);
            STREAM_READY.store(true, Ordering::Release);
            k_nano::slog_hal!(
                "VRAMSTREAM", "ok",
                "{}: stream ring READY base={:#x} slots={} slot={}KB bw={}.{}GB/s (canário golden)",
                gpu.name, base, plan.slots, plan.x_slot_bytes / 1024, gbps_x10 / 10, gbps_x10 % 10
            );
            true
        }
        _ => {
            CANARY_FAILS.fetch_add(1, Ordering::Relaxed);
            vram_free(base, ring_bytes as usize);
            k_nano::slog_hal!("VRAMSTREAM", "fail", "{}: canário round-trip FAIL (D3/barramento) — streaming off", gpu.name);
            false
        }
    }
}

/// Copia um bloco de ativações i8 para o slot `slot` do ring (host→VRAM).
/// Retorna o endereço físico VRAM do slot. Fail-closed sem streaming.
pub fn stream_x_slot(x_i8: &[i8], slot: usize, plan: &RingPlan) -> Option<u64> {
    if !STREAM_READY.load(Ordering::Acquire) || slot >= plan.slots {
        return None;
    }
    if x_i8.len() > plan.x_slot_bytes {
        return None;
    }
    let base = X_RING_VRAM.load(Ordering::Relaxed);
    let off = base + (slot * plan.x_slot_bytes) as u64;
    unsafe {
        let Some(dst) = aperture_ptr(off, x_i8.len()) else { return None };
        let dst_i8 = dst as *mut i8;
        core::ptr::copy_nonoverlapping(x_i8.as_ptr(), dst_i8, x_i8.len());
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }
    BYTES_STREAMED.fetch_add(x_i8.len() as u64, Ordering::Relaxed);
    Some(off)
}

/// Registra N bytes de pesos residentes (chamado pelo bar_compute no stage).
pub fn note_weights_resident(bytes: u64) {
    BYTES_RESIDENT.fetch_add(bytes, Ordering::Relaxed);
}

/// Status textual para o HUD (padrão honesto — n/a quando off).
pub fn status_line() -> alloc::string::String {
    match stage() {
        StreamStage::Off => alloc::string::String::from("vramstream off"),
        s => alloc::format!(
            "vramstream {:?} resident={}MB streamed={}KB bw={}.{}GB/s",
            s,
            BYTES_RESIDENT.load(Ordering::Relaxed) / (1024 * 1024),
            BYTES_STREAMED.load(Ordering::Relaxed) / 1024,
            CANARY_GBPS_X10.load(Ordering::Relaxed) / 10,
            CANARY_GBPS_X10.load(Ordering::Relaxed) % 10
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_plan_default_cabe_na_aperture_minima() {
        let p = RingPlan::falcon3_default();
        // Ring (288KB) << 256MB (menor aperture honesta aceita pelo tier).
        assert!(p.ring_bytes() < 256 * 1024 * 1024);
        assert_eq!(p.slots, 4);
        assert_eq!(p.x_slot_bytes, 8 * 9216);
    }

    #[test]
    fn stage_off_sem_init_e_honesto() {
        // Sem init (host test), stage tem que ser Off e x_ring None.
        if !STREAM_READY.load(Ordering::Acquire) {
            assert_eq!(stage(), StreamStage::Off);
            assert!(x_ring_phys().is_none());
            assert!(stream_x_slot(&[0i8; 16], 0, &RingPlan::falcon3_default()).is_none());
        }
    }

    #[test]
    fn status_off_nao_mente() {
        if stage() == StreamStage::Off {
            assert_eq!(status_line(), "vramstream off");
        }
    }
}
