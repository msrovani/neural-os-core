use core::cell::UnsafeCell;
use core::sync::atomic::AtomicU16;

#[repr(C)]
pub struct PerCpu {
    pub self_ptr: u64,
    pub cpu_id: u64,
    pub cpu_type: u8,
    pub lapic_id: u32,
    pub is_bsp: bool,
    pub online: u8,
    pub ring: u8,
    pub tss_ptr: u64,           // pointer to this CPU's TSS
    pub ist_stacks: [u64; 3],   // IST stack tops for #DF, #PF, #GP (16KB each)
    _padding: [u8; 16],
}

pub const CPU_TYPE_P_CORE: u8 = 0;
pub const CPU_TYPE_E_CORE: u8 = 1;

/// ADR-0057 WS-F / ADR-0065 FASE 3.1 P13: IST stack size per AP (16KB each for #DF, #PF, #GP).
pub const IST_STACK_SIZE: usize = 16384; // 16KB per IST
/// Number of IST stacks per AP (#DF, #PF, #GP).
pub const IST_COUNT: usize = 3;

pub static BSP_PCPU: PerCpu = PerCpu {
    self_ptr: 0,
    cpu_id: 0,
    cpu_type: CPU_TYPE_P_CORE,
    lapic_id: 0,
    is_bsp: true,
    online: 1,
    ring: 0,
    tss_ptr: 0,
    ist_stacks: [0, 0, 0],
    _padding: [0u8; 16],
};

pub static CPU_COUNT: AtomicU16 = AtomicU16::new(1);
pub static AP_ONLINE: AtomicU16 = AtomicU16::new(0);

/// Tamanho do array BSS PerCpu/TSS (implementação, não política).
/// O silício é o MADT Enabled. Se o MADT for maior, é dívida: Vec no boot, não “usar menos cores”.
pub const MAX_APS: usize = 511;

/// Array de PerCpu por-AP. Cada AP recebe GS.base próprio (não mais o BSP
/// compartilhado — causa do não-wake com ≥2 APs).
pub struct ApPcpuArray(pub [UnsafeCell<PerCpu>; MAX_APS]);
unsafe impl Sync for ApPcpuArray {}

pub static AP_PCPU: ApPcpuArray = ApPcpuArray(
    [const {
        UnsafeCell::new(PerCpu {
            self_ptr: 0,
            cpu_id: 0,
            cpu_type: CPU_TYPE_P_CORE,
            lapic_id: 0,
            is_bsp: false,
            online: 0,
            ring: 1,
            tss_ptr: 0,
            ist_stacks: [0, 0, 0],
            _padding: [0u8; 16],
        })
    }; MAX_APS],
);

/// Ponteiro (u64) para o PerCpu do AP de índice `i` (para patch do trampoline).
pub fn ap_percpu_ptr(i: usize) -> u64 {
    if i >= MAX_APS {
        return 0;
    }
    let p = AP_PCPU.0[i].get();
    unsafe {
        (*p).self_ptr = p as u64;
        (*p).cpu_id = (i as u64) + 1;
    }
    p as u64
}

pub fn init_bsp_percpu(lapic_id: u32) {
    let pcpu = &BSP_PCPU as *const PerCpu as *mut PerCpu;
    unsafe {
        (*pcpu).self_ptr = pcpu as u64;
        (*pcpu).lapic_id = lapic_id;
        set_gs_base(pcpu as u64);
    }
}

pub unsafe fn set_gs_base(base: u64) {
    core::arch::asm!(
        "wrmsr",
        in("ecx") 0xC0000101u32,
        in("eax") base as u32,
        in("edx") (base >> 32) as u32,
        options(nostack, preserves_flags)
    );
}

pub fn this_cpu() -> &'static PerCpu {
    let ptr: u64;
    unsafe {
        core::arch::asm!(
            "mov {0}, gs:[0]",
            out(reg) ptr,
            options(nostack, preserves_flags, readonly)
        );
    }
    unsafe { &*(ptr as *const PerCpu) }
}

pub fn cpu_id() -> u64 {
    let id: u64;
    unsafe {
        core::arch::asm!(
            "mov {0}, gs:[8]",
            out(reg) id,
            options(nostack, preserves_flags, readonly)
        );
    }
    id
}

/// IST por AP: 3 stacks no heap (VA contígua). Frames físicos soltos não servem de stack.
pub unsafe fn init_ap_ist(_ap_index: usize) -> Option<[u64; 3]> {
    fn one_stack() -> Option<u64> {
        let layout = alloc::alloc::Layout::from_size_align(IST_STACK_SIZE, 16).ok()?;
        let p = unsafe { alloc::alloc::alloc(layout) };
        if p.is_null() {
            return None;
        }
        Some((p as u64) + IST_STACK_SIZE as u64)
    }
    Some([one_stack()?, one_stack()?, one_stack()?])
}
