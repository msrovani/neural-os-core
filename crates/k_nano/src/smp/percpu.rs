use core::cell::UnsafeCell;
use core::sync::atomic::AtomicU8;
use x86_64::{PhysAddr, VirtAddr};

#[repr(C)]
pub struct PerCpu {
    pub self_ptr: u64,
    pub cpu_id: u64,
    pub cpu_type: u8,
    pub lapic_id: u8,
    pub is_bsp: bool,
    pub online: u8,
    pub ring: u8,
    pub tss_ptr: u64,           // pointer to this CPU's TSS
    pub ist_stacks: [u64; 3],   // IST stack tops for #DF, #PF, #GP (16KB each)
    _padding: [u8; 19],
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
    _padding: [0u8; 19],
};

pub static CPU_COUNT: AtomicU8 = AtomicU8::new(1);
pub static AP_ONLINE: AtomicU8 = AtomicU8::new(0);

/// ADR-0057 WS-A: máximo de APs suportados (total de cores = MAX_APS + 1 BSP).
pub const MAX_APS: usize = 7;

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
            _padding: [0u8; 19],
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

/// ADR-0057 WS-F / ADR-0065 FASE 3.1 P13: Allocate IST stacks for an AP.
/// Allocates 3 × 16KB contiguous frames via the global allocator.
/// Returns array of stack tops (virtual addresses) for #DF, #PF, #GP.
pub unsafe fn init_ap_ist(ap_index: usize) -> [u64; 3] {
    use crate::memory::GLOBAL_ALLOCATOR;
    use x86_64::VirtAddr;
    use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};

    const IST_STACK_SIZE: usize = 16384; // 16KB
    const IST_COUNT: usize = 3;

    let layout = alloc::alloc::Layout::from_size_align(IST_STACK_SIZE * IST_COUNT, 4096)
        .expect("IST layout");
    let ptr = {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let alloc = guard.as_mut().expect("GLOBAL_ALLOCATOR not initialized");
        // Allocate 3 contiguous frames (48KB total)
        let mut frames = alloc::vec::Vec::new();
        for _ in 0..(IST_STACK_SIZE * IST_COUNT / 4096) {
            if let Some(frame) = alloc.allocate_frame() {
                frames.push(frame);
            } else {
                panic!("IST alloc failed");
            }
        }
        // Get the first frame's start address
        PhysAddr::new(frames[0].start_address().as_u64()).as_u64() as *mut u8
    };
    let base = VirtAddr::from_ptr(ptr);
    let top = base + (IST_STACK_SIZE * IST_COUNT);

    // Return stack tops for each IST (growing down from top)
    [
        top.as_u64(),                           // #DF IST (top of 1st 16KB)
        (top - IST_STACK_SIZE).as_u64(),        // #PF IST (top of 2nd 16KB)
        (top - 2 * IST_STACK_SIZE).as_u64(),    // #GP IST (top of 3rd 16KB)
    ]
}
