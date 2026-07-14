//! Cortex weight mmap — páginas de peso em VA fixo (ADR-0041 P5).
//! PoC: memória simulada (não GGUF/FAT); demand-paging = TODO page-fault.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::VirtAddr;

use crate::address_space::{self, AddressSpace};
use crate::serial_println;
use crate::syscall::{self, Cap, SYS_MAP_WEIGHTS};

/// VA base dos pesos no AddressSpace Cortex (após K_IA_DMA_VA).
pub const CORTEX_WEIGHT_VA: u64 = 0x0000_7000_0030_0000;
/// Páginas "peso" no PoC (simula mmap sem carregar 8GB no heap).
pub const DEMO_WEIGHT_PAGES: usize = 4;
const MAX_WEIGHT_FRAMES: usize = 16;
/// Magic na primeira palavra de cada página de peso (simulado).
const WEIGHT_MAGIC_BASE: u64 = 0xC007_E500;

static MMAP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Frames de peso eager-mapped (simulação; GGUF/FAT mmap = nice-to-have).
struct WeightStore {
    frames: [Option<PhysFrame<Size4KiB>>; MAX_WEIGHT_FRAMES],
    len: usize,
    /// TODO: false = demand-paging (present no first fault); PoC usa eager=true.
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
}

/// Aloca N páginas de peso e mapeia em `CORTEX_WEIGHT_VA`. Exige Cap::MAP_WEIGHTS.
/// Eager-map no PoC; page-fault on first touch = TODO (flag `eager` documentada).
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
    // Eager = true no PoC. Demand-paging: marcar NOT PRESENT + #PF handler (TODO).
    store.eager = true;
    let flags = address_space::rw_flags();
    let first = store.frames[start].ok_or("p5: weight frame vazio")?;
    if store.eager {
        for i in 0..n {
            let frame = store.frames[start + i].ok_or("p5: weight miss")?;
            let va = VirtAddr::new(CORTEX_WEIGHT_VA + (i as u64) * 4096);
            aspace.map_page(va, frame, flags)?;
        }
    }
    MMAP_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(WeightMap {
        virt: CORTEX_WEIGHT_VA,
        phys: first.start_address().as_u64(),
        pages: n,
    })
}

pub fn mmap_count() -> u64 {
    MMAP_COUNT.load(Ordering::Relaxed)
}

/// Demo non-fatal: deny → mmap SUCCESS → touch first weight page → restore CR3.
pub fn demo_cortex_mmap() -> Result<(), &'static str> {
    serial_println!("[P5] Cortex weight mmap demo (eager PoC; demand-paging TODO)");

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
