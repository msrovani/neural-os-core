//! CorePools — P/E affinity pools (ADR-0055). Tamanho = silício, não [8].

use alloc::vec::Vec;
use crate::smp::percpu::{CPU_TYPE_E_CORE, CPU_TYPE_P_CORE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CoreType {
    Performance = 0,
    Efficiency = 1,
}

#[derive(Clone, Copy)]
pub struct CpuInfo {
    pub cpu_id: u16,
    pub lapic_id: u32,
    pub core_type: CoreType,
    pub is_bsp: bool,
}

pub struct CorePools {
    pub ring0: Vec<u16>,
    pub ring1: Vec<u16>,
    pub ring2: Vec<u16>,
}

impl CorePools {
    pub fn empty() -> Self {
        Self {
            ring0: Vec::new(),
            ring1: Vec::new(),
            ring2: Vec::new(),
        }
    }
}

static mut POOLS: CorePools = CorePools {
    ring0: Vec::new(),
    ring1: Vec::new(),
    ring2: Vec::new(),
};
static POOLS_READY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Detecta tipo P/E via CPUID 0x1A (Intel hybrid). AMD/não-híbrido: P.
pub fn detect_core_type() -> CoreType {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max = core::arch::x86_64::__cpuid(0).eax;
        if max < 0x1A {
            return CoreType::Performance;
        }
        let leaf = core::arch::x86_64::__cpuid(0x1A);
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

pub fn assign_cores(cpus: &[CpuInfo]) -> CorePools {
    let mut pools = CorePools::empty();
    let Some(bsp) = cpus.iter().find(|c| c.is_bsp) else {
        return pools;
    };
    pools.ring0.push(bsp.cpu_id);

    for c in cpus.iter().filter(|c| !c.is_bsp) {
        match c.core_type {
            CoreType::Performance => pools.ring1.push(c.cpu_id),
            CoreType::Efficiency => pools.ring2.push(c.cpu_id),
        }
    }
    if pools.ring2.is_empty() && pools.ring1.len() > 1 {
        let move_n = pools.ring1.len() / 2;
        for _ in 0..move_n {
            if let Some(id) = pools.ring1.pop() {
                pools.ring2.push(id);
            }
        }
    }
    pools
}

/// Pools a partir do BSP + APs já acordados (tipo 0x1A por AP se gravado no PerCpu).
pub fn init_from_boot(bsp_lapic: u32, ap_count: u16) {
    let total = (ap_count as usize) + 1;
    let mut cpus = Vec::with_capacity(total);
    let bsp_type = if crate::platform_probe::gate().allow_ep_core_detect {
        detect_core_type()
    } else {
        CoreType::Performance
    };
    cpus.push(CpuInfo {
        cpu_id: 0,
        lapic_id: bsp_lapic,
        core_type: bsp_type,
        is_bsp: true,
    });
    for i in 0..ap_count as usize {
        let (lapic, ctype) = unsafe {
            if let Some(p) = crate::smp::percpu::ap_pcpu_ptr_mut(i) {
                let p = &*p;
                let t = if p.cpu_type == CPU_TYPE_E_CORE {
                    CoreType::Efficiency
                } else {
                    CoreType::Performance
                };
                (p.lapic_id, t)
            } else {
                (0, bsp_type)
            }
        };
        cpus.push(CpuInfo {
            cpu_id: (i as u16) + 1,
            lapic_id: lapic,
            core_type: ctype,
            is_bsp: false,
        });
    }
    let pools = assign_cores(&cpus);
    let (r0, r1, r2) = (pools.ring0.len(), pools.ring1.len(), pools.ring2.len());
    unsafe {
        POOLS = pools;
    }
    POOLS_READY.store(true, core::sync::atomic::Ordering::Release);
    crate::slog_nano!(
        "SMP",
        "ok",
        "CorePools r0={} r1={} r2={} bsp_type={} total={}",
        r0,
        r1,
        r2,
        match bsp_type {
            CoreType::Performance => "P",
            CoreType::Efficiency => "E",
        },
        total
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

pub fn ring_for_cpu(cpu_id: u16) -> u8 {
    let Some(p) = pools() else {
        return 0;
    };
    if p.ring0.iter().any(|&id| id == cpu_id) {
        return 0;
    }
    if p.ring1.iter().any(|&id| id == cpu_id) {
        return 1;
    }
    2
}
