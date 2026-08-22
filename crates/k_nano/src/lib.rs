#![cfg_attr(not(test), no_std)]
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
pub mod apic_heartbeat;
pub mod async_rt;
pub mod ata;
pub mod block_dev;
pub mod boot_chime;
pub mod boot_handoff;
pub mod boot_bind;
pub mod boot_logger;
pub mod boot_mode;
pub mod boot_report;
pub mod btrfs_reader;
pub mod firewall;
pub mod fts_search;
pub mod fw_cfg;
pub mod luks_open;
pub mod user_accounts;
pub mod boot_ramlog;
pub mod load_status;
// ponytail: cfs.rs moved to LEGACY/v1.5-dead-k2chj/k_nano/ (dead code)
pub mod disk_agent;
pub mod disk_power;
pub mod dma;
pub mod e1000;
pub mod i225;
pub mod env;
pub mod exfat;
pub mod exfat_write;
pub mod ext2_reader;
pub mod fat32;
pub mod fs;
pub mod globals;
pub use globals::ATA_DRIVER;
pub use globals::AHCI_DRIVER;
pub use globals::EVENT_BUS;
pub use globals::LATENT_BUS;
pub use globals::SKILL_REGISTRY;
pub use scancode_to_ascii::scancode_to_ascii;
pub mod fs_driver;
pub mod gpt;
pub mod hal;
pub mod hardware;
pub mod hw_profiler;
pub mod hw_change;
pub mod hnsw;
pub mod kernel_hnsw;
pub mod hw_rng;
pub mod identity;
pub mod interrupts;
pub mod io_scheduler;
pub mod ipc;
pub mod scheduler;
pub mod suspend_resume;
pub mod limine;
pub mod memory;
pub mod mhi;
pub mod mpmc;
pub mod multi_user;
pub mod net;
pub mod nic_globals;
pub mod storage;
pub mod storage_probe;
pub mod numa_alloc;
pub mod core_pinning;
pub mod cpufreq;
pub mod crypto;

pub mod neural_fs;
pub mod ntfs_reader;
pub mod pci;
pub mod pci_aer;
pub mod platform_probe;
pub mod proof_gate;
pub mod rtc;
pub mod rtl8139;
pub mod scancode_to_ascii;
pub mod serial;
pub mod slog;
pub mod simd;
pub mod slab;
pub mod slab_buddy;
pub mod slip;
pub mod sys_installer;
pub mod installer_agent;
pub mod self_check;
pub mod rollback;
pub mod smp;
pub mod tsc;
pub mod display;
pub mod storage_manager;
pub mod storage_bus;
pub mod sync;
pub mod time;
pub mod telemetry;
pub mod tpm;
pub mod tracer;
pub mod usb_msc;
pub mod usb_trust;
pub mod vfs;
pub mod vga_buffer;
pub mod verify;
pub mod virtio_net;
pub mod audio;
pub mod xhci;

// Macros (serial_println!, println!, kjson!, klogc!, slog_bin!) are exported via
// #[macro_export] from their respective source files.
// DEPRECATED: `debug_rl!` (rate-limited debug log) existia no monólito mas
// foi substituído por `slog_bin!`. Mantido apenas como histórico em main.rs
// com atributo #[deprecated]. Não portar para k_nano.
