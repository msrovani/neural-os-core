# crates/k_nano/ — Ring 0 Foundation (HAL Base)

## Responsibility

Lowest product crate of the K³CHJ stack: hardware foundation layer (Ring 0) for the
bare-metal x86_64 kernel. `#![no_std]` (optional `global-alloc` feature); every other
crate (`k_hal`, `k_ai`, `cortex`, `hermes`, `jarbas`) depends on it. Owns physical
memory management, the heap, IDT/GDT/TSS + exceptions + APIC, PCI config space, ACPI,
SMP bring-up (trampoline + per-AP TSS/IDT), serial, VGA/framebuffer, DMA/MMIO page
mapping, network drivers (RTL8139/E1000/I225/VirtIO), USB xHCI (HID + MSC), storage
(ATA/AHCI/NVMe/USB-MSC), filesystems (FAT32/exFAT/ext2/NTFS/Btrfs readers + native
CoW NeuralFS), VFS, the scheduler primitives (CFS, core pairs), sync primitives,
IPC (CellChannel), P2P mesh transport, MHI memory hierarchy, and boot logging/ramlog.
~141 `.rs` files. `neural-kernel` (bin) is the only consumer that wires everything;
logic lives here.

## Design Patterns

- **Singleton statics in `.data` (not `.bss`)** — hard-won lesson (SESSION_233/234):
  `GLOBAL_ALLOCATOR` (`TicketLock<Option<BitmapFrameAllocator>>`), `PHYS_MEM_OFFSET`,
  `TOTAL_RAM_MB`, and `nic_globals::{RTL8139, E1000, VIRTIO_DEV, NET_CONFIG}` live in
  `#[link_section = ".data"]` so the 512MB `.bss` `HEAP_BUFFER` bump-heap extension
  cannot clobber them. Global driver slots are `Mutex<Option<T>>` (spin).
- **lazy_static singletons**: `EVENT_BUS`/`LATENT_BUS`/`MESSAGE_BUS` (event-bus crate),
  `SKILL_REGISTRY` (`ticket_lock::TicketLock<SkillRegistry>` — canonical cross-crate
  registry; bin must NOT shadow it), `VFS` (`Mutex<Option<VfsRegistry>>`), GDT + TSS
  array, `XHCI_STATE`.
- **Lock-free / IRQ-safe primitives**: `smp::spsc` SPSC queue, `sync::mpmc`, atomic
  counters (`TIMER_TICKS`, `AP_ENTRY_COUNTER`, `CPU_COUNT`), `CURSOR_LOCK`
  (`AtomicBool` spin, IRQ side uses swap(true) and never blocks), `sync::irq_lock::IrqSafeLock`
  (TicketLock + temporary IRQ disable).
- **Agent pattern**: domain logic is wrapped in `agent_core::Agent` implementations with
  explicit `AgentManifest` — `disk_agent::DiskIntelligenceAgent`, `neural_fs::neural_fs_agent::NeuralFsAgent`,
  `installer_agent::AutoInstallerAgent`; `fs::FilesystemAgent` trait + `FS_AGENTS` registry.
- **MMIO via volatile raw pointers**: `xhci::r32/w32`, `e1000::read32/write32`;
  DMA pages and NIC MMIO are mapped uncacheable (`apic::map_page_uc`, PWT|PCD) —
  required for hypervisor DMA coherence (root cause of the E1000 RX=0 saga).
- **Driver registration**: fixed global slots (`globals::ATA_DRIVER/AHCI_DRIVER/USB_MSC`,
  `nic_globals` NIC slots + `set_nic_config(mac,ip)`), trait-object registries
  (`storage_bus::StorageBus::register_probe`, `disk_agent::{StorageController,
  FsProbeRegistry, VolMgrRegistry}`), and a registry of filesystem probes.
- **Trait abstractions**: `hal::Architecture` (x86_64 impl; port seam for aarch64/riscv64),
  `boot_handoff::BootHandoff` (unifies rust-bootloader 0.11 vs Limine), `BlockDevice`,
  `FrameAllocator` impl over the bitmap allocator.

## Data & Control Flow

**Boot (driven by `neural-kernel` bin, `kernel_main`/`agents::init_platform_sync`):**
1. `serial` probe, framebuffer probe (`vga_buffer::init`), `interrupts::init_idt`,
   `memory::init_memory(phys_offset)` (OffsetPageTable over CR3), frame allocator
   `init_from_usable_ranges` (from `BootHandoff::usable_regions`, Limine path) →
   `init_global_allocator`, then `allocator::init_heap` (talc/slab at
   `HEAP_START=0x_4000_0000_0000`, 512MB).
2. `init_platform_sync` (bin): `pci::init_pci()` → `acpi::init_acpi` (RSDP/FADT/MADT/SRAT)
   → `apic::init_apic` → `smp::init_smp` → `core_pinning::init_pools(total_cores)` →
   STI → `interrupts::calibrate_timer_hz`. `PlatformAgent` is idempotent if this ran.
3. Driver phase: ATA/AHCI/NVMe/xHCI probes + NIC init (`nic_globals` slots filled),
   `boot_logger::init`/`boot_ramlog`, VFS `init_standard_mounts`.
4. `disk_agent::DiskIntelligenceAgent` (Oneshot): probe controllers → S.M.A.R.T. →
   GPT (preferred) / MBR partition parse → `FsProbeRegistry` detect → volume mgr
   (LVM2/LUKS) → register MHI tiers → mount into VFS.
5. Runtime: scheduler (`scheduler::core_pair`, CFS), `async_rt::init_async_rt`
   (executor + waker queue fed from timer IRQ via `process_wakes`).

**NIC init order (e1000, `E1000Driver::init`)**: reset (`CTRL_RST`, poll clear) →
re-verify PCI Bus Master (re-enable via `pci::enable_pci_bus_master_unsafe`) → read MAC →
force link up (`CTRL_SLU|CTRL_FD`) → re-write RAL/RAH+AV → allocate+UC-map TX ring/bufs,
program `REG_TDBAL/TDBAH/TDLEN/TDH/TDT` (**0x3800/0x3818**, not the unwired QEMU
0x0420/0x0438 aliases) → RX ring/bufs, `REG_RDBAL/RDBAH/RDLEN/RDH` → `RDT=0` (empty ring)
→ `RXDCTL` QUEUE_ENABLE → clear MTA → TIPG/RDTR → **`RCTL` enable first, then `TCTL`** →
**`RDT=N-1` only after receiver enabled** → mask all IRQs (poll mode). `sfence` after
descriptor writes, `lfence` before reads. `send()`/`recv()` write descriptors via raw
volatile pointers (never packed-struct references) and bump TDT/RDT.

**SMP AP wake (ADR-0057 WS-A, `smp::init_smp` → `wake_aps_sequential`)**: alloc trampoline
frame <1MB (`allocate_below_1mb`, frame 0x40000), identity-map it; for each AP LAPIC ID
(from `acpi::BOOT_APIC_IDS`, fallback sequential guess): `trampoline::init_trampoline`
(patches blob with CR3, per-AP stack top from heap-top region, per-AP PerCpu ptr,
`ap_entry`) → directed `INIT-SIPI-SIPI` ×3 retry → wait `AP_ENTRY_COUNTER`. AP path:
real-mode trampoline → `ap_entry` (Rust) → PerCpu + LAPIC SVR → `init_ap_ist` →
`interrupts::init_ap_tss` → `ap_load_idt_and_tss` → `AP_IDT_READY` barrier → last AP
sets `ap_pollable(true)` → `ap_work::ap_idle_loop` (barrier jobs). `cortex::parallel_*`
is gated by `ap_pollable()` (false until full IDT/IPI path — safe BSP fallback).

**Net poll / P2P**: the smoltcp stack + DHCP live in hermes (bin `netstack`); k_nano
owns the NIC handles and raw L2 transport. Bin registers `hermes::net_bridge` hooks
(`register_http_get_url`, `register_udp_xfer` → bin `net::resolve_and_http_get_safe`).
`net::mesh::p2p_tick` (called from bin) reads `nic_globals::NET_CONFIG` (synced via
`set_nic_config` after IP config), builds signed `AiosTaskPacket` frames
(`net::noproto`), sends via `net::udp_broadcast::udp_broadcast_send` (VirtIO→E1000→RTL8139
fallback, port 42069, broadcast 255.255.255.255); non-heartbeat packets are published on
EventBus topic `TOPIC_P2P_PACKET` for hermes consumers (`skill_marketplace`, `skill_sync`).

## Integration Points

- **Consumers**: `k_hal` (R1; uses `k_nano::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET}`
  — never `crate::memory::` inside k_hal), `cortex` (`smp::ap_entry_count`, `ap_pollable`,
  `memory::global_hardware_context`), `hermes` (`k_nano::net::mesh`, `net::udp_broadcast`,
  `net::noproto`, `SKILL_REGISTRY`, `EVENT_BUS`, `vfs::VFS`/`vfs::fd`, `smp::AP_COUNT`),
  `k_ai`, `jarbas`, and `neural-kernel` (bin — sole integrator).
- **Bin re-exports** (`pub use k_nano::...`): `globals::{EVENT_BUS, LATENT_BUS, USB_MSC}`,
  `ATA_DRIVER`, `AHCI_DRIVER`, `SKILL_REGISTRY`, `load_status`, macros
  `kjson/klog/klogc/serial_print/serial_println` (+ `slog_*!`, `slog_bin!`).
- **Key public exports/statics**: `memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET, TOTAL_RAM_MB,
  init_memory, init_global_allocator, alloc_physical_frame, global_hardware_context}`,
  `allocator::{HEAP_START, HEAP_SIZE, init_heap}`,
  `nic_globals::{RTL8139, E1000, VIRTIO_DEV, NET_CONFIG, set_nic_config}`,
  `globals::{EVENT_BUS, LATENT_BUS, MESSAGE_BUS, SKILL_REGISTRY}`,
  `vfs::VFS`, `mhi::MHI_REGISTRY`, `xhci::XHCI_STATE`,
  `smp::{ap_entry, AP_COUNT, ap_entry_count, total_cores, ap_pollable, init_smp, wake_aps_sequential}`,
  `interrupts::{init_idt, TIMER_TICKS, TIMER_HZ, calibrate_timer_hz}`,
  `hal::ARCH`, `boot_handoff::BootHandoff`, `net::{mesh, udp_broadcast, noproto, transport}`.

## Submodule Map

| Submodule | Files | Responsibility |
|---|---|---|
| `smp/` | 7 | AP bring-up: trampoline, directed INIT-SIPI-SIPI wake, per-AP PerCpu/TSS/IDT, barrier work queue, core pools, SPSC + work-stealing |
| `disk_agent/` | 7 | `DiskIntelligenceAgent`: controller abstraction (ATA/AHCI/NVMe/USB), GPT/MBR + FS/volume probing, S.M.A.R.T., ARC cache, MHI tiering |
| `hardware/` | 5 | CPU topology/capability probe: xeon/epyc detection, cache topology, consolidated `HardwareReport` |
| `net/` | 5 | P2P/mesh transport: `noproto` packet codec, `udp_broadcast` raw-L2 frames, `transport` L2/UDP hybrid, `mesh` discovery/election/roles |
| `neural_fs/` | 12 | Native CoW filesystem: superblock, B-tree, inodes, extents, dir, CRC32C checksum tree, journal, volume API, agent |
| `xhci/` | 3 | xHCI USB3: controller init, hub addressing, HID keyboard/mouse, bulk endpoints + MSC |
| `storage/` | 3 | ADR-0063 persistence: TickvLite KV (append-log+GC), flash controller, legacy NVMe stub |
| `vfs/` | 3 | Virtual filesystem: mount table, path utils, fd table (`open/read/close`) |
| `scheduler/` | 2 | `cfs` vruntime scheduler + `core_pair` compute-dispatch pairs (ADR-0057 WS-B) |
| `sync/` | 4 | `IrqSafeLock`, Lamport `clock`, `mpmc` channel |
| `ipc/` | 1 | CellChannel abstraction: local SPSC + remote hybrid-transport channels |
| `fs/` | 2 | `FilesystemAgent` registry + VFS I/O bridge (`read_vfs/write_vfs/list_vfs`), ATA fs agent |

Root-level single files (not submodules): `memory`, `allocator`, `interrupts`, `pci`,
`pci_aer`, `acpi`, `apic`, `apic_heartbeat`, `serial`, `vga_buffer`, `simd`, `dma`,
`rtl8139`, `e1000`, `i225`, `virtio_net`, `virtio_gpu`, `ata`, `ahci`, `gpt`, `fat32`,
`exfat`, `exfat_write`, `ext2_reader`, `ntfs_reader`, `btrfs_reader`, `luks_open`,
`usb_msc`, `usb_trust`, `tpm`, `fw_cfg`, `hw_rng`, `async_rt`, `io_scheduler`,
`suspend_resume`, `cpufreq`, `core_pinning`, `numa_alloc`, `mhi`, `slab`, `slab_buddy`,
`hal`, `boot_handoff`, `boot_bind`, `storage_probe`, `boot_logger`, `boot_ramlog`, `boot_report`, `boot_chime`,
`load_status`, `identity`, `verify`, `proof_gate`, `rollback`, `firewall`, `env`,
`multi_user`, `user_accounts`, `time`, `rtc`, `telemetry`, `tracer`, `slog`, `slip`,
`scancode_to_ascii`, `p2p`, `sys_installer`, `installer_agent`, `self_check`,
`platform_probe`, `globals`, `nic_globals`, `kernel_hnsw`, `hnsw`, `fts_search`,
`fs_driver`, `hw_profiler`, `hw_change`.
