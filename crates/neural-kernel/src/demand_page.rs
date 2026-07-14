//! Demand-pager — VAs reservadas (leaf NOT PRESENT), cured no #PF (ADR-0041 P7).
//! Frames pré-alocados no register; path #PF só instala leaf (sem GLOBAL_ALLOCATOR).

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{PageTableFlags, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use x86_64::VirtAddr;

use crate::address_space::{self, AddressSpace};
use crate::sync::irq_lock::IrqSafeLock;

const MAX_REGIONS: usize = 4;
const MAX_PAGES: usize = 16;

#[derive(Clone, Copy)]
struct LazyRegion {
    used: bool,
    l4_phys: u64,
    va_base: u64,
    pages: usize,
    frames: [u64; MAX_PAGES],
    mapped: [bool; MAX_PAGES],
    user: bool,
}

impl LazyRegion {
    const fn empty() -> Self {
        Self {
            used: false,
            l4_phys: 0,
            va_base: 0,
            pages: 0,
            frames: [0; MAX_PAGES],
            mapped: [false; MAX_PAGES],
            user: false,
        }
    }
}

struct Registry {
    regions: [LazyRegion; MAX_REGIONS],
}

static REGISTRY: IrqSafeLock<Registry> = IrqSafeLock::new(Registry {
    regions: [
        LazyRegion::empty(),
        LazyRegion::empty(),
        LazyRegion::empty(),
        LazyRegion::empty(),
    ],
});

static CURE_COUNT: AtomicU64 = AtomicU64::new(0);
static FAIL_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn cure_count() -> u64 {
    CURE_COUNT.load(Ordering::Relaxed)
}

/// Registra range lazy: frames já alocados/preenchidos; reserva PT sem PRESENT.
pub unsafe fn register_lazy(
    aspace: &mut AddressSpace,
    va_base: u64,
    frames: &[PhysFrame<Size4KiB>],
    user: bool,
) -> Result<(), &'static str> {
    let n = frames.len();
    if n == 0 || n > MAX_PAGES {
        return Err("p7: lazy pages invalido");
    }
    for i in 0..n {
        let va = VirtAddr::new(va_base + (i as u64) * 4096);
        aspace.reserve_page(va, user)?;
    }
    let mut guard = REGISTRY.lock();
    let slot = guard
        .regions
        .iter_mut()
        .find(|r| !r.used)
        .ok_or("p7: registry cheio")?;
    *slot = LazyRegion::empty();
    slot.used = true;
    slot.l4_phys = aspace.l4_frame.start_address().as_u64();
    slot.va_base = va_base;
    slot.pages = n;
    slot.user = user;
    for i in 0..n {
        slot.frames[i] = frames[i].start_address().as_u64();
        slot.mapped[i] = false;
    }
    Ok(())
}

/// Tenta curar #PF em CR2. true = leaf instalada (retry insn); false = não é lazy / falha.
pub fn try_handle_fault(cr2: u64) -> bool {
    let Some(mut guard) = REGISTRY.try_lock() else {
        FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
        return false;
    };
    let (l4, _) = Cr3::read();
    let l4_phys = l4.start_address().as_u64();
    for region in guard.regions.iter_mut() {
        if !region.used || region.l4_phys != l4_phys {
            continue;
        }
        if cr2 < region.va_base {
            continue;
        }
        let page = ((cr2 - region.va_base) / 4096) as usize;
        if page >= region.pages {
            continue;
        }
        if region.mapped[page] {
            return true;
        }
        let frame =
            PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(region.frames[page]));
        let va = VirtAddr::new(region.va_base + (page as u64) * 4096);
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        if region.user {
            flags.insert(PageTableFlags::USER_ACCESSIBLE);
        }
        match unsafe { address_space::install_present_leaf_current(va, frame, flags) } {
            Ok(()) => {
                region.mapped[page] = true;
                CURE_COUNT.fetch_add(1, Ordering::Relaxed);
                return true;
            }
            Err(_) => {
                FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        }
    }
    false
}
