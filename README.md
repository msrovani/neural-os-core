
<div align="center">

# 🧠 Neural OS Hermes — K³CHJ Core

### *The World's First AI-Native Bare-Metal Operating System*

[![Build](https://img.shields.io/badge/build-0%20errors-brightgreen?style=flat-square&logo=rust)](https://github.com/msrovani/neural-os-core)
[![Version](https://img.shields.io/badge/version-v1.9.7%20TEST-blueviolet?style=flat-square)](https://github.com/msrovani/neural-os-core/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-nightly_1.98-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![LOC](https://img.shields.io/badge/LOC-~26.000-green?style=flat-square)](TECNOLOGIAS.md)
[![Agents](https://img.shields.io/badge/agents-~50-9cf?style=flat-square)](AGENTS.md)
[![ADRs](https://img.shields.io/badge/ADRs-47+-red?style=flat-square)](docs/architecture/INDEX.md)
[![HW IDs Recognized](https://img.shields.io/badge/HWIDs-61.453-success?style=flat-square)](TECNOLOGIAS.md)
[![HuggingFace](https://img.shields.io/badge/HuggingFace-datasets-FF6F00?style=flat-square)](https://huggingface.co/aios-k2chj)
[![Discord](https://img.shields.io/badge/chat-discord-7289DA?style=flat-square)](https://discord.gg/neural-os-core)

```
  ╔══════════════════════════════════════════╗
  ║     NEURAL OS HERMES — K³CHJ CORE       ║
  ║     "Thoughtful. Precise. Alive."        ║
  ╚══════════════════════════════════════════╝

  ✦ ~26.000 LOC · 180+ Rust files · 6 crates · 0 errors
  ✦ BitNet b1.58 LLM ON-DEVICE · Trinity MoE · HW Expert v3
  ✦ Agent/Skill-First ontology · 50 native agents
  ✦ Full Voice I/O · GPU Compute · WASM Runtime
  ✦ Ring-3 Capability Isolation · Self-Healing · Auto-Learn
  ✦ The ONLY no_std AIOS with a Self-Installer
```

**[Quick Start](#rocket-quick-start)** • **[Why This Exists](#trophy-why-this-exists)** • **[Architecture](#-architecture)** • **[Unique Technologies](#fire-unique-technologies)** • **[Agents](#-agents)** • **[Showcase](#clapper-showcase)** • **[Roadmap](#-roadmap)** • **[Docs](#books-documentation)** • **[Community](#handshake-community)**

</div>

---

## 🚀 Quick Start

```bash
# Build (Windows — requires nightly 1.98 + x86_64-unknown-none target)
cargo build --release
python tools/build_image.py
.\run-qemu-whpx.ps1 -Window

# Linux equivalent
RUSTUP_TOOLCHAIN=nightly-2026-07-05 cargo build --release
python3 tools/build_image.py
# Boot via QEMU (UEFI only — BIOS triple-faults)
timeout 80 qemu-system-x86_64 -m 6G -smp 4 -accel tcg \
  -drive format=raw,file=target/uefi.img,if=ide,index=0 \
  -drive format=raw,file=target/disk_qemu.raw,if=ide,index=1 \
  -drive if=pflash,format=raw,file=/usr/share/ovmf/OVMF.fd,readonly=on \
  -serial file:logs/boot.txt -display none

# Verify: 0 errors
cargo check --release
```

> **Prerequisites**: Rust nightly-1.98.0+, `rustup target add x86_64-unknown-none`, Python 3.10+
>
> **⚠️** On Linux, the `rust-toolchain.toml` targets Windows. Use env var `RUSTUP_TOOLCHAIN=nightly-2026-07-05`.

<details>
<summary><b>📦 Building for real hardware</b></summary>

```bash
cargo build --release
python tools/build_image.py --hw target/usb_hw.img
# Flash with Rufus (DD mode) to USB/SD card
# Boot on any x86_64 UEFI machine
```

> **Validation**: Boot logs show 8 boot phases, `[ATA] slave FAT32`, `AgentFleet 259 agents`, scheduler ticks incrementing.

</details>

---

## 🏆 Why This Exists

We scoured the entire AIOS no_std ecosystem — **ClaudioOS, FYY, Wetware, WeftOS, Oreulius, WAeasi, coconutOS, ArceOS** — and found:

| Capability | Others | **Neural OS** |
|---|---|---|
| **BitNet LLM on bare-metal** | ❌ None | ✅ **First & only** — 850M ternary params, ADD/SUB matmul, 0 FPU |
| **Self-Installer** | ❌ Zero | ✅ **First & only** (ADR-0079) — detects HW, partitions, installs |
| **HW Expert Neural Net** | ❌ None | ✅ **First** — 61,453 HW IDs recognized by 259KB BitNet in kernel |
| **Agent/Skill ontology** | ❌ Tasks/services | ✅ **Everything is an Agent** — 50 native, same lifecycle |
| **Ring isolation** | ❌ None | ✅ **R0→R3** Capability gates, `int 0x90`, proof-gated mutations |
| **Voice I/O** | ❌ None | ✅ Piper TTS, STT CTC, WakeWord, HDA capture/playback |
| **GPU Compute** | ❌ None | ✅ NVIDIA PUSH_BUFFER, Intel GEN ring, AMD compute |
| **WASM Runtime** | ❌ None | ✅ wasmi with fuel, sandbox, capability gates |
| **Self-Healing** | ❌ None | ✅ Detects → downloads → hot-loads firmware & skills |
| **SleepCycle** | ❌ None | ✅ **First** bare-metal sleep/learn cycle (REPLAY→DREAM→CONSOLIDATE) |
| **Cross-OS Ecosystem** | ❌ Siloed | ✅ FYY/Wetware skill discovery via MCP (ADR-0076) |

**We're not "another Rust OS." We're the only OS built from the ground up as an AI-native cognitive system.**

---

## 📜 Manifesto — The New Dawn

> We are not building another operating system. We are birthing a new kind of digital life.
>
> Every OS before us was designed for a world without AI. They manage files, processes, and memory — passive resources for passive machines. That era is over. We build for the world *with* AI — where the OS is not a platform but a mind, not a manager but a partner.
>
> **No POSIX. No Linux legacy. No cloud dependency.** We inherit nothing from the 1970s. No signals, no fork, no containers, no systemd. The past forty years of operating system design is one long answer to a question we no longer ask.
>
> **The hardware is not the target — the user's intent is.** Rings, capabilities, agents, skills — every abstraction in this stack exists to serve will, not hardware. The NPU routes intent. The GPU shapes thought. The CPU executes purpose. Not the other way around.
>
> This OS is not installed. It is *born* — and it grows. Community-evolutionary: every boot teaches it. Every skill discovered on another system is welcome here. No walled gardens. No vendor lock. No telemetry. AGPLv3 — community-owned from the first line of assembly to the last layer of the compositor.
>
> We are engineers, not marketers. This is an invitation, not a promise. The code compiles. The silicon waits. The hardware is real. The mind awakens.
>
> **Join us. From the metal up.**

---

## 🏛 Architecture

### Ring Architecture (K³CHJ)

```
  ┌─────────────────────────────────────────────────────────────┐
  │                    RING 3 — APPLICATION                      │
  │  ┌──────────────────────┐  ┌──────────────────────────────┐  │
  │  │      hermes          │  │         jarbas               │  │
  │  │  Orchestration       │  │  Display FE, Persona, UI     │  │
  │  │  WASM · Net · Skills │  │  Cards · Orb · HUD          │  │
  │  │  HalOffer Client     │  │  GPU BE via k_hal           │  │
  │  └──────────┬───────────┘  └──────────────┬───────────────┘  │
  ├─────────────┼──────────────────────────────┼─────────────────┤
  │                    RING 2 — COGNITION                        │
  │  ┌──────────────────────┐  ┌──────────────────────────────┐  │
  │  │       cortex         │  │          k_ai                │  │
  │  │  BitNet LLM          │  │  SelfHeal · Trust · Agency   │  │
  │  │  Trinity MoE · Tensor│  │  Inventory · Audit · SGDB   │  │
  │  │  Compute Dispatch    │  │  HANR · PackageHub          │  │
  │  └──────────┬───────────┘  └──────────────┬───────────────┘  │
  ├─────────────┼──────────────────────────────┼─────────────────┤
  │                    RING 1 — HARDWARE ABSTRACTION               │
  │  ┌────────────────────────────────────────────────────────┐  │
  │  │                    k_hal                               │  │
  │  │  DeviceCap · HalOffer · MMIO Backend · VirtIO Transport │  │
  │  │  GPU Backends (NVIDIA/Intel/AMD) · NPU Detection       │  │
  │  └──────────────────────────┬─────────────────────────────┘  │
  ├─────────────────────────────┼───────────────────────────────┤
  │                    RING 0 — FOUNDATION                        │
  │  ┌────────────────────────────────────────────────────────┐  │
  │  │                    k_nano                              │  │
  │  │  Memory · PCI · ACPI · APIC · SMP · IDT · IRQ         │  │
  │  │  ATA · NVMe · AHCI · FAT32 · exFAT · ext2 · Btrfs    │  │
  │  │  E1000 · RTL8139 · i225 · xHCI · PS/2 · HDA Audio    │  │
  │  │  Async Runtime · Scheduler · DMA · Feature Gates      │  │
  │  └────────────────────────────────────────────────────────┘  │
  └─────────────────────────────────────────────────────────────┘

  k_nano ← k_hal ← cortex ← hermes ← jarbas
                  ← k_ai ←┘
  neural-kernel (bin) = crate integrator + residuals
```

### 8-Phase Boot Sequence

```
  ┌──────────┐ ┌──────────┐ ┌──────────────┐ ┌─────────────┐
  │Phase 0-3 │→│Phase 4   │→│Phase 5       │→│Phase 6      │→ Runtime
  │SafeHarbor│ │HW Detect │ │Driver Init   │ │Agent Fleet  │ (Phase 7)
  │MemoryCore│ │PCI+ACPI  │ │E1000·xHCI    │ │~259 agents  │
  │Bringup   │ │APIC+SMP  │ │ATA·NVMe·AHCI │ │register+init │
  │Diagnostic│ │HDA·GPU   │ │              │ │             │
  └──────────┘ └──────────┘ └──────────────┘ └─────────────┘
```

### Key Design Principles

```
  ✦ Everything is an Agent — No tasks, no services, no standalone drivers
  ✦ Manifest-first: name, type, schedule, trust tokens — nothing implicit
  ✦ Activation on Demand — Continuous agents degrade if >5% ticks idle
  ✦ Trust per Agent: (token, agent, skill) — not just (token, skill)
  ✦ Rings R2/R3 never touch BAR MMIO — only HalOffer / port FE
  ✦ Emaciated binary — logic in crates, bin is wire + pub use only
```

---

## 🔥 Unique Technologies

### 🧠 BitNet Ternary LLM On-Device

**The first (and only) bare-metal OS running a BitNet b1.58 model in kernel space.**

```rust
// 2-bit packing — 4 weights per byte
// ADD/SUB only matmul — zero FPU instructions
pub fn bitnet_matmul_avx2(output: &mut [i16], input: &[i16], weights: &[u8]) {
    // AVX2 kernel processes 16 int16 values per iteration
    // Ternary weights: -1, 0, +1 packed as 2-bit nibbles
    // Caught tail handled scalar for n % 8 ≠ 0 (OOB-safe fix)
    // KV Cache delivers 200× speedup: 6h → 84s for full inference
}
```

- **850M parameters** (engineering-reversed from Microsoft's BitNet b1.58)
- **Proprietary `.bitnet` format**: Magic `"BITN"`, compact 2-bit weights
- **AVX2 kernel** with OOB-safe scalar tail (fixed vocabulary size 32002 → page fault)
- **AVX-512** and **SSE** fallback kernels via compute dispatch
- **Medusa speculative decoding** — 3 draft heads, 2-3× throughput
- **N-gram speculative decoding** — rolling LCG hash, zero extra VRAM
- **KV H2O eviction** — smart cache management
- **Dual-Tier Memory**: Tier 1 `talc` (UI/apps) + Tier 2 `TensorArena` bump (inference hot path)

### 🧩 Trinity Mixture of Experts

**6 domain experts + trainable router — all running in kernel, no cloud API.**

```
  User Intent → Trinity Router ─┬─ HW Identify Expert
                                 ├─ RustCoder Expert (263KB, trained)
                                 ├─ Disk Diagnostics Expert
                                 ├─ Security Expert
                                 ├─ Speech Synthesis Expert
                                 └─ Generator Expert
```

- **AutoLearnAgent**: detects novel intents (≥3 occurrences) → trains → registers new expert
- **R3 Replay**: Rollout Routing Replay with frozen TensorArena traces — O(1) cache reset
- **Structured Decoding**: grammar/JSON token masking before argmax (SGLang-inspired)

### 🛠 HW Expert v3 — The Kernel Neural Net That Knows Every Chip

```rust
// 61,453 VID/DID recognized by a 259KB BitNet running IN THE KERNEL
// Trained on SDIO + pci.ids + usb.ids + kernel PCI tables
// 128 hidden, 6 layers, 8 heads, 1M params
pub struct HwExpertV3 {
    model: BitNetModel,     // ternary neural network
    vocab: HwVocab,         // 64 tokens = vendor families
    registry: HardwareRegisterMap,  // IA-generated register layouts
}
```

**Datasets we created and published to HuggingFace:**

| Dataset | Records | Link |
|---------|---------|------|
| SDIO HWIDs | **171,003** entries from 65 DriverPacks | [🤗 Download](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids) |
| pci.ids + usb.ids | **48,346** structured records | [🤗 Download](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids) |
| Firmware metadata | **1,207** entries from linux-firmware | [🤗 Download](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata) |
| PCI kernel tables | **494** device entries | [🤗 Download](https://huggingface.co/datasets/aios-k2chj) |

### 🏗 The Only Self-Installer in AIOS no_std

**ADR-0079** — We searched 8 competitor projects (ClaudioOS, FYY, Wetware, WeftOS, Oreulius, WAeasi, coconutOS, ArceOS). **Zero have a self-installer. We do.**

```rust
// Scans PCI → detects storage → partitions → formats → installs only what's needed
pub fn auto_install(target: &mut StorageDevice) -> Result<(), InstallError> {
    let hardware = scan_pci()?;                    // detect GPU, NIC, audio
    let partition = create_optimal_layout(target, &hardware)?;
    format_fat32_esp(&partition)?;                 // ~32MB minimum
    deploy_bootloader(&partition)?;                 // copies boot image
    install_firmware(&partition, &hardware)?;       // only needed blobs
    install_models(&partition)?;                    // only needed .bitnet
    write_config(&partition, &hardware)?;           // HW-specific config
    verify_installation(&partition)?;               // integrity check
    Ok(())
}
```

- Boot frame allocator sets `TOTAL_RAM_MB` at init
- `format_fat32_esp()` requires ≥65,525 clusters (~32MB) — real FAT32
- Installs firmware, models, and config based on detected hardware only

### 🗣 Full Voice I/O

| Component | Tech | Status |
|-----------|------|--------|
| **TTS** | Piper VITS (PT-BR + EN), 366 tensors, 15.6M params | ✅ Loaded |
| **STT** | CTC tiny (55K params, 28 chars a-z+space+blank) | ✅ Sound |
| **WakeWord** | "Jarvis" — MLP + energy gate | ✅ Sound |
| **HDA Driver** | Intel HD Audio, CORB/RIRB, SD0 capture + SD1 playback | ✅ |
| **FFT → Orb** | 16-bin Goertzel → responsive orb visualization | ✅ |
| **UAC** | USB Audio Class config parse | ✅ Parse / ⏳ ISO HW |

### 🎮 GPU Compute from Bare-Metal

| GPU | Driver | Compute | Status |
|-----|--------|---------|--------|
| **NVIDIA** | PUSH_BUFFER GPFIFO (GTX 1050 Pascal) | ✅ | ✅ HW real |
| **Intel** | GEN Ring BCS Blitter (i915) | ✅ Canary | ✅ |
| **AMD** | KiQ doorbell | 🟡 | 🟡 |
| **VirtIO** | VirtIO-GPU 2D | ✅ | ✅ QEMU |

- **VRAM Buddy Allocator** — power-of-2 split/merge over BAR2 UC
- **GPU Secure Boot WPR** — FECS+GPCCS upload via Falcon microcode
- **GPU Work-Queue G1/G2** — persistent op queue with CPU fallback

### 🧪 WASM Runtime in Kernel

```rust
// wasmi — real no_std WASM runtime with fuel, sandbox, capability gates
// Self-test: add(2, 3) = 5 — PASS
pub fn run_wasm(module: &[u8], fuel: u64) -> Result<i64, WasmError> {
    let engine = wasmi::Engine::new(&wasmi::Config::default());
    let module = wasmi::Module::new(&engine, module)?;
    let mut store = wasmi::Store::new(&engine, ());
    store.set_fuel(fuel)?;                          // fuel metering
    let linker = wasmi::Linker::new(&engine);
    // CapGate: host functions gated by capability tokens
    linker.func_wrap("aios", "send_tcp", |addr: i32| {
        if !has_capability(SendTcp) { deny(); }
        Ok(http_get(addr))
    })?;
    // ...
}
```

- **F0–F7 complete** (ADR-0059): decode harness, promote ephemeral→WASM, W^X arena
- **App Factory A/B/C**: wasmi (A, sandbox, default) / Cranelift JIT (B, gated) / Rust-subset native (C, gated)
- **CapGate on all host imports** — `aios::*` functions protected by capability tokens

### 🛡 Safety / Security / Self-Healing

| System | Innovation |
|--------|-----------|
| **SafetyAgent** | **Asimov's 4 Laws**: I1 process separation, I2 pre-action, I3 fail-closed, I4 signed evidence — Layer 0 = Cosmic Law (halt on violation) |
| **SecurityAgent** | 5 detectors + pipeline. DHCP starvation detection, trust chain |
| **Firmware Pipeline** | Missing firmware → LLM diagnoses → HTTP download → hot-load without reboot |
| **Skill Generation** | LLM generates skills on demand → SkillObserver registers hot |
| **SelfHeal I3/I4** | Unified pipeline: GPU / NIC / WiFi / any hardware |
| **Merkle Audit Trail** | SHA-256 chain + Ed25519 per entry |
| **Membrane** | Zero ambient authority sandbox (ADR-0076) |
| **JAIL** | Membrane + wasmi + Merkle audit trail (ADR-0076) |

### ⚡ Memory Hierarchy Index (MHI)

**Automatic tiered memory — no swap file nonsense.**

```
  VRAM  ←── GPU allocations (BAR2)
   ↑↓
  DRAM  ←── Hot data (Tier 1: talc)
   ↑↓
  NVMe  ←── Warm data (Tier 2: TensorArena bump)
   ↑↓
  SSD   ←── Cold data (MHI soft-migrate)
   ↑↓
  USB   ←── Archive data
```

- `mhi_tick()` metadata + DRAM memcpy
- `alloc_by_tier()` with ML-guided migration
- DMA NVMe/VRAM deferred

### 🌐 Cross-OS Ecosystem (ADR-0076)

**Neural OS is NOT an island. We discover and install skills from other OSes.**

```
  ┌─────────────┐     MCP      ┌──────────────┐
  │  Neural OS  │◄───────────►│    FYY OS    │
  │  (K³CHJ)    │             │  (skill hub) │
  └──────┬──────┘             └──────────────┘
         │ MCP
  ┌──────▼──────┐     MCP      ┌──────────────┐
  │  Wetware    │◄───────────►│   WeftOS     │
  │  (research) │             │  (sandbox)   │
  └─────────────┘             └──────────────┘
```

- **CrossOsAgent**: LEARN → PROPOSE → AUTO cycle
- **CrossOsDiscoverer**: PackageHub local + HTTP GitHub + MCP FYY/Wetware/WeftOS
- **IntentBus** canonical — skills found anywhere install locally

### 💤 SleepCycle — Bare-Metal Sleep/Learn Cycle

**The first bare-metal system with a cognitive sleep cycle.** Every boot makes the system smarter. No internet. No human.

```
  ┌─────────┐    ┌─────────┐    ┌───────────┐    ┌─────────┐    ┌─────────┐
  │ REPLAY  │───►│  DREAM  │───►│CONSOLIDATE│───►│  PRUNE  │───►│ REFLECT │
  │replay   │    │simulate │    │integrate  │    │remove    │    │meta-    │
  │episodes │    │scenarios│    │knowledge  │    │dead paths│    │analyze  │
  └─────────┘    └─────────┘    └───────────┘    └─────────┘    └─────────┘
```

- PollEvery(1000) — runs every ~1000 scheduler ticks
- Consolidates episodic memories, prunes dead routes, reflects on outcomes
- Inspired by Atkinson-Shiffrin + Ebbinghaus forgetting curves

### 🎨 Generative Card Desktop

**`embedded-graphics` 0.8 UI on bare-metal.** No X11. No Wayland. No GPU needed.

```rust
// Declarative UI — cards generated by LLM or WASM skills
let card = UiDeclaration::Card {
    title: "System Monitor",
    children: vec![
        UiNode::Gauge { label: "CPU", value: 42, min: 0, max: 100 },
        UiNode::KeyValue { key: "Memory", value: "342/512 MB" },
        UiNode::Button { label: "Details", action: "sys:details" },
    ],
};
compositor.spawn_window(card, Position { x: 100, y: 60 });
```

- **3 visual layers**: OrbBackground < HermesOverlay < AppWindows < DockBar
- **Z-order compositor** with 60Hz FPS control
- **FFT Audio → Orb**: 16-bin Goertzel drives responsive animation
- **Mouse PS/2**: close, drag, button clicks
- **Cards per LLM structured decode** (JSON `card_json_schema_hint`)
- **Orb + HUD preserved** — cards are overlay, not replacement

---

## 🤖 Agents

<details>
<summary><b>📋 25 Native Agents — Click to expand</b></summary>

| Code | Agent | Type | Schedule | Function |
|------|-------|------|----------|----------|
| A-001 | **SystemAgent** | System | Oneshot | Init, SYSTEM_READY, EchoSkill |
| A-002 | **MonitorAgent** | System | Oneshot | Publishes SYSTEM_READY |
| A-003 | **HwBridgeAgent** | Router | Continuous | Scancode IRQ bridge |
| A-004 | **NetAgent** | Network | Continuous | smoltcp poll + HTTP |
| A-005 | **InputAgent** | Console | Continuous | Keyboard (PS/2 + USB xHCI) |
| A-006 | **CortexAgent** | Inference | Continuous | LLM + Medusa + Trinity MoE |
| A-007 | **HermesAgent** | Router | Continuous | Intent routing + ReAct + Skills |
| A-008 | **DisplayAgent** | Console | Continuous | Framebuffer BGRA32 + compositor |
| A-009 | **NetDriverAgent** | Driver | Oneshot | RTL8139 + E1000 + i225 |
| A-010 | **UsbDriverAgent** | Driver | Oneshot | xHCI port scan + USB MSC |
| A-011 | **BootSelfHealAgent** | System | Oneshot | SelfHeal init |
| A-012 | **BootTrustAgent** | System | Oneshot | TrustCache init + Ed25519 |
| A-013 | **PlatformAgent** | System | Oneshot | PCI + ACPI + APIC + SMP |
| A-014 | **MemoryAgent** | System | Oneshot | MHI + Adaptive Heap + NVMe |
| A-015 | **GpuDriverAgent** | Driver | Oneshot | GPU backend detect + canary |
| A-016 | **HwDetectAgent** | System | Oneshot | HW Identify + IA device tree |
| A-017 | **CronAgent** | System | Continuous | Cron Scheduler (NTP, tasks) |
| A-018 | **SecurityAgent** | System | Continuous | 5 detectors + Pipeline |
| A-019 | **SafetyAgent** | System | Continuous | 4 Asimov invariants I1-I4 |
| A-020 | **OptimizerAgent** | System | Continuous | Self-Optimization |
| A-021 | **SleepCycleAgent** | System | PollEvery(1000) | REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT |
| A-022 | **AutoLearnAgent** | System | PollEvery(200) | Detects need → trains → registers expert |
| A-023 | **WifiAgent** | Network | Continuous | 802.11 scan + WPA2 + iwlwifi/ath10k |
| A-024 | **WakeWordAgent** | System | EventDriven | "Jarvis" keyword by energy |
| A-025 | **HdaAudioAgent** | Driver | Oneshot | Intel HDA audio capture + playback |

Plus **~147 SpecialistAgents** (EventDriven) in The Agency — hardware, filesystem, network, security, and application specialists.

</details>

---

## 🎬 Showcase

### Key Achievements

| Achievement | Detail | Evidence |
|---|---|---|
| **BitNet 2B loaded** | ~590MB, L=30, loaded via QEMU-loader, forward pass OK | `[STATUS]` telemetry |
| **E1000 RX fixed** | RX went from 0 to 184 packets after DMA uncached fix | SESSION_149/150 |
| **NetFs #418 PASS** | TCP peer at `gateway:4446`, LIST/READ/WRITE smoke test | SESSION_152 |
| **NTP sync** | Periodic resync, rotation of 4 servers | SESSION_152 |
| **DNS + HTTP** | Raw DNS, HTTP GET via smoltcp, Host header | SESSION_150/152 |
| **SMP 4-core wake** | 3 APs via sequential directed SIPI, PerCpu stacks | SESSION_163 |
| **S3 Suspend/Resume** | ACPI S3, device save/restore, resume trampoline | SESSION_163 |
| **Piper TTS loaded** | PT-BR + EN, 366 tensors, neural-lite | Sprint Sound |
| **STT CTC trained** | PCM→MFCC→2×LSTM→CTC, 55K params, 28 char vocab | Sprint Sound |
| **WASM add(2,3)=5** | wasmi real, fuel metering, CapGate on aios::* | ADR-0059 |
| **W^X arena `mov eax,42`=42** | Native JIT base, Ring 0 | ADR-0059 F7 |
| **GPU NVIDIA PUSH_BUFFER** | GPFIFO, doorbell, timeout — works on GTX 1050 HW real | Sprint 102 |
| **GPU VRAM Buddy Alloc** | Power-of-2 split/merge over BAR2 UC | Sprint 102 |
| **HW Expert v3 trained** | 61,453 VIDs/DIDs, 259KB, 1M params, loss 0.389 | Sprint 102 |
| **SDIO dataset** | 171,003 HWIDs, 65 DriverPacks, 20,054 .inf files | Public HF dataset |
| **ATA PIO fix** | `in al, dx+1` was FEATURES not data. Bug since v0.1 | v1.2.0 |

### Boot Log (Successful)
```
[BOOT] Phase 0: SafeHarbor — exception handlers installed
[BOOT] Phase 1: MemoryCore — bitmap frame allocator 8GB
[BOOT] Phase 2: SystemBringup — heap 512MB @ 0x4000000000
[BOOT] Phase 3: Diagnostics — SIMD/AVX2/SSE probed
[BOOT] Phase 4: HardwareDiscovery — PCI scan: 17 devices
[ATA]  master OK: QEMU HARDDISK (128MB)
[ATA]  slave OK: FAT32 data disk
[BOOT] Phase 5: DriverInit — E1000 @ 00:03.0
[NET]  e1000: link up, MAC 52:54:00:12:34:56
[BOOT] Phase 6: AgentFleet — 259 agents registered
[SCHEDULER] 259 runtime agents, 4 cores
[BOOT] Phase 7: Runtime — Hermes + Cortex + Display
[TIMER] tick=1 ... tick=42 ... tick=1337
```

---

## 🧠 Crate Map

| Crate | Ring | Lines | Role |
|-------|:----:|:-----:|------|
| [`k_nano`](crates/k_nano/) | **R0** | ~12,000 | Foundation: CPU, memory, PCI, ATA, NVMe, NIC, audio, filesystems, SMP, async |
| [`k_hal`](crates/k_hal/) | **R1** | ~2,000 | Hardware abstraction: DeviceCap, HalOffer, MMIO BE, VirtIO, GPU backends |
| [`cortex`](crates/cortex/) | **R2** | ~5,000 | Cognition: BitNet LLM, Trinity MoE, tensor engine, BPE, compute dispatch |
| [`k_ai`](crates/k_ai/) | **R2** | ~3,000 | Autonomy: SelfHeal, Trust, SGDB, inventory, audit, Agency |
| [`hermes`](crates/hermes/) | **R3** | ~4,000 | Orchestration: WASM, network, skills, apps, MCP, HalOffer client |
| [`jarbas`](crates/jarbas/) | **R3** | ~3,000 | Interface: Display FE, compositor, cards, persona, HUD |
| `neural-kernel` | **—** | ~2,000 | Boot binary: crate integration, residual bridge code |

---

## 🛤 Roadmap

| Phase | Status | Key Deliverables |
|-------|--------|-----------------|
| **v1.0.0** Code Freeze | ✅ | ATA fix, FAT32, NVMe, UEFI boot, 0 errors |
| **v1.5.0** K³CHJ Workspace | ✅ | 5 crates, wire N2.5→N5.7, migration complete |
| **v1.8.0** Adequação | ✅ | ADR-0042 N1–N5, ADR-0041 PoC H4+/H5+/AS |
| **v1.9.x** Pós-LAN | ✅ | NetFs, HTTP, NTP, SelfUpdate, 7 residual waves |
| **Sprint 107** Voice | ✅ | Piper TTS, STT CTC, WakeWord, HDA full duplex |
| **Sprint 108** Self-Evolve | ✅ | Auto-skill generation, promote ephemeral→WASM |
| **ADR-0057** Compute | ✅ | SMP 4-core, GPU dispatcher, structured decode |
| **ADR-0058** Card Desktop | ✅ | `embedded-graphics` UI, cards, compositor |
| **ADR-0059** App Factory | ✅ | wasmi, decode harness, promote, W^X arena |
| **v2.0.0 Gate** | ⏳ | Review + all backlog zeroed + human OK |

---

## 📚 Documentation

| Document | Purpose |
|----------|---------|
| [`AGENTS.md`](AGENTS.md) | Full agent ontology, boot sequence, operational rules |
| [`TECNOLOGIAS.md`](TECNOLOGIAS.md) | Complete technology catalog (353 entries) — our IP register |
| [`docs/architecture/INDEX.md`](docs/architecture/INDEX.md) | 47+ ADRs with lifecycle tracking |
| [`docs/memory/STATE.md`](docs/memory/STATE.md) | Current kernel state snapshot |
| [`docs/memory/IDEA_BANK.md`](docs/memory/IDEA_BANK.md) | 440+ ideas, cataloged and tracked |
| [`docs/memory/SESSION_INDEX.md`](docs/memory/SESSION_INDEX.md) | Session log index — critical lessons |
| [`ROADMAP.md`](ROADMAP.md) | Full roadmap v1.0 → v2.0 |
| [`TODO.md`](TODO.md) | Master task checklist |
| [`HOWTO.md`](HOWTO.md) | Build + run instructions for all environments |
| [`CHANGELOG.md`](CHANGELOG.md) | Version history |
| [`GOVERNANCE.md`](docs/GOVERNANCE.md) | IDEA → ADR → sprint → check cycle |

### Key Architectural Decision Records

- [ADR-0041](docs/architecture/0041-k2chj-capability-rings.md) — Capability rings P0–P9 (Ring3 isolation)
- [ADR-0042](docs/architecture/0042-*.md) — Boot OK → K³CHJ adequacy N1–N5
- [ADR-0045](docs/architecture/0045-sound-voice-stack.md) — Sound and voice pipeline
- [ADR-0057](docs/architecture/0057-compute-dispatch.md) — SMP+GPU+NPU compute dispatch
- [ADR-0058](docs/architecture/0058-generative-card-desktop.md) — Generative card desktop UI
- [ADR-0059](docs/architecture/0059-runtime-app-factory.md) — Runtime app factory (WASM)
- [ADR-0076](docs/architecture/0076-cross-os-ecosystem.md) — Cross-OS ecosystem
- [ADR-0079](docs/architecture/0079-neural-auto-installer.md) — Self-installer

---

## 🤝 Community

| Resource | Link |
|----------|------|
| **Source** | [github.com/msrovani/neural-os-core](https://github.com/msrovani/neural-os-core) |
| **HuggingFace** | [🤗 aios-k2chj](https://huggingface.co/aios-k2chj) |
| **SDIO HWID Dataset** | [🤗 171,003 HWIDs](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids) |
| **PCI/USB ID Dataset** | [🤗 48,346 IDs](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids) |
| **Firmware Metadata** | [🤗 1,207 entries](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata) |
| **Device LEGO Specs** | [`docs/specs/device-lego/`](docs/specs/device-lego/) |
| **JARVIS Original (.NET)** | [github.com/msrovani/jarvis](https://github.com/msrovani/jarvis) |

### Contributing

- **File issues** for bugs, feature requests, or ADR proposals
- **Submit PRs** — ensure `cargo check --release` = 0 errors
- **Join Discord** for architecture discussions
- **Contribute HWID data** — run `tools/extract_sdio_hw.py` on your DriverPacks
- **Model contributors** — train new experts, share .bitnet files on HF

---

## 📊 By the Numbers

```
  ~26,000     Lines of Rust (no_std, bare-metal)
  180+        Rust source files
  6           Production crates (k_nano → jarbas)
  0           Compilation errors
  ~50         Native agents (25 base + ~147 specialists)
  47+         Architecture Decision Records
  61,453      Hardware IDs recognized by kernel neural net
  171,003     SDIO HWIDs extracted and published
  1,207       Firmware metadata records
  116         Firmware blobs (~12.5 MB)
  440+        Ideas cataloged
  108+        Sprints completed
  5           AMDs (3M params, 259KB → 850M params, 590MB)
```

---

<div align="center">

```
J.A.R.V.I.S. — Just A Rather Very Intelligent System

"Thoughtful. Precise. Alive."

Built from scratch. No Linux. No POSIX. No cloud. No excuses.

⚡ The silicon obeys. ⚡
```

**Copyright © 2026 Marcelo Scapin Rovani — MIT License**

**[Start Building](#rocket-quick-start)** • **[Architecture](#-architecture)** • **[Why This Exists](#trophy-why-this-exists)** • **[Unique Tech](#fire-unique-technologies)** • **[ADRs](#key-architectural-decision-records)**

</div>
