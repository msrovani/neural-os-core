# SESSION_171 — ADR-0062 P1 mark + P2 StorageBus + P3 NVMe + P24a HID

**Data:** 2026-07-23  
**Foco:** Plano P1/P2/P3/P24a; premissa **emagreçer neural-kernel**.

## Entregas

### D0 — P1 TLS
- ADR-0062 + INDEX: P1/#479 **✅ MVP** (SESSION_157/158; residual CertVerify/FAT).

### P3 — NVMe I/O + BlockDevice
- [`k_nano/disk_agent/nvme.rs`](../../crates/k_nano/src/disk_agent/nvme.rs): rewrite SQE NVMe 1.4, doorbell `2*qid`, **I/O qid=1**, bounce DMA, Identify CNS0/1.
- `impl BlockDevice for NvmeDriver`; global `NVME_DRIVER`; `NvmeCtrl` lê o global.
- Boot policy FAT: **NVMe → AHCI → ATA → USB-MSC**.

### P2 — StorageBus
- Novo [`k_nano/storage_bus.rs`](../../crates/k_nano/src/storage_bus.rs): `register_probe` + smoke MBR + exFAT detect → VFS `/mnt/data|/mnt/sata|/mnt/hdd|/mnt/usb`.
- `storage_manager` reporta o bus (não stub).

### Emagreçer
- Regra [`.cursor/rules/neural-emagrecer-bin.mdc`](../../.cursor/rules/neural-emagrecer-bin.mdc) + nota em AGENTS.md.
- `neural-kernel/disk_agent/` → **re-export** `k_nano::disk_agent` (apagados espelhos cache/controller/nvme/…).

### P24a — HID boot keyboard
- `bringup_hid_keyboard`: portas CCS ≠ `msc_port`; Address + SET_CONFIG + interrupt EP + SET_PROTOCOL/IDLE.
- `poll_keyboard` deixa de ser teatro — exige `hid_ready`.
- Wire pós-USB-MSC no boot.

## Gate

- `cargo check --release -p neural-kernel --features fat-boot-log` → **0 erros** (`target/check-p2p3`).
- Serial esperado: `StorageBus … MBR OK`, `NVMe … IO qid=1`, `P24a HID…` (QEMU `-device usb-kbd` / `-device nvme` para evidência HW).

## Residuals

- P24b hubs + mouse; NVMe multi-queue/PRP; POSIX VFS; CertVerify/FAT (TLS).
