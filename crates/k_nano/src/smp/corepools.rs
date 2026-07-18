//! CorePools — P/E affinity pools (ADR-0055 / ex-ADR-0014 §SMP).

use crate::smp::percpu::{CPU_TYPE_E_CORE, CPU_TYPE_P_CORE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CoreType {
    Performance = 0,
    Efficiency = 1,
}

#[derive(Clone, Copy)]
pub struct CpuInfo {
    pub cpu_id: u8,
    pub lapic_id: u8,
    pub core_type: CoreType,
    pub is_bsp: bool,
    pub physical_core: u8,
}

pub struct CorePools {
    pub ring0: [u8; 8],
    pub ring0_len: u8,
    pub ring1: [u8; 16],
    pub ring1_len: u8,
    pub ring2: [u8; 32],
    pub ring2_len: u8,
}

impl CorePools {
    pub const fn empty() -> Self {
        Self {
            ring0: [0; 8],
            ring0_len: 0,
            ring1: [0; 16],
            ring1_len: 0,
            ring2: [0; 32],
            ring2_len: 0,
        }
    }
}

static mut POOLS: CorePools = CorePools::empty();
static POOLS_READY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Detecta tipo P/E via CPUID 0x1A (Intel hybrid). Fallback: todos P.
pub fn detect_core_type() -> CoreType {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max = core::arch::x86_64::__cpuid(0).eax;
        if max < 0x1A {
            return CoreType::Performance;
        }
        let leaf = core::arch::x86_64::__cpuid(0x1A);
        // EAX[31:24] = core type: 0x20 = Atom (E), 0x40 = Core (P) — Intel SDM
        let kind = (leaf.eax >> 24) & 0xFF;
        match kind {
            0x20 => CoreType::Efficiency,
            0x40 => CoreType::Performance,
            _ => CoreType::Performance,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        CoreType::Performance
    }
}

/// Monta pools a partir de lista de CPUs (BSP = ring0).
pub fn assign_cores(cpus: &[CpuInfo]) -> CorePools {
    let mut pools = CorePools::empty();
    let Some(bsp) = cpus.iter().find(|c| c.is_bsp) else {
        return pools;
    };
    pools.ring0[0] = bsp.cpu_id;
    pools.ring0_len = 1;

    for c in cpus.iter().filter(|c| !c.is_bsp) {
        match c.core_type {
            CoreType::Performance => {
                if (pools.ring1_len as usize) < pools.ring1.len() {
                    pools.ring1[pools.ring1_len as usize] = c.cpu_id;
                    pools.ring1_len += 1;
                }
            }
            CoreType::Efficiency => {
                if (pools.ring2_len as usize) < pools.ring2.len() {
                    pools.ring2[pools.ring2_len as usize] = c.cpu_id;
                    pools.ring2_len += 1;
                }
            }
        }
    }
    // Sem E-cores: metade dos P extras → ring2
    if pools.ring2_len == 0 && pools.ring1_len > 1 {
        let move_n = pools.ring1_len / 2;
        for i in 0..move_n {
            let idx = (pools.ring1_len - 1 - i) as usize;
            let id = pools.ring1[idx];
            pools.ring2[pools.ring2_len as usize] = id;
            pools.ring2_len += 1;
        }
        pools.ring1_len -= move_n;
    }
    pools
}

/// Inicializa pools globais a partir do BSP + AP_COUNT (tipos homogêneos até APs reportarem).
pub fn init_from_boot(bsp_lapic: u8, ap_count: u8) {
    let total = (ap_count as usize) + 1;
    let mut cpus = [CpuInfo {
        cpu_id: 0,
        lapic_id: 0,
        core_type: CoreType::Performance,
        is_bsp: true,
        physical_core: 0,
    }; 32];
    let n = total.min(32);
    let bsp_type = if crate::platform_probe::gate().allow_ep_core_detect {
        detect_core_type()
    } else {
        CoreType::Performance
    };
    for i in 0..n {
        cpus[i] = CpuInfo {
            cpu_id: i as u8,
            lapic_id: if i == 0 { bsp_lapic } else { i as u8 },
            core_type: bsp_type, // APs: refined when each AP runs detect
            is_bsp: i == 0,
            physical_core: i as u8,
        };
    }
    let pools = assign_cores(&cpus[..n]);
    let (r0, r1, r2) = (pools.ring0_len, pools.ring1_len, pools.ring2_len);
    unsafe {
        POOLS = pools;
    }
    POOLS_READY.store(true, core::sync::atomic::Ordering::Release);
    crate::slog_nano!(
        "SMP",
        "info",
        "CorePools r0={} r1={} r2={} bsp_type={}",
        r0,
        r1,
        r2,
        match bsp_type {
            CoreType::Performance => "P",
            CoreType::Efficiency => "E",
        }
    );
    let _ = (CPU_TYPE_P_CORE, CPU_TYPE_E_CORE);
}

pub fn pools() -> Option<&'static CorePools> {
    if POOLS_READY.load(core::sync::atomic::Ordering::Acquire) {
        Some(unsafe { &*core::ptr::addr_of!(POOLS) })
    } else {
        None
    }
}

/// Ring sugerido para cpu_id (0/1/2).
pub fn ring_for_cpu(cpu_id: u8) -> u8 {
    let Some(p) = pools() else {
        return 0;
    };
    for i in 0..p.ring0_len as usize {
        if p.ring0[i] == cpu_id {
            return 0;
        }
    }
    for i in 0..p.ring1_len as usize {
        if p.ring1[i] == cpu_id {
            return 1;
        }
    }
    for i in 0..p.ring2_len as usize {
        if p.ring2[i] == cpu_id {
            return 2;
        }
    }
    2
}
