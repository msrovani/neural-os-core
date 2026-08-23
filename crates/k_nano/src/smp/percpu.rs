use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU16, AtomicUsize, Ordering};

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

/// BSP PerCpu — `static mut` (não `static`): `init_bsp_percpu` escreve self_ptr/lapic.
/// `static` ia para .rodata → #PF err=3 no QEMU TCG (K225).
pub static mut BSP_PCPU: PerCpu = PerCpu {
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

/// T-037: não é teto de silício. Slots AP = MADT, alocados no heap.
pub fn ap_slots() -> usize {
    AP_PCPU_LEN.load(Ordering::Acquire) as usize
}

fn empty_pcpu() -> PerCpu {
    PerCpu {
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
    }
}

static AP_PCPU_PTR: AtomicUsize = AtomicUsize::new(0);
static AP_PCPU_LEN: AtomicU16 = AtomicU16::new(0);

/// Aloca PerCpu[n_aps] no heap (leak). 1c → n_aps=0, sem reserva.
pub fn alloc_slots(n_aps: usize) -> bool {
    if n_aps == 0 {
        AP_PCPU_LEN.store(0, Ordering::Release);
        return true;
    }
    let mut v = alloc::vec::Vec::with_capacity(n_aps);
    for _ in 0..n_aps {
        v.push(UnsafeCell::new(empty_pcpu()));
    }
    let leaked = alloc::boxed::Box::leak(v.into_boxed_slice());
    AP_PCPU_PTR.store(leaked.as_mut_ptr() as usize, Ordering::Release);
    AP_PCPU_LEN.store(n_aps as u16, Ordering::Release);
    crate::slog_nano!("SMP", "info", "PerCpu heap slots={} (T-037, sem BSS 511)", n_aps);
    true
}

pub fn ap_pcpu_ptr_mut(i: usize) -> Option<*mut PerCpu> {
    let n = ap_slots();
    if i >= n {
        return None;
    }
    let p = AP_PCPU_PTR.load(Ordering::Acquire) as *mut UnsafeCell<PerCpu>;
    if p.is_null() {
        return None;
    }
    Some(unsafe { (*p.add(i)).get() })
}

/// Ponteiro (u64) para o PerCpu do AP de índice `i` (para patch do trampoline).
pub fn ap_percpu_ptr(i: usize) -> u64 {
    let Some(p) = ap_pcpu_ptr_mut(i) else {
        return 0;
    };
    unsafe {
        (*p).self_ptr = p as u64;
        (*p).cpu_id = (i as u64) + 1;
    }
    p as u64
}

pub fn init_bsp_percpu(lapic_id: u32) {
    unsafe {
        let pcpu = core::ptr::addr_of_mut!(BSP_PCPU);
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

#[cfg(test)]
mod tests {
    #[test]
    fn no_511_ap_array_in_bss() {
        // Slots começam em 0; 1c não reserva PerCpu[511].
        assert_eq!(super::ap_slots(), 0);
    }
}

/// IST por AP: 3 stacks no heap (VA contígua). Frames físicos soltos não servem de stack.
pub unsafe fn init_ap_ist(_ap_index: usize) -> Option<[u64; 3]> {
    fn one_stack() -> Option<u64> {
        alloc_mapped_stack(IST_STACK_SIZE)
    }
    Some([one_stack()?, one_stack()?, one_stack()?])
}

/// Topo (cresce para baixo) de um buffer no bump/TALC — VA mapeada. Não usar HEAP_START fantasma.
pub fn alloc_mapped_stack(size: usize) -> Option<u64> {
    let layout = alloc::alloc::Layout::from_size_align(size, 16).ok()?;
    let p = unsafe { alloc::alloc::alloc_zeroed(layout) };
    if p.is_null() {
        return None;
    }
    Some((p as u64) + size as u64)
}
