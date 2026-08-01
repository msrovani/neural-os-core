# crates/neural-kernel/src/disk_agent/

Disk-intelligence facade (single `mod.rs`): `pub use k_nano::disk_agent::*` — the
canonical implementation lives in k_nano (emagrecer ADR-0042/0062); the bin only
re-exports and wires it.

## Key symbols

`DiskIntelligenceAgent` (predictive/health agent), `controller::{AtaCtrl, NvmeCtrl}`
adapters, `nvme::{NvmeDriver, NVME_DRIVER}` static.

## Integration

In `kernel_boot()`: `DiskIntelligenceAgent::new()` registered into the AgentRegistry,
controllers registered from `ATA_DRIVER`/`NVME_DRIVER`, and `k_nano::storage_bus::STORAGE_BUS`
probed in NVMe > AHCI > ATA > USB order.
