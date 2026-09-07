//! StoragePort — BE k_hal (R1), discovery + BlockDevice registry.
//! FASE E migration: k_nano storage_bus API exposed here as facade.
//! Callers: k_nano (storage BusKind), k_ai (sgdb store, self_heal_disk).
//! STORAGE_BUS statics permanecem em k_nano (bin-only, not migrated).
//! Em slices futuros, pode ser reimplementado contra k_hal DeviceTree + HalOffer.

use k_nano::storage_bus::{self as sb, BusKind};
use k_nano::block_dev::BlockDevice;

/// Device count shortcut.
pub fn device_count() -> usize {
    sb::STORAGE_BUS.lock().device_count()
}

/// Publish all storage (invokes sgdb publish + VFS mount).
pub fn publish_all_storage() {
    sb::STORAGE_BUS.lock().publish_all_storage()
}

/// Register a device probe.
pub fn register_probe(kind: BusKind, name: &'static str, dev: &mut dyn BlockDevice) {
    sb::STORAGE_BUS.lock().register_probe(kind, name, dev)
}

/// Bus report string.
pub fn bus_report() -> alloc::string::String {
    sb::bus_report()
}
