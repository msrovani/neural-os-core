//! Storage — ADR-0063 Flash + TickvLite; legacy `nvme` init denied (SESSION_372).
//!
//! NVMe de produção: `disk_agent::nvme` (SESSION_171).

pub mod flash;
pub mod nvme;
pub mod tickv;

pub use flash::{init_flash, ActiveFlash, FileFlash, FlashController, FLASH};
pub use tickv::{
    backend_name, corrupt_smoke, crc32, dump_flash, encode_record, gc_is_suspended, gc_smoke,
    get_blob, install_ram_flash, is_degraded, is_ready, power_loss_smoke, put_blob,
    record_size, remount_after_usb_msc, scan_volume, set_gc_suspended, smoke as tickv_smoke,
    status_line as tickv_status, stress_gc_smoke, with_tickv, CKPT_KEY, HEADER, MAGIC, ScanResult,
    TickvLite, TickvStats, TICKV,
};

/// Batch put global (s410e) — UMA aquisição do lock TICKV para N writes.
/// Complementa `put_blob` (1 lock por put); usado pelo TickvStorageAdapter
/// (`put_many` do Storage trait) e por checkpoints L0/L1 do engine.
pub fn put_batch(items: &[(&str, &[u8])]) -> Result<(), &'static str> {
    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    if !kv.is_ready() {
        kv.mount()?;
    }
    kv.put_batch(items)
}

/// T-007 helper canônico — `k_nano::storage::measure_bandwidth(&mut dyn BlockDevice) -> u64` (B/s, TSC).
pub fn measure_bandwidth(dev: &mut dyn crate::block_dev::BlockDevice) -> u64 {
    crate::storage_bw::measure_bandwidth(dev)
}
