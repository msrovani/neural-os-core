//! Storage — ADR-0063 Flash + TickvLite; stub nvme legacy retained.
//!
//! NVMe de produção: `disk_agent::nvme` (SESSION_171).

pub mod flash;
pub mod nvme;
pub mod tickv;

pub use flash::{init_flash, ActiveFlash, FileFlash, FlashController, FLASH};
pub use tickv::{
    backend_name, corrupt_smoke, crc32, dump_flash, encode_record, gc_smoke, get_blob,
    install_ram_flash, is_ready, power_loss_smoke, put_blob, record_size, remount_after_usb_msc,
    scan_volume, smoke as tickv_smoke, status_line as tickv_status, stress_gc_smoke, with_tickv,
    CKPT_KEY, HEADER, MAGIC, ScanResult, TickvLite, TickvStats, TICKV,
};

/// T-007 helper canônico — `k_nano::storage::measure_bandwidth(&mut dyn BlockDevice) -> u64` (B/s, TSC).
pub fn measure_bandwidth(dev: &mut dyn crate::block_dev::BlockDevice) -> u64 {
    crate::storage_bw::measure_bandwidth(dev)
}
