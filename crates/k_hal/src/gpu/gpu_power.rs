//! GPU power wake D3→D0 via PCI PMCSR — s423 (H3 sobrevive ao D-state).
//!
//! SESSION_260 descobriu que a dGPU do notebook dorme (D3) no boot: BARs
//! visíveis, mas tocar VRAM/BAR = hang de barramento PCIe. Os gates do
//! `init_vram_tier`/`nvidia::probe` recusavam honestamente — o H3 (hints
//! neurais via aperture) e o lane VRAM (ADR-0112) morriam em qualquer
//! notebook. Este módulo faz o power-on ANTES do mapeamento:
//!
//! Contrato de segurança (nunca hang):
//! 1. **Prova de vida ANTES do write** — o CF8/CFC do próprio device responde
//!    `0xFFFF` quando o device está D3cold/ausente do barramento (config
//!    space morto). Escrever PMCSR nesse estado = completion abortando no
//!    chip set = freeze. Recusa honesta ANTES de qualquer write.
//! 2. **Write via PMCSR** (cap PM id 0x01, offset+4, bits [1:0] = 00 = D0,
//!    PME_Status bit15 limpo) — `pci::pci_power_on_d0` já faz isso com
//!    readback; aqui só orquestramos prova de vida + budget + settle.
//! 3. **Budget TSC** no poll do D-state pós-write (lição SESSION_354:
//!    timeout sem budget = hang honesto disfarçado).
//! 4. **D3cold (PMCSR[7:4] com PME assist) não é wake por software** —
//!    se o device não responde config (prova de vida falha), é D3cold ou
//!    ausente: skip honesto, nunca write às cegas.
//!
//! Honesty: wake ok = slog ok + re-scan; wake falho = slog warn + recusa
//! (o caller mantém o comportamento fail-closed de hoje).

use core::sync::atomic::Ordering;

/// Resultado do wake (telemetria honesta p/ painel/log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeResult {
    /// Já estava em D0/D1/D2 — nada a fazer.
    AlreadyAwake,
    /// D3→D0 completou: PMCSR aceitou e o re-scan confirma ≤D2.
    Woke,
    /// Device sem config viva (D3cold/ausente) — recusa ANTES do write.
    NoLife,
    /// PMCSR aceitou o write mas o D-state pós-settle não voltou a ≤D2.
    Stuck,
    /// Sem PM capability (0xFF da convenção pci_power_state) — assume D0.
    NoPmCap,
}

impl WakeResult {
    pub fn as_str(self) -> &'static str {
        match self {
            WakeResult::AlreadyAwake => "already-awake",
            WakeResult::Woke => "woke",
            WakeResult::NoLife => "no-life (d3cold/ausente)",
            WakeResult::Stuck => "stuck",
            WakeResult::NoPmCap => "no-pm-cap",
        }
    }
}

/// Budget de settle D3→D0 (µs). PCI PM spec §5.4 dá até 10ms para D3hot→D0
/// (o pci_power_on_d0 faz 1 dummy read — barato; aqui o poll re-temperado
/// cobre plataformas lentas sem estourar o boot).
const SETTLE_TIMEOUT_US: u64 = 10_000;

/// Prova de vida: config space do device responde algo ≠ 0xFFFF (vendor ID).
/// Unsafe: roda CF8/CFC direto (domínio 0).
unsafe fn config_alive(bus: u8, dev: u8, func: u8) -> bool {
    let vid = k_nano::pci::read_config_word(bus, dev, func, 0x00);
    vid != 0xFFFF
}

/// Poll do D-state pós-write com budget TSC. Retorna o último D-state lido.
unsafe fn poll_dstate(bus: u8, dev: u8, func: u8) -> u8 {
    let t0 = k_nano::tsc::now_us();
    let mut d = 0xFFu8;
    loop {
        let (state, _) = k_nano::pci::pci_power_state(bus, dev, func);
        d = state;
        if state <= 2 || state == 0xFF {
            return d;
        }
        if k_nano::tsc::now_us().saturating_sub(t0) > SETTLE_TIMEOUT_US {
            return d;
        }
        core::hint::spin_loop();
    }
}

/// Tenta acordar a GPU (D3→D0 via PMCSR) antes de qualquer acesso a BAR/VRAM.
/// Não-op em D0–D2. Puro no sentido de não tocar MMIO do device — só config.
/// `gpu` é mutável porque o `pci_dstate` re-medido é persistido (os gates
/// `power_ok_for_compute`/`init_vram_tier`/`nvidia::probe` enxergam D0 real).
pub fn wake_to_d0(gpu: &mut crate::gpu::detect::GpuInfo) -> WakeResult {
    if gpu.pci_dstate <= 2 {
        return WakeResult::AlreadyAwake;
    }
    if gpu.pci_dstate == 0xFF {
        return WakeResult::NoPmCap;
    }
    unsafe {
        // 1) Prova de vida ANTES de qualquer write (regra anti-hang).
        let (bus, dev, func) = (gpu.pci_bus, gpu.pci_dev, gpu.pci_fn);
        if !config_alive(bus, dev, func) {
            k_nano::slog_hal!(
                "GPUPWR", "warn",
                "{}: config space morto (vid=0xFFFF) — D3cold/ausente, sem write às cegas (skip honesto)",
                gpu.name
            );
            return WakeResult::NoLife;
        }
        // 2) Write D0 via PMCSR (com readback interno) + settle com budget.
        k_nano::slog_hal!(
            "GPUPWR", "info",
            "{}: D-state={} — wake D3→D0 via PMCSR (H3/lane sobrevivem ao D-state)",
            gpu.name, gpu.pci_dstate
        );
        let after_write = k_nano::pci::pci_power_on_d0(bus, dev, func);
        let after_settle = if after_write == 0 { after_write } else { poll_dstate(bus, dev, func) };
        // 3) Persiste o re-scan — os gates existentes passam a ver D0.
        gpu.pci_dstate = after_settle;
        if after_settle <= 2 {
            k_nano::slog_hal!(
                "GPUPWR", "ok",
                "{}: acordou (D-state={} pós-settle) — VRAM liberada p/ map/compute",
                gpu.name, after_settle
            );
            return WakeResult::Woke;
        }
        k_nano::slog_hal!(
            "GPUPWR", "warn",
            "{}: PMCSR não trouxe a D0 (stuck em D-state={}) — skip honesto",
            gpu.name, after_settle
        );
        WakeResult::Stuck
    }
}

/// Conveniência para detect/display_coex: wake in-place numa lista e retorna
/// quantos acordaram (telemetria 1 linha no boot).
pub fn wake_all(gpus: &mut [crate::gpu::detect::GpuInfo]) -> usize {
    let mut woke = 0usize;
    for g in gpus.iter_mut() {
        if wake_to_d0(g) == WakeResult::Woke {
            woke += 1;
        }
    }
    if woke > 0 {
        k_nano::slog_hal!("GPUPWR", "ok", "{} dGPU(s) acordada(s) D3→D0 no boot", woke);
    }
    woke
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_result_honesto() {
        assert_eq!(WakeResult::Woke.as_str(), "woke");
        assert_eq!(WakeResult::NoLife.as_str(), "no-life (d3cold/ausente)");
        assert_eq!(WakeResult::AlreadyAwake.as_str(), "already-awake");
        assert_eq!(WakeResult::Stuck.as_str(), "stuck");
        assert_eq!(WakeResult::NoPmCap.as_str(), "no-pm-cap");
    }

    #[test]
    fn d0_d1_d2_sao_already_awake_sem_tocar_pmcsr() {
        // Host-testável: ≤2 é no-op ANTES de qualquer acesso de config
        // (determinístico — não depende de HW).
        let mut g = stub_gpu(0);
        assert_eq!(wake_to_d0(&mut g), WakeResult::AlreadyAwake);
        let mut g1 = stub_gpu(1);
        assert_eq!(wake_to_d0(&mut g1), WakeResult::AlreadyAwake);
        let mut g2 = stub_gpu(2);
        assert_eq!(wake_to_d0(&mut g2), WakeResult::AlreadyAwake);
    }

    #[test]
    fn sem_pm_cap_e_noop() {
        let mut g = stub_gpu(0xFF);
        assert_eq!(wake_to_d0(&mut g), WakeResult::NoPmCap);
        assert_eq!(g.pci_dstate, 0xFF); // não persistiu nada
    }

    fn stub_gpu(dstate: u8) -> crate::gpu::detect::GpuInfo {
        crate::gpu::detect::GpuInfo {
            vendor: crate::gpu::detect::GpuVendor::Nvidia,
            arch: crate::gpu::detect::GpuArch::NvidiaPascal,
            device_id: 0x25ac,
            bar0: 0,
            bar2: 0,
            vram_size: 0,
            has_display_engine: false,
            has_compute: false,
            is_integrated: false,
            pci_bus: 0,
            pci_dev: 0,
            pci_fn: 0,
            pci_dstate: dstate,
            name: "stub dGPU",
            backend_kind: crate::gpu::compute_abi::ComputeBackendKind::LegacyAcr,
            isa_tag: crate::gpu::compute_abi::IsaTag::Sm61,
            compute_candidate: true,
        }
    }
}
