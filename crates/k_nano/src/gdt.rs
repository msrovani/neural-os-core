//! GDT do kernel — ADR-0088 / ADR-0100 T-037: 1 TSS por CPU **no MADT**, não 511 no .bss.
//! Early: 5 segmentos + TSS BSP. Após MADT: tabela no heap + TSS dos APs (Box::leak).

use core::ptr::addr_of;
use core::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};
use x86_64::instructions::tables::{lgdt, DescriptorTablePointer};
use x86_64::structures::gdt::{Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::{PrivilegeLevel, VirtAddr};

/// Slots early: null + KCS + KDS + UCS + UDS + TSS BSP (2×u64).
const EARLY_U64: usize = 1 + 4 + 2;

#[repr(C, align(16))]
struct KernelGdtEarly {
    table: [u64; EARLY_U64],
}

static mut KERNEL_GDT_EARLY: KernelGdtEarly = KernelGdtEarly {
    table: [0; EARLY_U64],
};
static GDT_BASE: AtomicU64 = AtomicU64::new(0);
static GDT_LIMIT: AtomicU16 = AtomicU16::new(0);
static TSS_SEL_PTR: AtomicUsize = AtomicUsize::new(0);
static TSS_SEL_LEN: AtomicUsize = AtomicUsize::new(0);

pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    /// Só BSP no early boot; APs via `tss_selector(i)`.
    pub tss_selectors: [SegmentSelector; 1],
}

fn push_desc(table: &mut [u64], idx: &mut usize, desc: Descriptor) -> SegmentSelector {
    let index = *idx as u16;
    match desc {
        Descriptor::UserSegment(v) => {
            table[*idx] = v;
            *idx += 1;
            let rpl = PrivilegeLevel::from_u16(((v >> 45) & 3) as u16);
            SegmentSelector::new(index, rpl)
        }
        Descriptor::SystemSegment(lo, hi) => {
            table[*idx] = lo;
            table[*idx + 1] = hi;
            *idx += 2;
            SegmentSelector::new(index, PrivilegeLevel::Ring0)
        }
    }
}

fn publish_table(base: u64, n_u64: usize) {
    GDT_BASE.store(base, Ordering::Release);
    GDT_LIMIT.store((n_u64 * 8 - 1) as u16, Ordering::Release);
}

/// Early GDT (BSP only). Sem array 511.
///
/// # Safety
/// `bsp_tss` vive o resto do boot.
pub unsafe fn build_early(bsp_tss: &'static TaskStateSegment) -> Selectors {
    let table = &mut KERNEL_GDT_EARLY.table;
    let mut idx = 1;
    let code_selector = push_desc(table, &mut idx, Descriptor::kernel_code_segment());
    let data_selector = push_desc(table, &mut idx, Descriptor::kernel_data_segment());
    let user_code_selector = push_desc(table, &mut idx, Descriptor::user_code_segment());
    let user_data_selector = push_desc(table, &mut idx, Descriptor::user_data_segment());
    let tss0 = push_desc(table, &mut idx, Descriptor::tss_segment(bsp_tss));
    publish_table(addr_of!(KERNEL_GDT_EARLY.table) as u64, idx);

    let sels = alloc::boxed::Box::leak(alloc::vec![tss0].into_boxed_slice());
    TSS_SEL_PTR.store(sels.as_ptr() as usize, Ordering::Release);
    TSS_SEL_LEN.store(1, Ordering::Release);

    crate::slog_nano!(
        "GDT",
        "info",
        "early GDT cpus=1 limit={} (T-037 heap APs depois do MADT)",
        GDT_LIMIT.load(Ordering::Relaxed)
    );

    Selectors {
        code_selector,
        data_selector,
        user_code_selector,
        user_data_selector,
        tss_selectors: [tss0],
    }
}

/// Compat: mesmo que `build_early` (ignora array BSS legado).
pub unsafe fn build(
    bsp_tss: &'static TaskStateSegment,
    _tss_array: *mut TaskStateSegment,
) -> Selectors {
    build_early(bsp_tss)
}

pub fn tss_selector(i: usize) -> Option<SegmentSelector> {
    let n = TSS_SEL_LEN.load(Ordering::Acquire);
    if i >= n {
        return None;
    }
    let p = TSS_SEL_PTR.load(Ordering::Acquire) as *const SegmentSelector;
    if p.is_null() {
        return None;
    }
    Some(unsafe { *p.add(i) })
}

/// Reconstrói GDT com 1+n_aps TSS. Índices CS/DS/UCS/UDS/TSS0 iguais ao early.
///
/// # Safety
/// `ap_tss` leaked; chamar antes do wake dos APs. BSP recarrega CS+ltr.
pub unsafe fn expand_for_aps(
    n_aps: usize,
    ap_tss: &'static mut [TaskStateSegment],
    bsp_tss: &'static TaskStateSegment,
) -> bool {
    if n_aps == 0 || ap_tss.len() < n_aps {
        return false;
    }
    let n_cpu = 1 + n_aps;
    let n_u64 = 1 + 4 + n_cpu * 2;
    let mut table = alloc::vec![0u64; n_u64];
    let mut idx = 1;
    let _cs = push_desc(&mut table, &mut idx, Descriptor::kernel_code_segment());
    let _ds = push_desc(&mut table, &mut idx, Descriptor::kernel_data_segment());
    let _ucs = push_desc(&mut table, &mut idx, Descriptor::user_code_segment());
    let _uds = push_desc(&mut table, &mut idx, Descriptor::user_data_segment());
    let mut sels = alloc::vec![SegmentSelector::new(0, PrivilegeLevel::Ring0); n_cpu];
    sels[0] = push_desc(&mut table, &mut idx, Descriptor::tss_segment(bsp_tss));
    for i in 0..n_aps {
        let tss_ptr = &mut ap_tss[i] as *mut TaskStateSegment;
        sels[i + 1] = push_desc(
            &mut table,
            &mut idx,
            Descriptor::tss_segment_unchecked(tss_ptr),
        );
    }
    let leaked = alloc::boxed::Box::leak(table.into_boxed_slice());
    publish_table(leaked.as_ptr() as u64, idx);
    let sel_leak = alloc::boxed::Box::leak(sels.into_boxed_slice());
    TSS_SEL_PTR.store(sel_leak.as_ptr() as usize, Ordering::Release);
    TSS_SEL_LEN.store(n_cpu, Ordering::Release);
    crate::slog_nano!(
        "GDT",
        "info",
        "expand GDT cpus={} limit={} (heap, T-037)",
        n_cpu,
        GDT_LIMIT.load(Ordering::Relaxed)
    );
    true
}

pub fn load() {
    let limit = GDT_LIMIT.load(Ordering::Acquire);
    let base = GDT_BASE.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    let ptr = DescriptorTablePointer {
        limit,
        base: VirtAddr::new(base),
    };
    unsafe { lgdt(&ptr) };
}

/// T-038: early table is 7 u64, not 511×TSS.
pub fn early_gdt_u64_slots() -> usize {
    EARLY_U64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_gdt_not_511() {
        assert_eq!(early_gdt_u64_slots(), 7);
        assert!(core::mem::size_of::<KernelGdtEarly>() <= 128);
    }
}
