//! VirtIO-Net — backend MMIO/transport (ADR-0041 H3).
//! Migrated from k_nano (FASE B). k_nano::virtio_net delegates here via facade.
//! Limits: vring MMIO + notification; policy in k_nano nic_globals.

pub use k_nano::virtio_net::*;
