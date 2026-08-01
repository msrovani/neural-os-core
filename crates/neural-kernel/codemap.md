# crates/neural-kernel/

## Responsibility

Boot **binary** of neural-os-core (`no_std` + `no_main`, x86_64, higher-half via Limine).
Single entry chain: `limine_boot::_start` → `limine_entry` (collect `LimineHandoff`) →
`kernel_boot(handoff)` → 8-phase boot → `agent_core::AgentRegistry` scheduler loop.

- **Stays here**: everything that must exist exactly once in the final image — the
  boot sequence, the native agent fleet (`agents.rs`), bin-only residuals (net stack
  `net.rs`/`netstack.rs`, cortex model loading, TLS, GGUF, audio truth, ring3/IPC demos,
  cognitive singletons, storage drivers ATA/AHCI/FAT/exFAT), and all **wiring** between
  the K³CHJ crates.
- **Lives in crates (NOT here)**: R0 HAL (k_nano), R1 device caps/MMIO (k_hal), R2
  trust/self-heal (k_ai) + LLM/MoE/tensors (cortex), R3 orchestration/WASM/network
  (hermes), R3 display/persona (jarbas). This crate links them via `pub use` re-exports
  and bridge registrations; new logic must go in a crate, not here.

## Design Patterns

1. **8-phase event-driven boot** — `BootPhase` enum (SafeHarbor → MemoryCore →
   SystemBringup → Diagnostics → HardwareDiscovery → DriverInit → AgentFleet → Runtime)
   published as events on topic `BOOT_PHASE`; `BootLogAgent` + a static consumer drain
   them. Each phase also logs a framebuffer checkpoint (`display::fb::boot_ckpt`).
2. **Agent/Skill-first** — every entity is an `agent_core::Agent`; native agents live in
   `agents.rs` (`MonitorAgent`, `HwBridgeAgent`, `NetAgent`, `CortexAgent`, `HermesAgent`,
   `PlatformAgent`, `NetDriverAgent`, …); skills are registered in the **canonical**
   `k_nano::SKILL_REGISTRY` via `register_builtin_skills()` (P001: no private shadow copy).
3. **Wire/bridge pattern** — crates are re-exported with `pub use` (hermes modules,
   `k_ai::{self_heal, trust}`, `jarbas_crate::{display, gpu, jarvis, …}`) and glued via
   explicit bridges: `hermes_crate::net_bridge` ← bin `net::{tcp_exchange, dns, http}`,
   `hermes_crate::globals::install_vfs_bridge` ← `fs::{read_vfs, write_vfs, list_vfs}`,
   `hermes_crate::theme_bridge`, `k_nano::smp::install_wake_fn(apic::send_ipi_reschedule)`,
   `isolation_ring::init_connectors()`.
4. **Idempotent driver init** — `init_platform_sync()` (PCI+ACPI+APIC+SMP) guarded by
   `PLATFORM_READY` atomic; `PlatformAgent`/`NetDriverAgent` become no-ops if sync already
   ran. NIC ladder e1000 → i225 → rtl8139 → serial tunnel (SLIP), each `publish_boot_phase`
   only if the previous failed.
5. **Canonical statics in k_nano** — `EVENT_BUS`, `LATENT_BUS`, `SKILL_REGISTRY`,
   `ATA_DRIVER`, `AHCI_DRIVER`, `USB_MSC`, `nic_globals::{RTL8139, E1000, VIRTIO_DEV}`,
   `load_status`, `storage_bus::STORAGE_BUS`, `boot_ramlog`, `boot_report`. Bin-local
   statics: `I225`, `AUDIT_TRAIL`, `QEMU_LOADER_SCAN_START`.
6. **Honest gates, non-fatal demos** — ADR-0042 N1–N5 gates (`n3_cortex_gate`,
   `n4_hermes_gate`, `n5_jarbas_gate`), `adr0047_mvp_gates()`, and ADR-0041 P0–P9 demos
   (MVP-C `ipc::demo_two_spaces`, CapGate, Jarbas FB, K-IA DMA, cortex mmap, Ring3
   `user_mode::demo_ring3`, demand-paging, vring, gguf mmap) log PASS/PARTIAL/ABSENT and
   never halt the boot.
7. **Scheduler on heap stack** — `raw_sched_run` switches RSP to a 2 MB heap-allocated
   stack before `AgentRegistry::init_phase()`/`run()` to dodge the bootloader stack-boundary
   #PF (SESSION_233 lineage).

## Data & Control Flow

1. **Entry**: Limine `_start` → `limine_entry()`: early serial proof-of-life, collect
   `LimineHandoff` (HHDM offset, memmap, RSDP), `limine::apply_hhdm`, probe framebuffer
   into `display::fb::GPU`, then `kernel_boot(&handoff)`.
2. **`kernel_boot`** (all phases):
   - Store `PHYS_MEM_OFFSET` **early** (DMA drivers need PA→VA), RSDP, serial+VGA/framebuffer,
     `interrupts::init_idt()`, `BitmapFrameAllocator::init_from_usable_ranges`,
     `memory::init_memory` (page tables), `allocator::init_heap` (TALC) + bump resize,
     `arena::init_arena_region` → `global_arena`. Publish **SafeHarbor / MemoryCore**.
   - Labor boot-smokes (k_hal hw_gate, ipc_bus, async_io, wifi softmac/WPA2, tls, ntp, …),
     `k_nano::platform_probe::detect()` + `simd::enable_simd()`, `tpm::init_tpm`,
     `init_global_allocator`, `bei_init::init_bei`. Publish **SystemBringup / Diagnostics**.
   - Env detect (`net::detect_dev_env`), `init_platform_sync()` → **HardwareDiscovery**.
   - **DriverInit**: NIC ladder + `network_agent::bootstrap_early()` (smoltcp static/DNS/
     HTTP/TLS/NTP smokes + net_bridge registration), ATA/AHCI/NVMe/USB-MSC probes,
     xHCI HID keyboard/mouse, `boot_logger` (BOOT.LOG), VFS mounts + `fs::init_fs_agents`,
     DiskAgent + `storage_bus`, apps/audio/WASM (`wasmi_rt`, `app_factory`, `exec_arena`),
     model loading ladder **QEMU-loader (magic 0xBE11BE11 scan) → NVMe → AHCI → ATA FAT →
     USB-MSC** for BGE embedding, BitNet LLM (loader @4 GB, copy+`Box::leak` backing), Trinity
     experts (HW expert v3/v4, RustCoder), ModelHub slots, BPE vocab, STT CTC.
   - Greeting: LLM → TTS → FB paint when model+BPE loaded; `n3/n4/n5` gates; cognitive
     status dumps. Publish **AgentFleet** (registry full), **Runtime**.
3. **Runtime**: `raw_sched_run` → `registry.init_phase()` (round-robin Oneshot with
   timeout) → `registry.run()` loop: per tick `k_nano::cpufreq::ondemand_tick`, `hlt`,
   `bei_tick` hook, `RESPAWN_QUEUE` drain (crash respawn via `sched_metrics_hook`-adjacent
   respawn closure mapping names → agent constructors).
4. **State transitions**: `PLATFORM_READY` (sync once), `SYSTEM_READY` event (MonitorAgent
   publishes; SystemAgent consumes → Echo/Diagnostic skills), `TIMER_TICKS` (PIT/LAPIC),
   `load_status` (`AssetKind::{Llm, Bge, Stt}`), boot_ramlog → BOOT.LOG persist.

## Integration Points

| Crate | Wired via |
|---|---|
| **k_nano** (R0) | `EVENT_BUS`, `LATENT_BUS`, `SKILL_REGISTRY`, `ATA_DRIVER`, `AHCI_DRIVER`, `USB_MSC`, `nic_globals`, `load_status`, `storage_bus`, `disk_agent::nvme::NVME_DRIVER`, `usb_msc`, `boot_handoff::BootHandoff`, `limine`, `boot_report`, `boot_ramlog`, `async_rt`, `smp` (`ap_entry_count`, `install_wake_fn`, `ap_pollable`), `core_pinning`, `platform_probe`, `cpufreq`, `storage::tickv_smoke` |
| **k_hal** (R1) | `k_hal::init()`, `hw_gate`, `virtio`, `cap_gate`, `lego_boot`, `npu`, `audio::hda::HdaAudioAgent` |
| **k_ai** (R2) | `self_heal`, `trust`, `sgdb` (+ `boot_init`/`demo`/Hamming), `hw_capability` |
| **cortex** (R2) | bin `cortex.rs` wrapper (`load_model`/`set_model`/`generate_via_model`, hwexpert/rustcoder slots), `cortex_crate::parallel_matmul`, `cortex_crate::decode::self_test`, HW Expert v4 (`load_hwexpert_v5`) |
| **hermes** (R3) | broad `pub use` (`cron`, `security`, `safety`, `optimizer`, `wasmi_rt`/`wasm_build`/`app_factory`, `net_bridge`, `package_hub`, `skill_loader`, `evolve`, `self_update`, `wifi_*`, `fs`, `vfs`, `neural_fs`, …); bridges: `net_bridge::register_*` ← bin `net`, `install_vfs_bridge` ← bin `fs` |
| **jarbas** (R3) | `display` (fb, agent, metrics, ui_spec), `gpu`, `jarvis`, `uvc_driver`, `virtio_gpu`, `vision_agent`; audio truth = `mod audio` re-exporting `jarbas_crate::audio::*` (E4) |

**EventBus topics**: `BOOT_PHASE` (bin), `SYSTEM_READY` / `RAW_HW_IRQ1` (agents.rs),
`MOUSE_MOVED/CLICK/DRAG/SCROLL` (agents::mouse_agent), `HW_DEVICE_<id>` (HwSpecialistAgent),
`HW_NET_RTL8139`/`NETWORK_CONFIGURED`/`NETWORK_DEGRADED`/`NETWORK_HEALTH` (net.rs),
`SYSTEM_SHUTDOWN/REBOOT/HIBERNATE` (shutdown.rs), `USER_INTENT`/`HERMES_RESPONSE` (hermes),
`TOPIC_LLM_REQUEST` (cortex.rs), `P2P_PACKET` (k_nano, ADR-0081 mesh),
`TOPIC_UI_SPEC` (display::ui_spec), audio `TOPIC_AUDIO_IN/OUT`, `TOPIC_WAKEWORD`,
`TOPIC_STT_TEXT`, `TOPIC_TTS_CMD` (audio facade).

## Submodule Map

| Module | Compiled? | Responsibility |
|---|---|---|
| `agents.rs` (+ `agents/`) | ✅ `pub mod agents` | Native agent fleet (~60 agents: system/router/driver/console/network + `init_platform_sync`, `register_agency_agents`, `register_hw_agents`); `agents/` adds MouseAgent, SysInfoAgent, LogAnalystAgent |
| `audio/` | ✅ `mod audio` | Voice/audio facade: `pub use jarbas_crate::audio::*` (TTS/STT skills, Jarvis/WakeWord/AudioPipeline/Mixer agents, `TOPIC_*`) |
| `disk_agent/` | ✅ `mod disk_agent` | Disk intelligence facade: `pub use k_nano::disk_agent::*` (DiskIntelligenceAgent, NVMe, controller adapters) |
| `smp/` | ✅ `mod smp` | SMP bring-up: `init_smp()`, `AP_COUNT`, PerCpu, SPSC queue, cortex parallel-matmul re-export, k_nano trampoline/work-stealing re-exports |
| `task/` | ✅ `mod task` | Minimal async helpers: `dummy_waker()`, `yield_now()` |
| `ipc/` | ✅ `mod ipc` | Cross-address-space IPC PoC (ADR-0041 MVP-C): `SharedSpscRing`, `demo_two_spaces()` |
| `fs/` | ❌ legacy mirror | `FilesystemAgent` trait + `FS_AGENTS` + `read_vfs/write_vfs/list_vfs` + `RingBufStore` + 8 fs agents + MHI scheduler — **not declared in main.rs**; compiled `crate::fs` = `hermes_crate::fs` re-export (E1b). Keep in sync with `crates/hermes/src/fs/` |
| `vfs/` | ❌ legacy mirror | VFS registry (mount table, `VfsNode`, `VectorFs`, `VFS` static) — compiled `crate::vfs` = `hermes_crate::vfs` (E1b) |
| `neural_fs/` | ❌ legacy mirror | NeuralFS CoW facade (k_nano re-exports) + `NeuralFsAgent` + tests — compiled `crate::neural_fs` = `hermes_crate::neural_fs` (E1b) |
