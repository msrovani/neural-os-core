//! GDT do kernel — ADR-0088: o teto de 8 slots da crate `x86_64` 0.14 **não** é o silício.
//! Observe = MADT/MAX_APS; Act = 1 TSS por CPU na tabela; Verify = ltr+sti no AP.

use crate::smp::percpu::MAX_APS;
use core::ptr::addr_of;
use core::sync::atomic::{AtomicU16, Ordering};
use x86_64::instructions::tables::{lgdt, DescriptorTablePointer};
use x86_64::structures::gdt::{Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::{PrivilegeLevel, VirtAddr};

pub const N_CPU: usize = MAX_APS + 1;
/// null + KCS + KDS + UCS + UDS + 2 u64 por TSS.
const GDT_U64: usize = 1 + 4 + N_CPU * 2;

#[repr(C, align(16))]
struct KernelGdt {
    table: [u64; GDT_U64],
}

static mut KERNEL_GDT: KernelGdt = KernelGdt {
    table: [0; GDT_U64],
};
static GDT_LIMIT: AtomicU16 = AtomicU16::new(0);

pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    pub tss_selectors: [SegmentSelector; N_CPU],
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

/// Preenche GDT com 1 TSS por CPU. `bsp_tss` força IST do BSP (SESSION_251).
///
/// # Safety
/// `tss_array` vive o restante do boot; descriptors apontam para esses TSS.
pub unsafe fn build(bsp_tss: &'static TaskStateSegment, tss_array: *mut [TaskStateSegment; N_CPU]) -> Selectors {
    let table = &mut KERNEL_GDT.table;
    let mut idx = 1; // slot 0 = null

    let code_selector = push_desc(table, &mut idx, Descriptor::kernel_code_segment());
    let data_selector = push_desc(table, &mut idx, Descriptor::kernel_data_segment());
    let user_code_selector = push_desc(table, &mut idx, Descriptor::user_code_segment());
    let user_data_selector = push_desc(table, &mut idx, Descriptor::user_data_segment());

    let mut tss_selectors = [SegmentSelector::new(0, PrivilegeLevel::Ring0); N_CPU];
    tss_selectors[0] = push_desc(table, &mut idx, Descriptor::tss_segment(bsp_tss));
    for i in 1..N_CPU {
        let tss_ptr = &mut (*tss_array)[i] as *mut TaskStateSegment;
        tss_selectors[i] = push_desc(
            table,
            &mut idx,
            Descriptor::tss_segment_unchecked(tss_ptr),
        );
    }

    GDT_LIMIT.store((idx * 8 - 1) as u16, Ordering::Release);
    crate::slog_nano!(
        "GDT",
        "info",
        "AIOS GDT cpus={} limit={} (crate-8-slot bypass removido)",
        N_CPU,
        GDT_LIMIT.load(Ordering::Relaxed)
    );

    Selectors {
        code_selector,
        data_selector,
        user_code_selector,
        user_data_selector,
        tss_selectors,
    }
}

pub fn load() {
    let limit = GDT_LIMIT.load(Ordering::Acquire);
    let base = VirtAddr::new(unsafe { addr_of!(KERNEL_GDT.table) as u64 });
    let ptr = DescriptorTablePointer { limit, base };
    unsafe { lgdt(&ptr) };
}