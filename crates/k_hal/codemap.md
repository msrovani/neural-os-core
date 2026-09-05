# crates/k_hal/ — k-hal: Ring 1 (R1) Hardware Abstraction Layer

## Responsibility

Ring 1 of the K³CHJ stack (ADR-0041 §9, "L1 sensório-motor"): the hardware abstraction
layer between R0 silicon (k_nano) and the R3 agents (hermes/jarbas). Owns device
discovery (PCI → `DeviceCap`/DeviceTree), the `HalOffer` capability/bind API, per-class
FE ports (compute/net/display/audio/video), **USB host BE** (`usb/` — hub route/TT → MSC),
MMIO backends for GPU (blit/ring/intel/nvidia/amd), audio (Intel HDA), WiFi
(ath10k/iwlwifi/generic), the VirtIO transport (MMIO + PCI QUEUE_NOTIFY), the DeviceRecipe
trust table (ADR-0056), UnlockDAG tokens, HW-GATE residual tracking, and NPU detection.

**Dependency rule (hard):** k_hal consumes R0 singletons exclusively through
`k_nano::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET}`, `k_nano::pci`, `k_nano::apic`
(`map_page_uc`/`map_region_uc_2mb`), `k_nano::ATA_DRIVER`+`fat32`/`exfat`,
`k_nano::xhci` **host primitives** (rings/TRB/MMIO — não política de enum), 
`k_nano::EVENT_BUS`, `k_nano::slog_hal!`/`slog_bin!` — never `crate::memory::`.

**USB:** política hub→MSC em `k_hal::usb` (registra hook em `init_h1`). R0
`bringup_boot_msc` só despacha o hook; fallback root-only é degradado.

## Design

- **DeviceCap/HalOffer capability model.** `device_cap::{DeviceClass, DeviceId, DeviceCap}`
  is a metadata-only identity model (no MMIO). `discovery::populate_from_pci()` scans PCI
  (class 0x03/0x02/0x01/0x04/0x0C) into a global DeviceTree and stamps each cap with
  BAR0 (`pci_bar::decode_bar`), VirtIO binding (VID 0x1AF4), and a `device_recipe::log_match`
  promote verdict. `offer::refresh_from_tree()` rebuilds per-class offer slots;
  `offer::bind(class, agent)` gates through `device_recipe::gate_bind_class` (trusted
  recipe + FAT FW presence), grants a logical `cap_gate::HalCap` (`grant_fe`), syncs the
  class port (`port_sync_bound`), and returns a `BindHandle { class, slot, topic }`.
  `release()` revokes the cap. All traffic flows over EventBus topics
  (`TOPIC_CAMERA_FRAME`, `TOPIC_COMPUTE_JOB`, `TOPIC_AUDIO_PCM`, `TOPIC_NET_RX`,
  `TOPIC_DISPLAY_FRAME`, …).
- **MMIO backend pattern (Cap-gated).** Every BAR touch is R1-only and checked with
  `cap_gate::check_map_bar(caller_ring, has_cap)` / `check_fe_bound(HalCap)` — ring ≤ 1
  always allowed, ring ≥ 3 denied without cap (enforceable via `set_cap_enforce`).
  BAR pages are mapped uncacheable through `k_nano::apic::map_page_uc` /
  `map_region_uc_2mb` (`backend::map_bars_uc`). Ports (`compute/net/display/audio/video_port`)
  are thin atomic status cells with Cap-gated FE entry points (`fe_tick`, `fe_present`,
  `fe_stream`, `fe_frame`, `submit_vector_add_stub`).
- **GPU backend detection + canary gating.** `gpu::detect::detect_all()` (class 0x03)
  yields `GpuInfo { vendor, arch, isa_tag, backend_kind, vram, … }` via DID tables +
  runtime probes (Intel `GMD_ID` @0xd8c, NVIDIA `PMC_BOOT_0` @BAR0+0, AMD IP discovery).
  `display_coex::plan_assignment` picks display vs compute owner (iGPU=display, dGPU=
  compute; NVIDIA > AMD > Arc). `backend::init_backend_with_plan` maps BARs UC, runs
  `firmware::secure_boot_gpu` (NVIDIA ACR / AMD PSP / Intel GuC), probes the vendor
  backend (`IntelRing`+`BcsRing` / `NvidiaGpu` / `AmdGpu`), then runs the `canary::`
  vector_add golden. `has_compute`/`CapToken::GpuCompute`/`BackendState::Ready` only
  after the canary PASS — never from a PFIFO NOP alone (ADR-0047 gate). CPU fallback is
  the honest default.
- **VirtIO = transport only.** `virtio.rs` classifies backends (`Native`/`VirtioPci`/
  `VirtioMmio`) from the DeviceTree (`select_backends_from_tree`) and implements real
  QUEUE_NOTIFY: MMIO magic check + write @0x050, or modern PCI `NOTIFY_CFG` capability
  (cfg_type=2) resolution → BAR + offset + queue×multiplier → UC map → `write_volatile`.
  Not the HalOffer API.
- **Trust model.** `device_recipe::GOLDEN_RECIPES` (static bind table:
  net.virtio, wifi.qca6174.ath10k, gpu.nvidia.gp108, usb.xhci.host, bt.template) with
  `RecipePromote::{Ok, Escalate, NeedsFw, None}`; `unlock_dag::CapToken` atomic bitset
  for cross-subsystem stages; `hw_gate::GATES` residual catalog
  (AWAITING_REAL_HW / CLOSED); `lego_boot::boot_selftest` cross-checks FAT `LEGO*.MD`
  specs against the recipe table.

## Flow

1. **Offer/consume** — `init_h1()` (bin calls `k_hal::init()`): `populate_from_pci` →
   `unlock_dag::boot_platform_tokens(pci_ok, fat_hint)` → `offer::refresh_from_tree`.
   FE agent (hermes/jarbas): `offer::request/query` → `bind` (recipe gate → `grant_fe` →
   `port_sync_bound` → `BindHandle`) → consume EventBus topic; `release` revokes.
2. **GPU init** — `detect_all()` → `plan_assignment()` → `init_backend_with_plan()`:
   `map_bars_uc` + `validate_bar0` → `GpuJobRing::new` → `secure_boot_gpu` (grant
   `GpuAcrBooted` on HsBooted) → vendor probe → `canary::run_vector_add_canary_{nv,intel,amd}`
   → `state_after` → Ready → `grant(GpuCompute)` → `compute_port::sync_from_backend()`.
   Runtime compute: `backend::gpu_matmul` (HW path gated by `adr0047_compute_gate` /
   `BackendState::Ready`, else `work_queue::drain` + CPU `tensor::matmul`).
3. **VirtIO transport** — `init_h4_log()`/`bring_up_h4()`: select backends, then per
   VirtIO device `try_pci_queue_notify(bus,dev,fn,q)` (or `try_queue_notify` MMIO);
   stage tracked as `VirtioStage::{Absent, LayoutReady, NotifySent, NotifySkipped}`.
4. **WiFi BE** — `net::wifi_ath10k::a3_on_bind` (CE/BMI → HTC/WMI → scan/assoc), Intel
   `wifi_iwlwifi` ucode path, and `generic_wifi::runtime_probe_and_bind` agnostic engine;
   `net::register_net_bound`/`set_link_up` sync the NetPort.

## Integration

- **neural-kernel (bin):** `k_hal::init()` (H1), `virtio::init_h4_log()`,
  `cap_gate::demo_h5_deny()`, `lego_boot::boot_selftest()`, `npu::init_npu()`,
  `hw_gate::{mark_boot_smoke, emit_all, emit_all_refresh}`, registers
  `audio::hda::HdaAudioAgent` in the agent fleet, gates firmware preload on
  `gpu::detect::GpuVendor`; `address_space.rs` uses `cap_gate::{hal_as_bar0, bind_hal_as,
  check_map_bar}` for the R1 AS.
- **hermes (FE):** `hal_offer.rs`/`link_watcher.rs` (offer query/bind, `DeviceClass`),
  `wifi_agent.rs` (`unlock_dag::CapToken`, `net::wifi_ath10k::{last_verdict, try_assoc}`,
  `net::wifi_msix::set_rx_inject`, `net::wifi_softmac::enable_if_associated`),
  `wifi_protocol.rs` (`net::wifi_crypto::inject_wpa2_key`); `lib.rs` re-exports
  `k_hal::net::{generic_wifi, wifi_compat, wifi_iwlwifi, wifi_msix, wifi_softmac}`.
- **jarbas (FE):** `vision_agent.rs`/`uvc_driver.rs` (offer camera topics), `virtio_gpu.rs`
  (`offer`, `virtio::VirtioStage`, `discovery::device_tree`), `audio/{mixer,voice}.rs`
  (`audio::hda::{write_hda_playback, poll_hda_audio}`), `display/fb.rs`
  (`gpu::backend::disable_intel_vga_plane`), `display/gauges.rs` (`gpu::vram::vram_usage`),
  `gpu/mod.rs` (`pub use k_hal::gpu::*`).
- **cortex:** `cortex::tensor::Tensor` consumed by `gpu::backend::gpu_matmul` / canary.

## Submodule Map

| Module | Responsibility |
|---|---|
| `device_cap.rs` | `DeviceClass`/`DeviceId`/`DeviceCap` — metadata-only device identity model |
| `discovery.rs` | DeviceTree — PCI scan → `DeviceCap` snapshot, `mark_bound` |
| `offer.rs` | HalOffer — per-class query/bind/release, EventBus topics, `BindHandle` |
| `cap_gate.rs` | HalCap (`MapBar`, `DeviceIo`, `Fe*`) ring-gated enforcement + R1 AS binding |
| `device_recipe.rs` | `GOLDEN_RECIPES` bind table + `RecipePromote` gate (trusted/FW) |
| `unlock_dag.rs` | `CapToken` bitset — cross-subsystem boot stages (honest Partial≠Ready) |
| `hw_gate.rs` | HW-GATE residual catalog (AWAITING_REAL_HW / CLOSED, rg-scannable) |
| `lego_boot.rs` | Boot selftest: locate FAT `LEGO*.MD` specs vs `GOLDEN_RECIPES` |
| `fat_assets.rs` | FAT32/exFAT root-file reads (recipe FW presence, LEGO specs) |
| `pci_bar.rs` | BAR decode (32/64-bit memory + I/O; canonical-address guard) |
| `compute_port.rs` | Compute FE status cell + Cap FeCompute-gated submit |
| `net_port.rs` | Net FE status cell (NotBound/Bound/Up/Denied) + Cap-gated `fe_tick` |
| `display_port.rs` | Display FE status cell + Cap FeDisplay-gated `fe_present` |
| `audio_port.rs` | Audio FE status cell + Cap FeAudio-gated `fe_stream` |
| `video_port.rs` | Video FE status cell + Cap FeVideo-gated `fe_frame` |
| `virtio.rs` | VirtIO transport — backend classification, MMIO 0x050 / PCI NOTIFY_CFG QUEUE_NOTIFY |
| `npu.rs` | NPU detection (AMD XDNA 0x1022:0x1502/17F0, Intel 0x8086:0x7D1D/643E) + software fallback |
| `gpu/` | GPU backends — detect, display_coex plan, backend init, canary, blit, ring, firmware, vendor stacks |
| `audio/` | Intel HDA MMIO driver — CORB/RIRB, SD0 capture + SD1 playback DMA |
| `net/` | WiFi BE MMIO drivers — ath10k (CE/BMI/HTC/WMI), iwlwifi ucode, generic register-map engine |
