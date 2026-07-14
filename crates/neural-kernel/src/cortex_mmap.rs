//! Cortex weight mmap — páginas de peso em VA fixo (ADR-0041 P5/P7).
//! P5: eager map; P7: lazy reserve + demand-paging via #PF.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::VirtAddr;

use crate::address_space::{self, AddressSpace};
use crate::demand_page;
use crate::serial_println;
use crate::syscall::{self, Cap, SYS_DEMAND_PAGE, SYS_MAP_WEIGHTS};

/// VA base dos pesos no AddressSpace Cortex (após K_IA_DMA_VA).
pub const CORTEX_WEIGHT_VA: u64 = 0x0000_7000_0030_0000;
/// Páginas "peso" no PoC (simula mmap sem carregar 8GB no heap).
pub const DEMO_WEIGHT_PAGES: usize = 4;
const MAX_WEIGHT_FRAMES: usize = 16;
/// Magic na primeira palavra de cada página de peso (simulado).
const WEIGHT_MAGIC_BASE: u64 = 0xC007_E500;

static MMAP_COUNT: AtomicU64 = AtomicU64::new(0);
static LAZY_COUNT: AtomicU64 = AtomicU64::new(0);

/// Frames de peso (eager ou pré-allocados para lazy).
struct WeightStore {
    frames: [Option<PhysFrame<Size4KiB>>; MAX_WEIGHT_FRAMES],
    len: usize,
    eager: bool,
}

impl WeightStore {
    const fn empty() -> Self {
        Self {
            frames: [None; MAX_WEIGHT_FRAMES],
            len: 0,
            eager: true,
        }
    }
}

static mut WEIGHT_STORE: WeightStore = WeightStore::empty();

/// Handle do mmap de pesos (VA Cortex + phys first page).
#[derive(Clone, Copy, Debug)]
pub struct WeightMap {
    pub virt: u64,
    pub phys: u64,
    pub pages: usize,
    pub lazy: bool,
}

unsafe fn alloc_weight_frames(n: usize) -> Result<(usize, PhysFrame<Size4KiB>), &'static str> {
    if n == 0 || n > MAX_WEIGHT_FRAMES {
        return Err("p5: weight pages invalido");
    }
    let store = &mut *core::ptr::addr_of_mut!(WEIGHT_STORE);
    if store.len + n > MAX_WEIGHT_FRAMES {
        return Err("p5: weight store cheio");
    }
    let start = store.len;
    for i in 0..n {
        let frame = match address_space::alloc_frame() {
            Ok(f) => f,
            Err(e) => {
                for j in 0..i {
                    if let Some(fr) = store.frames[start + j].take() {
                        crate::memory::dealloc_physical_frame(fr);
                    }
                }
                store.len = start;
                return Err(e);
            }
        };
        let hhdm = address_space::hhdm_mut::<u64>(frame);
        core::ptr::write_volatile(hhdm, WEIGHT_MAGIC_BASE.wrapping_add(i as u64));
        store.frames[start + i] = Some(frame);
        store.len = start + i + 1;
    }
    let first = store.frames[start].ok_or("p5: weight frame vazio")?;
    Ok((start, first))
}

/// Aloca N páginas de peso e mapeia eagerly em `CORTEX_WEIGHT_VA`. Cap::MAP_WEIGHTS.
pub unsafe fn mmap_weights(
    aspace: &mut AddressSpace,
    n: usize,
    held: Cap,
) -> Result<WeightMap, &'static str> {
    if !held.contains(Cap::MAP_WEIGHTS) {
        serial_println!("[CapGate] DENY MAP_WEIGHTS held=0x{:x}", held.bits());
        return Err("EPERM: Cap::MAP_WEIGHTS");
    }
    let _ = syscall::dispatch(SYS_MAP_WEIGHTS, n as u64, held)?;
    let (start, first) = alloc_weight_frames(n)?;
    let store = &mut *core::ptr::addr_of_mut!(WEIGHT_STORE);
    store.eager = true;
    let flags = address_space::rw_flags();
    for i in 0..n {
        let frame = store.frames[start + i].ok_or("p5: weight miss")?;
        let va = VirtAddr::new(CORTEX_WEIGHT_VA + (i as u64) * 4096);
        aspace.map_page(va, frame, flags)?;
    }
    MMAP_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(WeightMap {
        virt: CORTEX_WEIGHT_VA,
        phys: first.start_address().as_u64(),
        pages: n,
        lazy: false,
    })
}

/// Reserva N páginas lazy (NOT PRESENT) + registra demand-pager. Cap MAP_WEIGHTS|DEMAND_PAGE.
pub unsafe fn mmap_weights_lazy(
    aspace: &mut AddressSpace,
    n: usize,
    held: Cap,
) -> Result<WeightMap, &'static str> {
    if !held.contains(Cap::MAP_WEIGHTS) || !held.contains(Cap::DEMAND_PAGE) {
        serial_println!(
            "[CapGate] DENY MAP_WEIGHTS|DEMAND_PAGE held=0x{:x}",
            held.bits()
        );
        return Err("EPERM: Cap::MAP_WEIGHTS|DEMAND_PAGE");
    }
    let _ = syscall::dispatch(SYS_MAP_WEIGHTS, n as u64, held)?;
    let _ = syscall::dispatch(SYS_DEMAND_PAGE, n as u64, held)?;
    let (start, first) = alloc_weight_frames(n)?;
    let store = &mut *core::ptr::addr_of_mut!(WEIGHT_STORE);
    store.eager = false;
    let mut frames = [PhysFrame::containing_address(x86_64::PhysAddr::new(0)); MAX_WEIGHT_FRAMES];
    for i in 0..n {
        frames[i] = store.frames[start + i].ok_or("p7: weight miss")?;
    }
    demand_page::register_lazy(aspace, CORTEX_WEIGHT_VA, &frames[..n], false)?;
    LAZY_COUNT.fetch_add(1, Ordering::Relaxed);
    MMAP_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(WeightMap {
        virt: CORTEX_WEIGHT_VA,
        phys: first.start_address().as_u64(),
        pages: n,
        lazy: true,
    })
}

pub fn mmap_count() -> u64 {
    MMAP_COUNT.load(Ordering::Relaxed)
}

/// Demo non-fatal P5: deny → mmap eager SUCCESS → touch → restore CR3.
pub fn demo_cortex_mmap() -> Result<(), &'static str> {
    serial_println!("[P5] Cortex weight mmap demo (eager; lazy = P7)");

    if syscall::dispatch(SYS_MAP_WEIGHTS, 0, Cap::EMPTY).is_ok() {
        return Err("p5: Cap vazia nao deveria MAP_WEIGHTS");
    }

    let (kernel_l4, kernel_flags) = address_space::kernel_cr3();
    let mut as_cortex = AddressSpace::clone_current()?;

    if unsafe { mmap_weights(&mut as_cortex, DEMO_WEIGHT_PAGES, Cap::EMPTY) }.is_ok() {
        return Err("p5: Cap vazia nao deveria mmap pesos");
    }

    let map = match unsafe { mmap_weights(&mut as_cortex, DEMO_WEIGHT_PAGES, Cap::MAP_WEIGHTS) }
    {
        Ok(m) => m,
        Err(e) => {
            serial_println!("[P5] WARN mmap_weights: {} — Cap-only path", e);
            syscall::dispatch(SYS_MAP_WEIGHTS, DEMO_WEIGHT_PAGES as u64, Cap::MAP_WEIGHTS)?;
            serial_println!("[P5] SUCCESS Cap MAP_WEIGHTS (sem frames)");
            return Ok(());
        }
    };

    let touch_ok = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        as_cortex.activate();
        let got = (map.virt as *const u64).read_volatile();
        address_space::restore_cr3(kernel_l4, kernel_flags);
        got == WEIGHT_MAGIC_BASE
    });
    if !touch_ok {
        return Err("p5: touch weight page falhou");
    }

    serial_println!(
        "[P5] SUCCESS mmap pesos pages={} va={:x} phys={:x} count={}",
        map.pages,
        map.virt,
        map.phys,
        mmap_count()
    );
    Ok(())
}

/// Demo non-fatal P7: lazy reserve → CR3 cortex → first-touch #PF cured → verify.
pub fn demo_demand_paging() -> Result<(), &'static str> {
    serial_println!("[P7] Demand-paging #PF demo (lazy weights)");

    let need = Cap::MAP_WEIGHTS.union(Cap::DEMAND_PAGE);
    if syscall::dispatch(SYS_DEMAND_PAGE, 0, Cap::EMPTY).is_ok() {
        return Err("p7: Cap vazia nao deveria DEMAND_PAGE");
    }
    if unsafe {
        mmap_weights_lazy(
            &mut AddressSpace::clone_current()?,
            DEMO_WEIGHT_PAGES,
            Cap::MAP_WEIGHTS,
        )
    }
    .is_ok()
    {
        return Err("p7: DEMAND_PAGE ausente deveria negar");
    }

    let (kernel_l4, kernel_flags) = address_space::kernel_cr3();
    let mut as_cortex = AddressSpace::clone_current()?;
    let map = match unsafe { mmap_weights_lazy(&mut as_cortex, DEMO_WEIGHT_PAGES, need) } {
        Ok(m) => m,
        Err(e) => {
            serial_println!("[P7] WARN mmap_weights_lazy: {} — Cap-only path", e);
            syscall::dispatch(SYS_DEMAND_PAGE, DEMO_WEIGHT_PAGES as u64, need)?;
            serial_println!("[P7] SUCCESS Cap DEMAND_PAGE (sem frames)");
            return Ok(());
        }
    };

    let cures_before = demand_page::cure_count();
    let verify = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        as_cortex.activate();
        let p0 = (map.virt as *const u64).read_volatile();
        let p1_ptr = (map.virt + 4096) as *mut u64;
        let p1_magic = p1_ptr.read_volatile();
        p1_ptr.write_volatile(0xDEAD_07CE);
        let p1_rw = p1_ptr.read_volatile();
        address_space::restore_cr3(kernel_l4, kernel_flags);
        p0 == WEIGHT_MAGIC_BASE
            && p1_magic == WEIGHT_MAGIC_BASE.wrapping_add(1)
            && p1_rw == 0xDEAD_07CE
    });
    if !verify {
        return Err("p7: first-touch / verify falhou");
    }
    let cured = demand_page::cure_count().saturating_sub(cures_before);
    if cured < 2 {
        return Err("p7: esperava >=2 cures #PF");
    }

    serial_println!(
        "[P7] SUCCESS lazy pages={} va={:x} cures={} lazy_maps={}",
        map.pages,
        map.virt,
        cured,
        LAZY_COUNT.load(Ordering::Relaxed)
    );
    Ok(())
}
