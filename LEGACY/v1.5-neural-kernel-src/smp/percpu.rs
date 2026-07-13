use core::sync::atomic::AtomicU8;

#[repr(C)]
pub struct PerCpu {
    pub self_ptr: u64,
    pub cpu_id: u64,
    pub cpu_type: u8,
    pub lapic_id: u8,
    pub is_bsp: bool,
    pub online: u8,
    pub ring: u8,
    _padding: [u8; 43],
}

pub const CPU_TYPE_P_CORE: u8 = 0;
pub const CPU_TYPE_E_CORE: u8 = 1;

pub static BSP_PCPU: PerCpu = PerCpu {
    self_ptr: 0,
    cpu_id: 0,
    cpu_type: CPU_TYPE_P_CORE,
    lapic_id: 0,
    is_bsp: true,
    online: 1,
    ring: 0,
    _padding: [0u8; 43],
};

pub static CPU_COUNT: AtomicU8 = AtomicU8::new(1);
pub static AP_ONLINE: AtomicU8 = AtomicU8::new(0);

pub fn init_bsp_percpu(lapic_id: u8) {
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
