#![no_std]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]
#![allow(unreachable_patterns)]
#![feature(abi_x86_interrupt)]
#![cfg_attr(feature = "global-alloc", feature(alloc_error_handler))]

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
// ponytail: cfs.rs moved to LEGACY/v1.5-dead-k2chj/k_nano/ (dead code)
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
pub use globals::ATA_DRIVER;
pub use globals::EVENT_BUS;
pub use globals::LATENT_BUS;
pub use globals::SKILL_REGISTRY;
pub use scancode_to_ascii::scancode_to_ascii;
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
pub mod net;
pub mod nic_globals;

pub mod neural_fs;
pub mod ntfs_reader;
pub mod pci;
pub mod rtl8139;
pub mod scancode_to_ascii;
pub mod serial;
pub mod simd;
pub mod slab;
pub mod slip;
pub mod smp;
pub mod storage_manager;
pub mod sync;
// ponytail: time_utils.rs moved to LEGACY/v1.5-dead-k2chj/k_nano/ (dead code)
pub mod tpm;
pub mod tracer;
pub mod usb_msc;
pub mod vfs;
pub mod vga_buffer;
pub mod verify;
pub mod virtio_gpu;
pub mod virtio_net;
pub mod xhci;

// Macros (serial_println!, println!, kjson!, klogc!) are exported via
// #[macro_export] from their respective source files.
// Nota: `debug_rl!` (rate-limited debug log) existe apenas no monólito
// neural-kernel (main.rs) — não foi portado para k_nano ainda.
