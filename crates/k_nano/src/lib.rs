#![no_std]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]
#![allow(unreachable_patterns)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

// ─── k_nano: Hardware Foundation ───
// Hardware base layer (Ring 0): CPU, memory, PCI, ATA, network drivers, filesystems
// Every other K²CHJ crate depends on this one.

pub mod acpi;
pub mod ahci;
pub mod allocator;
pub mod apic;
pub mod ata;
pub mod block_dev;
pub mod boot_logger;
pub mod cfs;
pub mod disk_agent;
pub mod disk_power;
pub mod dma;
pub mod e1000;
pub mod env;
pub mod exfat;
pub mod ext2_reader;
pub mod fat32;
pub mod fs;
pub mod globals;
pub mod fs_driver;
pub mod gpt;
pub mod hnsw;
pub mod hw_rng;
pub mod identity;
pub mod interrupts;
pub mod io_scheduler;
pub mod memory;
pub mod mhi;
pub mod multi_user;
pub mod nic_globals;

pub mod neural_fs;
pub mod ntfs_reader;
pub mod pci;
pub mod rtl8139;
pub mod serial;
pub mod simd;
pub mod slab;
pub mod slip;
pub mod smp;
pub mod storage_manager;
pub mod sync;
pub mod time_utils;
pub mod tpm;
pub mod tracer;
pub mod usb_msc;
pub mod vfs;
pub mod vga_buffer;
pub mod verify;
pub mod virtio_net;
pub mod xhci;

// Macros (serial_println!, println!, kjson!, klogc!, debug_rl!) are exported
// via #[macro_export] from their respective source files.
