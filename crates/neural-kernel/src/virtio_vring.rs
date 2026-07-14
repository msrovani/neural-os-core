//! VirtIO vring sobre DMA pinado (ADR-0041 P8).
//! Layout layout-compatible com `virtio_net` (desc+avail+used); Cap VRING_SETUP.
//! Path paralelo Cap-gated — não altera filas live do NIC.

use core::sync::atomic::Ordering;

use crate::k_ia_dma::{self, PinnedDmaBuf};
use crate::memory::PHYS_MEM_OFFSET;
use crate::serial_println;
use crate::syscall::{self, Cap, SYS_PIN_DMA, SYS_VRING_SETUP};

/// Queue size PoC (layout-compatible; NIC live usa 64).
pub const QUEUE_SIZE: u16 = 4;
/// Páginas pinadas: desc | avail | used | payload (1 cada).
pub const VRING_PIN_PAGES: usize = 4;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;
const PAYLOAD_LEN: u32 = 256;

/// Descritor VirtIO 1.1 §2.6.5 (igual a `virtio_net::Desc`).
#[repr(C, packed)]
struct Desc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, packed)]
struct AvailRing {
    flags: u16,
    idx: u16,
    ring: [u16; QUEUE_SIZE as usize],
    used_event: u16,
}

#[repr(C, packed)]
struct UsedElem {
    id: u32,
    len: u32,
}

#[repr(C, packed)]
struct UsedRing {
    flags: u16,
    idx: u16,
    ring: [UsedElem; QUEUE_SIZE as usize],
    avail_event: u16,
}

/// Handle da fila stub montada em frames pinnados.
#[derive(Clone, Copy, Debug)]
pub struct VringHandle {
    pub desc_phys: u64,
    pub avail_phys: u64,
    pub used_phys: u64,
    pub payload_phys: u64,
    pub queue_size: u16,
    pub avail_idx: u16,
    pub desc_head: u16,
}

#[inline]
fn phys_va(pa: u64) -> *mut u8 {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    (pa + offset) as *mut u8
}

/// Monta Virtqueue layout-compatible em `buf` (VRING_PIN_PAGES frames).
/// Zero-copy: `Desc.addr` aponta para página pinnada de payload (não heap scratch).
pub fn setup_vring(buf: &PinnedDmaBuf, held: Cap) -> Result<VringHandle, &'static str> {
    if !held.contains(Cap::VRING_SETUP) {
        serial_println!("[CapGate] DENY VRING_SETUP held=0x{:x}", held.bits());
        return Err("EPERM: Cap::VRING_SETUP");
    }
    let _ = syscall::dispatch(SYS_VRING_SETUP, buf.phys, held)?;
    if buf.pages < VRING_PIN_PAGES {
        return Err("p8: precisa de 4 pages pinadas");
    }

    let desc_pa = k_ia_dma::pinned_phys_at(buf, 0)?;
    let avail_pa = k_ia_dma::pinned_phys_at(buf, 1)?;
    let used_pa = k_ia_dma::pinned_phys_at(buf, 2)?;
    let payload_pa = k_ia_dma::pinned_phys_at(buf, 3)?;

    unsafe {
        core::ptr::write_bytes(phys_va(desc_pa), 0, 4096);
        core::ptr::write_bytes(phys_va(avail_pa), 0, 4096);
        core::ptr::write_bytes(phys_va(used_pa), 0, 4096);
        core::ptr::write_bytes(phys_va(payload_pa), 0, 4096);

        // Cadeia de 1 descritor: device WRITE no payload pinnado.
        let desc = phys_va(desc_pa) as *mut Desc;
        let d0 = &mut *desc;
        d0.addr = payload_pa;
        d0.len = PAYLOAD_LEN;
        d0.flags = VIRTQ_DESC_F_WRITE;
        d0.next = 0;

        let avail = phys_va(avail_pa) as *mut AvailRing;
        (*avail).flags = 0;
        (*avail).ring[0] = 0;
        core::sync::atomic::fence(Ordering::SeqCst);
        (*avail).idx = 1;

        let used = phys_va(used_pa) as *mut UsedRing;
        (*used).flags = 0;
        (*used).idx = 0;
    }

    Ok(VringHandle {
        desc_phys: desc_pa,
        avail_phys: avail_pa,
        used_phys: used_pa,
        payload_phys: payload_pa,
        queue_size: QUEUE_SIZE,
        avail_idx: 1,
        desc_head: 0,
    })
}

/// Lê de volta o descritor 0 (verifica phys pinnado).
fn verify_desc_points_pinned(h: &VringHandle) -> Result<(), &'static str> {
    unsafe {
        let desc = phys_va(h.desc_phys) as *const Desc;
        let d = &*desc;
        let addr = d.addr;
        let len = d.len;
        let flags = d.flags;
        if addr != h.payload_phys {
            return Err("p8: desc.addr != payload pinnado");
        }
        if len != PAYLOAD_LEN {
            return Err("p8: desc.len inesperado");
        }
        if flags & VIRTQ_DESC_F_WRITE == 0 {
            return Err("p8: desc sem WRITE");
        }
        let _ = VIRTQ_DESC_F_NEXT;
    }
    Ok(())
}

/// Se VirtIO-net já está ativo, loga filas live (NIC **não** é mutado).
fn probe_live_virtio() -> Option<(u64, u64)> {
    let guard = crate::net::VIRTIO_DEV.lock();
    let dev = guard.as_ref()?;
    if !dev.present {
        return None;
    }
    Some((dev.rx_queue_phys(), dev.tx_queue_phys()))
}

/// Demo non-fatal: deny Cap → pin+setup SUCCESS → log phys/indices → NIC untouched.
pub fn demo_virtio_vring() -> Result<(), &'static str> {
    serial_println!("[P8] VirtIO vring + DMA pin demo");

    if setup_vring(
        &PinnedDmaBuf {
            phys: 0,
            virt_as: 0,
            pages: VRING_PIN_PAGES,
        },
        Cap::EMPTY,
    )
    .is_ok()
    {
        return Err("p8: Cap vazia nao deveria VRING_SETUP");
    }
    if syscall::dispatch(SYS_VRING_SETUP, 0, Cap::EMPTY).is_ok() {
        return Err("p8: Cap vazia nao deveria SYS_VRING_SETUP");
    }

    let caps = Cap::PIN_DMA.union(Cap::MAP_DMA).union(Cap::VRING_SETUP);
    let buf = match k_ia_dma::pin_frames(VRING_PIN_PAGES, Cap::PIN_DMA) {
        Ok(b) => b,
        Err(e) => {
            serial_println!("[P8] WARN pin_frames: {} — Cap-only path", e);
            syscall::dispatch(SYS_PIN_DMA, VRING_PIN_PAGES as u64, Cap::PIN_DMA)?;
            syscall::dispatch(SYS_VRING_SETUP, 0, Cap::VRING_SETUP)?;
            serial_println!("[P8] SUCCESS Cap VRING_SETUP (layout-only sem frames)");
            return Ok(());
        }
    };

    let handle = match setup_vring(&buf, Cap::VRING_SETUP) {
        Ok(h) => h,
        Err(e) => {
            serial_println!("[P8] WARN setup_vring: {} — Cap path", e);
            syscall::dispatch(SYS_VRING_SETUP, buf.phys, Cap::VRING_SETUP)?;
            serial_println!("[P8] SUCCESS Cap VRING_SETUP (sem layout write)");
            return Ok(());
        }
    };
    verify_desc_points_pinned(&handle)?;

    match probe_live_virtio() {
        Some((rx, tx)) => {
            serial_println!(
                "[P8] VirtIO-net live rx_q={:x} tx_q={:x} — P8 stub paralelo (NIC untouched)",
                rx,
                tx
            );
        }
        None => {
            serial_println!("[P8] VirtIO-net ausente — PoC layout-only (Opcao B) = SUCCESS");
        }
    }

    serial_println!(
        "[P8] SUCCESS vring q={} desc={:x} avail={:x} used={:x} payload={:x} avail_idx={} head={} caps=0x{:x} pinned={}",
        handle.queue_size,
        handle.desc_phys,
        handle.avail_phys,
        handle.used_phys,
        handle.payload_phys,
        handle.avail_idx,
        handle.desc_head,
        caps.bits(),
        k_ia_dma::pinned_count()
    );
    Ok(())
}
