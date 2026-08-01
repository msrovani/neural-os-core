# crates/k_nano/src/storage/ — Flash KV & Legacy NVMe

**Responsibility**: ADR-0063 persistence — `tickv` (TickvLite: append-log KV with CRC,
GC/compaction, crash/power-loss recovery; blobs for firmware/WIFI.CFG/BOOT.LOG) over
`flash` (`FlashController` on a `BlockDevice`/NVMe) plus a legacy NVMe driver stub
(production NVMe lives in `disk_agent::nvme`).

**Key symbols**: `storage::{put_blob, get_blob, with_tickv, tickv_smoke, is_ready,
TICKV, TickvLite}`, `flash::{init_flash, FlashController, ActiveFlash, FLASH}`,
`nvme::{Nvme, NvmeResult}` (stub).

**Integration**: `mod.rs` re-exports the KV API for the bin (FAT blob storage path —
firmware/model blobs); complements disk_agent (block FS) with a log-structured KV tier.
