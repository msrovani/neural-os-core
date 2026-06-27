//! Minimal VirtIO-net driver — bare-metal, sem dependências externas.
//! Implementa o transporte PCI legacy (OASIS VirtIO 1.1 §4.1).
//!
//! ⚠️ Driver manual não 100%: sem IRQ (MSI-X), TX buffers não são reciclados
//! (vazamento de memória em TX), sem validação de integridade de descritores.
//! O `virtio-drivers` crate é a solução correta, mas está bloqueado por
//! `zerocopy-derive` (proc macro incompatível com MinGW toolchain no Windows).
//! Revisitar quando houver crate compatível ou após migração para MSVC.
//!
//! A struct VIRTIO_DEV em net.rs é populada por init_driver_virtio().
//! A NetPhy unificada em netstack.rs tenta RTL8139 e VirtIO.

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};
use x86_64::instructions::port::Port;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
use crate::{serial_println, println};

pub const VIRTIO_VENDOR: u16 = 0x1AF4;
pub const VIRTIO_NET_TRANSITIONAL: u16 = 0x1041; // transitional (legacy + modern)
pub const VIRTIO_NET_MODERN: u16 = 0x1000;       // modern only

const QUEUE_NUM: u16 = 64;  // virtqueues de 64 descritores
const BUF_SIZE: usize = 2048;

// Legacy I/O port offsets
const REG_DEVICE_FEATURES: u16 = 0x00;
const REG_GUEST_FEATURES: u16 = 0x04;
const REG_QUEUE_ADDR: u16 = 0x08;
const REG_QUEUE_SIZE: u16 = 0x0C;
const REG_QUEUE_SEL: u16 = 0x0E;
const REG_QUEUE_NOTIFY: u16 = 0x10;
const REG_STATUS: u16 = 0x12;
const REG_ISR: u16 = 0x13;
const REG_CONFIG: u16 = 0x14; // MAC address at offset 0

const STATUS_ACK: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;

const VIRTIO_NET_F_MAC: u64 = 5; // device has MAC

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
    ring: [u16; QUEUE_NUM as usize],
    used_event: u16,
}

#[repr(C, packed)]
struct UsedElem { id: u32, len: u32 }

#[repr(C, packed)]
struct UsedRing {
    flags: u16,
    idx: u16,
    ring: [UsedElem; QUEUE_NUM as usize],
    avail_event: u16,
}

/// Aloca N páginas físicas contíguas, retorna (phys_addr, virt_addr)
unsafe fn alloc_pages(n: usize) -> Option<(u64, *mut u8)> {
    use x86_64::structures::paging::FrameAllocator;
    let mut guard = GLOBAL_ALLOCATOR.lock();
    let alloc = (*guard).as_mut()?;
    let frame = alloc.allocate_contiguous(n)?;
    let pa = frame.start_address().as_u64();
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    Some((pa, (pa + offset) as *mut u8))
}

/// Desaloca N páginas a partir de phys_addr
unsafe fn dealloc_pages(pa: u64, n: usize) {
    use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB};
    let mut guard = GLOBAL_ALLOCATOR.lock();
    if let Some(alloc) = (*guard).as_mut() {
        let start = PhysFrame::<Size4KiB>::containing_address(
            x86_64::PhysAddr::new(pa));
        for i in 0..n {
            alloc.deallocate_frame(start + i as u64);
        }
    }
}

pub struct VirtIoDevice {
    io_base: u16,
    rx_queue_pa: u64,   // phys base of RX descriptor page
    tx_queue_pa: u64,   // phys base of TX descriptor page
    rx_avail_idx: u16,
    tx_avail_idx: u16,
    rx_used_last: u16,
    pub mac: [u8; 6],
    pub present: bool,
}

impl VirtIoDevice {
    pub fn new(dev: &PciDevice) -> Option<Self> {
        // BAR0 deve ser I/O (bit 0 = 1)
        let io_base = if dev.bar0 & 1 == 1 { (dev.bar0 & !0xFF) as u16 } else { return None; };
        if io_base == 0 { return None; }

        unsafe {
            let io = IoPorts::new(io_base);

            // Reset
            io.write8(REG_STATUS, 0);
            while io.read8(REG_STATUS) != 0 { core::hint::spin_loop(); }

            // Acknowledge + Driver
            io.add_status(STATUS_ACK);
            io.add_status(STATUS_DRIVER);

            // Features: só pedimos MAC
            let features = io.read32(REG_DEVICE_FEATURES) as u64
                | ((io.read32(REG_DEVICE_FEATURES + 4) as u64) << 32);
            let guest = features | (1 << VIRTIO_NET_F_MAC);
            io.write32(REG_GUEST_FEATURES, guest as u32);
            io.write32(REG_GUEST_FEATURES + 4, (guest >> 32) as u32);

            // FEATURES_OK
            io.add_status(STATUS_FEATURES_OK);
            if io.read8(REG_STATUS) & STATUS_FEATURES_OK == 0 {
                serial_println!("[VIRTIO] Features rejeitadas");
                return None;
            }

            // MAC
            let mac = [
                io.read8(REG_CONFIG),
                io.read8(REG_CONFIG + 1),
                io.read8(REG_CONFIG + 2),
                io.read8(REG_CONFIG + 3),
                io.read8(REG_CONFIG + 4),
                io.read8(REG_CONFIG + 5),
            ];

            // Set up 2 virtqueues (RX=0, TX=1)
            let rx_pa = setup_virtqueue(&io, 0, QUEUE_NUM)?;
            let tx_pa = setup_virtqueue(&io, 1, QUEUE_NUM)?;

            // Pre-allocate RX buffers
            prealloc_rx_buffers(rx_pa, io_base);

            // DRIVER_OK
            io.add_status(STATUS_DRIVER_OK);

            serial_println!("[VIRTIO] VirtIO-net OK. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

            Some(VirtIoDevice {
                io_base,
                rx_queue_pa: rx_pa,
                tx_queue_pa: tx_pa,
                rx_avail_idx: QUEUE_NUM,   // all initially in avail ring
                tx_avail_idx: 0,
                rx_used_last: 0,
                mac,
                present: true,
            })
        }
    }

    pub fn send(&mut self, data: &[u8]) -> bool {
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        unsafe {
            let d = (self.tx_queue_pa + offset) as *mut Desc;
            let avail = ((self.tx_queue_pa + 4096) + offset) as *mut AvailRing;

            let idx = self.tx_avail_idx;
            let desc_idx = idx % QUEUE_NUM;

            // Allocate buffer for this packet
            let buf_pa = match alloc_pages(1) {
                Some((pa, va)) => { core::ptr::write_bytes(va, 0, 4096); pa }
                None => return false,
            };
            let buf_va = buf_pa + offset;
            core::ptr::copy_nonoverlapping(data.as_ptr(), buf_va as *mut u8, data.len());

            // Write descriptor
            let d = &mut *d.add(desc_idx as usize);
            d.addr = buf_pa;
            d.len = data.len() as u32;
            d.flags = 0;     // TX = driver write, device read
            d.next = 0;

            // Add to available ring
            let avail_idx = (*avail).idx;
            (*avail).ring[avail_idx as usize % QUEUE_NUM as usize] = desc_idx;
            core::sync::atomic::fence(Ordering::SeqCst);
            (*avail).idx = avail_idx.wrapping_add(1);

            // Notify device
            Port::new(self.io_base + REG_QUEUE_NOTIFY).write(1u16);

            self.tx_avail_idx = idx.wrapping_add(1);

            // Poll briefly for completion
            let used = ((self.tx_queue_pa + 8192) + offset) as *mut UsedRing;
            for _ in 0..1000 {
                core::sync::atomic::fence(Ordering::SeqCst);
                let used_idx = (*used).idx;
                if used_idx.wrapping_sub(self.tx_avail_idx.wrapping_sub(1)) > 0 {
                    break;
                }
                core::hint::spin_loop();
            }
        }
        true
    }

    pub fn recv(&mut self) -> Option<Vec<u8>> {
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        unsafe {
            let used = ((self.rx_queue_pa + 8192) + offset) as *mut UsedRing;
            core::sync::atomic::fence(Ordering::SeqCst);
            let used_idx = (*used).idx;
            if used_idx == self.rx_used_last {
                return None;
            }

            let elem = &(*used).ring[self.rx_used_last as usize % QUEUE_NUM as usize];
            let desc_idx = elem.id as u16 % QUEUE_NUM;
            let len = elem.len as usize;

            let d = (self.rx_queue_pa + offset) as *mut Desc;
            let buf_pa = (*d.add(desc_idx as usize)).addr;
            let buf_va = buf_pa + offset;

            let mut data = Vec::with_capacity(len);
            core::ptr::copy_nonoverlapping(buf_va as *const u8, data.as_mut_ptr(), len);
            data.set_len(len);

            // Recycle: put buffer back on available ring
            let avail = ((self.rx_queue_pa + 4096) + offset) as *mut AvailRing;
            let avail_idx = (*avail).idx;
            (*avail).ring[avail_idx as usize % QUEUE_NUM as usize] = desc_idx;
            core::sync::atomic::fence(Ordering::SeqCst);
            (*avail).idx = avail_idx.wrapping_add(1);

            Port::new(self.io_base + REG_QUEUE_NOTIFY).write(0u16);

            self.rx_used_last = self.rx_used_last.wrapping_add(1);
            Some(data)
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

struct IoPorts(u16);
impl IoPorts {
    fn new(base: u16) -> Self { IoPorts(base) }
    fn read8(&self, reg: u16) -> u8 { unsafe { Port::new(self.0 + reg).read() } }
    fn write8(&self, reg: u16, v: u8) { unsafe { Port::new(self.0 + reg).write(v); } }
    fn read16(&self, reg: u16) -> u16 { unsafe { Port::new(self.0 + reg).read() } }
    fn write16(&self, reg: u16, v: u16) { unsafe { Port::new(self.0 + reg).write(v); } }
    fn read32(&self, reg: u16) -> u32 { unsafe { Port::new(self.0 + reg).read() } }
    fn write32(&self, reg: u16, v: u32) { unsafe { Port::new(self.0 + reg).write(v); } }
    fn add_status(&self, bits: u8) { self.write8(REG_STATUS, self.read8(REG_STATUS) | bits); }
}

/// Configura uma virtqueue. Retorna phys_addr da página do descritor.
unsafe fn setup_virtqueue(io: &IoPorts, queue_idx: u16, size: u16) -> Option<u64> {
    io.write16(REG_QUEUE_SEL, queue_idx);
    let max_size: u16 = io.read16(REG_QUEUE_SIZE);
    if max_size < size { return None; }

    // Allocate 3 pages: desc + avail + used (legacy layout)
    let (pa, va) = alloc_pages(3)?;
    core::ptr::write_bytes(va, 0, 3 * 4096);

    // Legacy: write PFN (page frame number = phys >> 12)
    io.write32(REG_QUEUE_ADDR, (pa >> 12) as u32);

    Some(pa)
}

/// Pre-allocate RX buffers into the available ring
unsafe fn prealloc_rx_buffers(queue_pa: u64, io_base: u16) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let desc = (queue_pa + offset) as *mut Desc;
    let avail = ((queue_pa + 4096) + offset) as *mut AvailRing;

    for i in 0..QUEUE_NUM {
        let (buf_pa, buf_va) = match alloc_pages(1) {
            Some(p) => p,
            None => return,
        };
        core::ptr::write_bytes(buf_va, 0, 4096);

        // Descriptor: device writes to this buffer
        let d = &mut *desc.add(i as usize);
        d.addr = buf_pa;
        d.len = BUF_SIZE as u32;
        d.flags = 2;   // WRITE
        d.next = 0;

        // Add to available ring
        (*avail).ring[i as usize] = i;
    }
    (*avail).idx = QUEUE_NUM;

    // Notify device about RX queue
    Port::new(io_base + REG_QUEUE_NOTIFY).write(0u16);
}

// ---------------------------------------------------------------------------
// Public init
// ---------------------------------------------------------------------------

pub unsafe fn init_driver_virtio() -> bool {
    let devices = crate::pci::scan_pci();
    for dev in &devices {
        if dev.vendor_id == VIRTIO_VENDOR &&
           (dev.device_id == VIRTIO_NET_TRANSITIONAL || dev.device_id == VIRTIO_NET_MODERN) {
            serial_println!("[VIRTIO] Detectado: {:02x}:{:02x}.{:02x}",
                dev.bus, dev.device, dev.function);
            if let Some(virtio) = VirtIoDevice::new(dev) {
                let mac = virtio.mac;
                crate::net::NET_CONFIG.lock().mac = mac;
                *crate::net::VIRTIO_DEV.lock() = Some(virtio);
                return true;
            }
        }
    }
    serial_println!("[VIRTIO] Nenhum dispositivo VirtIO-net encontrado.");
    false
}
