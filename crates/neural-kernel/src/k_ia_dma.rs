//! K-IA DMA pin — frames físicos não-reclaimáveis + map AS (ADR-0041 P5).
//! Stub VirtIO: buffer pinned + phys addr pronto; ring real = follow-up.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::VirtAddr;

use crate::address_space::{self, AddressSpace};
use crate::serial_println;
use crate::syscall::{self, Cap, SYS_MAP_DMA, SYS_PIN_DMA};

/// VA base do buffer DMA no AddressSpace K-IA (após JARBAS_FB_VA).
pub const K_IA_DMA_VA: u64 = 0x0000_7000_0020_0000;
/// Páginas pinadas no PoC (VirtIO vring tipicamente ≤2 páginas para stub).
pub const DEMO_PIN_PAGES: usize = 2;
const MAX_PINNED: usize = 16;

static PIN_COUNT: AtomicU64 = AtomicU64::new(0);

/// Registro de frames pinados (não devolvidos ao frame allocator).
struct PinRegistry {
    frames: [Option<PhysFrame<Size4KiB>>; MAX_PINNED],
    len: usize,
}

impl PinRegistry {
    const fn empty() -> Self {
        Self {
            frames: [None; MAX_PINNED],
            len: 0,
        }
    }
}

static mut PIN_REG: PinRegistry = PinRegistry::empty();

/// Handle: buffer DMA pinado pronto para VirtIO (phys contiguidade best-effort).
#[derive(Clone, Copy, Debug)]
pub struct PinnedDmaBuf {
    pub phys: u64,
    pub virt_as: u64,
    pub pages: usize,
}

/// Aloca e registra `n` frames como não-reclaimáveis. Exige Cap::PIN_DMA.
pub fn pin_frames(n: usize, held: Cap) -> Result<PinnedDmaBuf, &'static str> {
    if !held.contains(Cap::PIN_DMA) {
        serial_println!("[CapGate] DENY PIN_DMA held=0x{:x}", held.bits());
        return Err("EPERM: Cap::PIN_DMA");
    }
    let _ = syscall::dispatch(SYS_PIN_DMA, n as u64, held)?;
    if n == 0 || n > MAX_PINNED {
        return Err("p5: n frames invalido");
    }
    let reg = unsafe { &mut *core::ptr::addr_of_mut!(PIN_REG) };
    if reg.len + n > MAX_PINNED {
        return Err("p5: pin registry cheio");
    }
    let start_slot = reg.len;
    for i in 0..n {
        let frame = match address_space::alloc_frame() {
            Ok(f) => f,
            Err(e) => {
                // Non-fatal path: não panica; limpa slots desta tentativa.
                for j in 0..i {
                    reg.frames[start_slot + j] = None;
                }
                reg.len = start_slot;
                return Err(e);
            }
        };
        reg.frames[start_slot + i] = Some(frame);
        reg.len = start_slot + i + 1;
    }
    let first = reg.frames[start_slot].ok_or("p5: pin slot vazio")?;
    PIN_COUNT.fetch_add(n as u64, Ordering::Relaxed);
    Ok(PinnedDmaBuf {
        phys: first.start_address().as_u64(),
        virt_as: 0,
        pages: n,
    })
}

/// Mapeia frames pinados em `K_IA_DMA_VA` no AS K-IA. Exige Cap::MAP_DMA.
pub unsafe fn map_pinned(
    aspace: &mut AddressSpace,
    buf: &mut PinnedDmaBuf,
    held: Cap,
) -> Result<u64, &'static str> {
    if !held.contains(Cap::MAP_DMA) {
        serial_println!("[CapGate] DENY MAP_DMA held=0x{:x}", held.bits());
        return Err("EPERM: Cap::MAP_DMA");
    }
    let _ = syscall::dispatch(SYS_MAP_DMA, buf.phys, held)?;
    let reg = &*core::ptr::addr_of!(PIN_REG);
    let flags = address_space::rw_flags();
    // Localiza slot do first phys (últimos `pages` pinados com esse phys).
    let mut start = None;
    for i in 0..reg.len {
        if let Some(f) = reg.frames[i] {
            if f.start_address().as_u64() == buf.phys {
                start = Some(i);
                break;
            }
        }
    }
    let start = start.ok_or("p5: buf nao pinado")?;
    for i in 0..buf.pages {
        let frame = reg.frames[start + i].ok_or("p5: frame pin ausente")?;
        let va = VirtAddr::new(K_IA_DMA_VA + (i as u64) * 4096);
        aspace.map_page(va, frame, flags)?;
    }
    buf.virt_as = K_IA_DMA_VA;
    Ok(K_IA_DMA_VA)
}

/// Unpin opcional — devolve frames ao allocator (só PoC; VirtIO real não unpin mid-IO).
pub fn unpin_frames(buf: &PinnedDmaBuf, held: Cap) -> Result<(), &'static str> {
    if !held.contains(Cap::PIN_DMA) {
        return Err("EPERM: Cap::PIN_DMA");
    }
    let reg = unsafe { &mut *core::ptr::addr_of_mut!(PIN_REG) };
    let mut start = None;
    for i in 0..reg.len {
        if let Some(f) = reg.frames[i] {
            if f.start_address().as_u64() == buf.phys {
                start = Some(i);
                break;
            }
        }
    }
    let start = start.ok_or("p5: unpin miss")?;
    for i in 0..buf.pages {
        if let Some(frame) = reg.frames[start + i].take() {
            unsafe { crate::memory::dealloc_physical_frame(frame) };
            PIN_COUNT.fetch_sub(1, Ordering::Relaxed);
        }
    }
    // Compacta registry (simples: move cauda).
    let end = start + buf.pages;
    let mut w = start;
    for r in end..reg.len {
        reg.frames[w] = reg.frames[r].take();
        w += 1;
    }
    reg.len = w;
    Ok(())
}

pub fn pinned_count() -> u64 {
    PIN_COUNT.load(Ordering::Relaxed)
}

/// Phys do buffer pinado (stub "VirtIO buffer ready").
pub fn virtio_buf_phys(buf: &PinnedDmaBuf) -> u64 {
    buf.phys
}

/// Demo non-fatal: deny Cap → pin+map SUCCESS → log phys VirtIO stub → restore CR3.
pub fn demo_kia_dma() -> Result<(), &'static str> {
    serial_println!("[P5] K-IA DMA pin demo");

    if pin_frames(DEMO_PIN_PAGES, Cap::EMPTY).is_ok() {
        return Err("p5: Cap vazia nao deveria PIN_DMA");
    }
    if syscall::dispatch(SYS_MAP_DMA, 0, Cap::EMPTY).is_ok() {
        return Err("p5: Cap vazia nao deveria MAP_DMA");
    }

    let mut buf = match pin_frames(DEMO_PIN_PAGES, Cap::PIN_DMA) {
        Ok(b) => b,
        Err(e) => {
            serial_println!("[P5] WARN pin_frames: {} — Cap-only path", e);
            syscall::dispatch(SYS_PIN_DMA, DEMO_PIN_PAGES as u64, Cap::PIN_DMA)?;
            syscall::dispatch(SYS_MAP_DMA, 0, Cap::MAP_DMA)?;
            serial_println!("[P5] SUCCESS Cap PIN_DMA/MAP_DMA (sem frames)");
            return Ok(());
        }
    };

    let (kernel_l4, kernel_flags) = address_space::kernel_cr3();
    let mut as_kia = AddressSpace::clone_current()?;
    if unsafe { map_pinned(&mut as_kia, &mut buf, Cap::EMPTY) }.is_ok() {
        return Err("p5: Cap vazia nao deveria mapear DMA");
    }
    let mapped = unsafe { map_pinned(&mut as_kia, &mut buf, Cap::MAP_DMA)? };

    let touch_ok = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        as_kia.activate();
        let p = mapped as *mut u64;
        p.write_volatile(0xDEAD_D5A5);
        let got = p.read_volatile();
        address_space::restore_cr3(kernel_l4, kernel_flags);
        got == 0xDEAD_D5A5
    });
    if !touch_ok {
        return Err("p5: touch DMA VA falhou");
    }

    serial_println!(
        "[P5] SUCCESS DMA pin pages={} phys={:x} va={:x} (VirtIO buf stub ready) pinned={}",
        buf.pages,
        virtio_buf_phys(&buf),
        mapped,
        pinned_count()
    );
    // Mantém pinado no boot PoC (não unpin) — frames não-reclaimáveis até reboot.
    Ok(())
}
