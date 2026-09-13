//! Bridge between k_nano's bare-metal environment and the `virtio-drivers` crate.
//!
//! Implements the two traits the crate requires:
//! - [`Hal`] — DMA allocation, MMIO mapping, memory sharing
//! - [`ConfigurationAccess`] — PCI configuration space read/write
//!
//! The crate uses **modern** VirtIO PCI transport (BAR-based common config),
//! not legacy I/O ports. Legacy devices still use the manual drivers in
//! `virtio_net.rs` / `virtio_blk.rs`.

extern crate alloc;
use alloc::vec::Vec;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use spin::Mutex;

use virtio_drivers::{Hal, BufferDirection, PhysAddr, PAGE_SIZE};
use virtio_drivers::transport::pci::bus::{
    ConfigurationAccess, DeviceFunction, DeviceFunctionInfo, HeaderType,
};

use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::apic::map_page_uc;

// ---------------------------------------------------------------------------
// AiosHal — DMA + MMIO for virtio-drivers
// ---------------------------------------------------------------------------

/// HAL implementation for k_nano bare-metal.
///
/// DMA allocation goes through the global bump allocator. MMIO uses
/// `map_page_uc` (uncached). Share/unshare are identity (no IOMMU).
pub struct AiosHal;

unsafe impl Hal for AiosHal {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let alloc = match (*guard).as_mut() {
            Some(a) => a,
            None => return (0, NonNull::dangling()),
        };
        let frame = match alloc.allocate_contiguous(pages) {
            Some(f) => f,
            None => return (0, NonNull::dangling()),
        };
        let pa = frame.start_address().as_u64();
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let va = (pa + offset) as *mut u8;
        // Zero the DMA region
        unsafe {
            core::ptr::write_bytes(va, 0, pages * PAGE_SIZE);
        }
        (pa, unsafe { NonNull::new_unchecked(va) })
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, _vaddr: NonNull<u8>, pages: usize) -> i32 {
        use x86_64::structures::paging::{FrameDeallocator, PhysFrame, Size4KiB};
        let mut guard = GLOBAL_ALLOCATOR.lock();
        if let Some(alloc) = (*guard).as_mut() {
            let start = PhysFrame::<Size4KiB>::containing_address(
                x86_64::PhysAddr::new(paddr),
            );
            for i in 0..pages {
                alloc.deallocate_frame(start + i as u64);
            }
            0
        } else {
            -1
        }
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        // Map the page as uncacheable for MMIO
        map_page_uc(paddr, offset);
        let vaddr = paddr + offset;
        NonNull::new_unchecked(vaddr as *mut u8)
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let vaddr = buffer.as_ptr() as *mut u8 as usize;
        // Identity mapping: virt = phys + offset → phys = virt - offset
        (vaddr as u64).wrapping_sub(offset)
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {
        // No-op: identity mapping means no copy-back needed
    }
}

// ---------------------------------------------------------------------------
// AiosPciAccess — bridges k_nano PCI I/O to virtio-drivers ConfigurationAccess
// ---------------------------------------------------------------------------

/// PCI configuration access bridging to k_nano's port I/O functions.
pub struct AiosPciAccess;

impl ConfigurationAccess for AiosPciAccess {
    fn read_word(&self, device_function: DeviceFunction, register_offset: u8) -> u32 {
        unsafe {
            crate::pci::read_config_dword(
                device_function.bus,
                device_function.device,
                device_function.function,
                register_offset,
            )
        }
    }

    fn write_word(
        &mut self,
        device_function: DeviceFunction,
        register_offset: u8,
        data: u32,
    ) {
        unsafe {
            crate::pci::write_config_dword(
                device_function.bus,
                device_function.device,
                device_function.function,
                register_offset,
                data,
            );
        }
    }

    unsafe fn unsafe_clone(&self) -> Self {
        AiosPciAccess
    }
}

// ---------------------------------------------------------------------------
// Helper: enumerate VirtIO PCI devices using the crate's PciRoot
// ---------------------------------------------------------------------------

/// Returns a list of (DeviceFunction, DeviceType) for all VirtIO PCI devices.
pub fn enumerate_virtio_devices() -> Vec<(DeviceFunction, virtio_drivers::transport::DeviceType)> {
    use virtio_drivers::transport::pci::bus::PciRoot;
    use virtio_drivers::transport::pci::virtio_device_type;

    let root = PciRoot::new(AiosPciAccess);
    let mut devices = Vec::new();

    // enumerate_bus yields (DeviceFunction, DeviceFunctionInfo)
    for bus in 0..=255u8 {
        for (dev_func, info) in root.enumerate_bus(bus) {
            if let Some(dev_type) = virtio_device_type(&info) {
                devices.push((dev_func, dev_type));
            }
        }
    }
    devices
}

/// Try to create a PciTransport for a VirtIO device at the given DeviceFunction.
/// Returns None if the device doesn't support modern VirtIO PCI transport.
pub fn try_create_transport(
    device_function: DeviceFunction,
) -> Option<virtio_drivers::transport::pci::PciTransport> {
    use virtio_drivers::transport::pci::bus::PciRoot;
    use virtio_drivers::transport::pci::PciTransport;

    let mut root = PciRoot::new(AiosPciAccess);
    match PciTransport::new::<AiosHal, _>(&mut root, device_function) {
        Ok(transport) => Some(transport),
        Err(e) => {
            crate::slog_nano!("VIRTIO", "warn",
                "Modern transport failed for {:02x}:{:02x}.{:02x}: {:?}",
                device_function.bus, device_function.device, device_function.function, e);
            None
        }
    }
}
