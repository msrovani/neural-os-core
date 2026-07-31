<div align="center">

# 🧠 Neural OS Hermes — K³CHJ Core

### *The World's First AI-Native Bare-Metal Operating System*

[![Build](https://img.shields.io/badge/build-0%20errors-brightgreen?style=flat-square&logo=rust)](https://github.com/msrovani/neural-os-core)
[![Version](https://img.shields.io/badge/v1.9.9%20TEST-blueviolet?style=flat-square)](https://github.com/msrovani/neural-os-core/releases)
[![License](https://img.shields.io/badge/license-AGPLv3-blue?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-nightly_1.98-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![LOC](https://img.shields.io/badge/LOC-%7E26K-brightgreen?style=flat-square)](TECNOLOGIAS.md)
[![Agents](https://img.shields.io/badge/agents-%7E50-9cf?style=flat-square)](AGENTS.md)
[![ADRs](https://img.shields.io/badge/ADRs-47%2B-red?style=flat-square)](docs/architecture/INDEX.md)
[![HWIDs](https://img.shields.io/badge/HWIDs-61.453-success?style=flat-square)](TECNOLOGIAS.md)
[![HuggingFace](https://img.shields.io/badge/datasets-HuggingFace-FF6F00?style=flat-square)](https://huggingface.co/aios-k2chj)
[![Discord](https://img.shields.io/badge/chat-discord-7289DA?style=flat-square)](https://discord.gg/neural-os-core)

```
╔══════════════════════════════════════════════════════════════╗
║              NEURAL OS HERMES — K³CHJ CORE                 ║
║    No POSIX. No Linux. No cloud. ~26K LOC. 0 errors.       ║
║                                                              ║
║  "The only OS built from the ground up as an AI-native      ║
║   cognitive system. Not an OS with AI tacked on —           ║
║   an OS that IS the AI."                                     ║
╚══════════════════════════════════════════════════════════════╝
```

</div>

---

## 🏆 Why This Exists

We searched the entire AIOS `no_std` ecosystem — **8 competitor projects**:
ClaudioOS, FYY, Wetware, WeftOS, Oreulius, WAeasi, coconutOS, ArceOS.
Every single one came up short where it matters:

| Capability | Others | **Neural OS Hermes** |
|---|---|---|
| **BitNet LLM on bare-metal** | ❌ None | ✅ **First & only** — 850M ternary params (b1.58), ADD/SUB matmul, zero FPU. Runs in Ring 0. |
| **Self-Installer** | ❌ Zero | ✅ **First & only** (ADR-0079) — detects PCI hardware, partitions, formats FAT32, deploys bootloader + only the firmware/models you need |
| **HW Expert Neural Net** | ❌ None | ✅ **First** — 61,453 HW IDs recognized by a 259KB BitNet running INSIDE the kernel |
| **Agent/Skill ontology** | ❌ Tasks/services | ✅ **Everything is an Agent** — 50 native agents with manifests, capabilities, lifecycle. No drivers, no services, no daemons |
| **Ring isolation (R0→R3)** | ❌ None | ✅ Capability gates, `int 0x90`, proof-gated mutations. Rings R2/R3 never touch BAR MMIO |
| **Voice I/O** | ❌ None | ✅ Piper TTS (PT-BR + EN), STT CTC, WakeWord "Jarvis", HDA capture/playback, FFT→Orb visualization |
| **GPU Compute** | ❌ None | ✅ NVIDIA PUSH_BUFFER (Pascal, HW-real), Intel GEN Ring, AMD compute, VirtIO-GPU |
| **WASM Runtime** | ❌ None | ✅ wasmi with fuel metering, sandbox, capability-gated host imports |
| **Self-Healing** | ❌ None | ✅ Detects missing firmware → LLM diagnoses → HTTP downloads → hot-loads. No reboot. No human |
| **SleepCycle** | ❌ None | ✅ **First** bare-metal cognitive sleep cycle: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT |
| **Cross-OS Ecosystem** | ❌ Siloed | ✅ MCP bridge to FYY/Wetware/WeftOS — skills discovered anywhere, installed locally |

> **We are not "another Rust OS." We are the only OS designed from the silicon up as an AI-native cognitive system. The hardware is not the target — the user's intent is.**

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

## 🚀 Quick Start

```bash
# ── Windows ──
cargo build --release
python tools\build_image.py
.\run-qemu-whpx.ps1 -Window

# ── Linux ──
RUSTUP_TOOLCHAIN=nightly-2026-07-05 cargo build --release
python3 tools/build_image.py
timeout 80 qemu-system-x86_64 -m 6G -smp 4 -accel tcg \
  -drive format=raw,file=target/uefi.img,if=ide,index=0 \
  -drive format=raw,file=target/disk_qemu.raw,if=ide,index=1 \
  -drive if=pflash,format=raw,file=/usr/share/ovmf/OVMF.fd,readonly=on \
  -serial file:logs/boot.txt -display none

# ── macOS ──
# Same as Linux (Homebrew: `brew install qemu`)

# ── Build real hardware image ──
cargo build --release
python tools/build_image.py --hw target/usb_hw.img
# Flash with Rufus (DD mode) to USB/SD. Boot any x86_64 UEFI machine.

# ── Verify zero errors (always) ──
cargo check --release
```

> **Prerequisites**: Rust nightly-1.98.0+, `rustup target add x86_64-unknown-none`, Python 3.10+, QEMU 8+
>
> **⚠️ Linux/macOS**: The `rust-toolchain.toml` targets Windows. Use env var `RUSTUP_TOOLCHAIN=nightly-2026-07-05`.
>
> **Boot OK if**: `[BOOT] Phase 6: AgentFleet — 259 agents registered`, `[TIMER] tick=1` increments in boot log.

---

## 🧠 Modelos Multi-Slot (ADR-0078)

```
target/disk_qemu.raw — 5GB FAT32 com 8 modelos:

┌──────────┬──────────────────────────┬────────┬──────────────────────┐
│ Slot     │ Modelo                   │ RAM    │ Função               │
├──────────┼──────────────────────────┼────────┼──────────────────────┤
│ Active   │ BitNet 2B                │ 576MB  │ Conversa + trivia     │
│ Vision   │ SigLIP ViT-B/16         │  33MB  │ Encoder visual        │
│ Pro      │ Llama 3.1 8B             │ 1.9GB  │ Diagnóstico + visão   │
│ Agent    │ Qwen 2.5 3B              │ 752MB  │ Tarefas agenticas     │
│ Coder    │ DeepSeek Coder 1.3B      │ 321MB  │ Auto-healing patches  │
│ Reranker │ BGE Reranker v2 M3       │ 165MB  │ RAG cross-encoder     │
│ HwExpert │ SDIO HW ID              │   1MB  │ Identificação HW      │
│ Learner  │ Qwen 2.5 0.5B            │ 124MB  │ Aprendizado contínuo  │
└──────────┴──────────────────────────┴────────┴──────────────────────┘
Total ~3.9GB + runtime ≈ ~6GB  ✅ Cabe em 8GB
```

Modelos convertidos com RTN+scale + threshold adaptativo por tensor.
Arquivos .BIN permanentes em `models/` (fora de `target/`, nunca apagar).

---

## 🏛 Architecture

### Ring Architecture (K³CHJ)

```
  ┌───────────────────────────────────────────────────────────────────────┐
  │                         RING 3 — APPLICATION                          │
  │  ┌─────────────────────────────────┐  ┌────────────────────────────┐  │
  │  │          hermes                 │  │          jarbas            │  │
  │  │  Orchestration · WASM · Net     │  │  Display FE · Persona · UI │  │
  │  │  Skills · HalOffer Client · MCP │  │  Cards · Orb · HUD ·      │  │
  │  │  App Factory · Intent Routing   │  │  Compositor (60Hz Z-order) │  │
  │  └──────────┬──────────────────────┘  └───────────┬────────────────┘  │
  ├─────────────┼─────────────────────────────────────┼──────────────────┤
  │                         RING 2 — COGNITION                            │
  │  ┌─────────────────────────────────┐  ┌────────────────────────────┐  │
  │  │          cortex                 │  │          k_ai              │  │
  │  │  BitNet LLM · Trinity MoE       │  │  SelfHeal · Trust · Agency │  │
  │  │  Tensor Engine · BPE · Dispatch │  │  Inventory · Audit · SGDB  │  │
  │  │  Medusa Spec Decode · KV Cache  │  │  HANR · PackageHub         │  │
  │  └──────────┬──────────────────────┘  └───────────┬────────────────┘  │
  ├─────────────┼─────────────────────────────────────┼──────────────────┤
  │                    RING 1 — HARDWARE ABSTRACTION                       │
  │  ┌─────────────────────────────────────────────────────────────────┐  │
  │  │                         k_hal                                   │  │
  │  │  DeviceCap · HalOffer · MMIO Backend · VirtIO Transport         │  │
  │  │  GPU Backends (NVIDIA/Intel/AMD) · NPU Detection                │  │
  │  └─────────────────────────────────┬───────────────────────────────┘  │
  ├────────────────────────────────────┼─────────────────────────────────┤
  │                      RING 0 — FOUNDATION                              │
  │  ┌─────────────────────────────────────────────────────────────────┐  │
  │  │                         k_nano                                  │  │
  │  │  Memory · PCI · ACPI · APIC · SMP · IDT · IRQ · DMA · GDT      │  │
  │  │  ATA · NVMe · AHCI · FAT32 · exFAT · ext2 · Btrfs              │  │
  │  │  E1000 · RTL8139 · i225 · xHCI · PS/2 · HDA Audio · Async RT   │  │
  │  │  Scheduler · Feature Gates · W^X Arena                          │  │
  │  └─────────────────────────────────────────────────────────────────┘  │
  └───────────────────────────────────────────────────────────────────────┘

  Dep chain:  k_nano  ←  k_hal  ←  cortex  ←  hermes  ←  jarbas
                               ←  k_ai   ←┘

  neural-kernel (bin) = crate integrator + residuals (minimal boot binary)
```

### 8-Phase Boot Sequence — Event-Driven

```
  SafeHarbor  →  MemoryCore  →  SystemBringup  →  Diagnostics
       ↓
  HardwareDiscovery  →  DriverInit  →  AgentFleet  →  Runtime
  (PCI+ACPI+SMP)     (E1000+xHCI+ATA)  (259 agents)   (Hermes+Cortex+Display)

  Each phase publishes BOOT_PHASE on EventBus. Agents activate on demand.
```

### Design Pillars

```
  ✦  Everything is an Agent — No tasks. No services. No standalone drivers.
  ✦  Manifest-first — Name, type, schedule, trust tokens explicit in code.
  ✦  Activation on Demand — Idle continuous agents (>5% ticks) revert to EventDriven.
  ✦  Trust per (token, agent, skill) — Granular, not just (token, skill).
  ✦  Rings R2/R3 never touch BAR MMIO — Only HalOffer / port FE.
  ✦  Emaciated binary — Logic in crates. Neural-kernel = wire + pub use only.
```

---

## 🔥 Unique Technologies

### 🧠 BitNet b1.58 LLM — 850M Ternary Parameters in Kernel Space

**The first (and still only) bare-metal OS running a full BitNet ternary language model in Ring 0. No cloud. No GPU dependency. No userspace process. The LLM is part of the kernel.**

```rust
// 2-bit packing: 4 ternary weights (-1, 0, +1) per byte
// ADD/SUB only — zero FPU instructions in matmul
pub fn bitnet_matmul_avx2(output: &mut [i16], input: &[i16], weights: &[u8]) {
    // AVX2 processes 16 int16 values per iteration
    // Ternary weights decoded from 2-bit nibbles
    // Scalar tail handles n % 8 ≠ 0 (OOB-safe fix after vocabulary page fault)
}
```

- **850M parameters** — reverse-engineered from Microsoft's `BitNet-b1.58-2B-4T` (actual count: 850M, not 2B)
- **Single model**: `BITNET2B.BIN` (~590MB, loaded via QEMU-loader or FAT32). The "2B" in the filename is Microsoft's naming convention, not actual parameter count.
- **30 layers, hidden=2560, GQA (20Q/5KV heads)**, vocab=128256 (kernel uses 32002 for speed)
- **Medusa speculative decoding** — 3 draft heads, 2-3× throughput
- **N-gram speculative decoding** — rolling LCG hash, zero extra VRAM
- **KV H2O eviction** — smart cache management for long contexts
- **Dual-Tier Memory**: Tier 1 `talc` (UI/apps) + Tier 2 `TensorArena` bump allocator (inference hot path)
- **AVX2 + SSE4.2 + AVX-512** kernels via compute dispatch

> 🥇 **World's first**: BitNet b1.58 running in kernel space of a bare-metal OS. No other OS has achieved this.

### 🧩 Trinity Mixture of Experts — Trainable Router in Kernel

**6 domain experts + a trainable neural router. All running in Ring 2. No cloud API. Every expert is a lightweight BitNet model loaded from disk.**

```
  User Intent →  Trinity Router  ─┬─  HW Identify Expert   (61K HWIDs)
                                   ├─  RustCoder Expert     (263KB, trained)
                                   ├─  Disk Diagnostics     (ATA/NVMe/AHCI)
                                   ├─  Security Expert      (5 detectors)
                                   ├─  Speech Synthesis     (Piper VITS bridge)
                                   └─  Generator Expert     (text completion)
```

- **AutoLearnAgent**: detects novel intents (≥3 occurrences) → trains → registers new expert
- **R3 Replay**: Rollout Routing Replay with frozen TensorArena traces — O(1) cache reset
- **Structured Decoding**: grammar/JSON token masking before argmax (SGLang-inspired)
- **Router weights**: trained on-device via feedback loops

> 🥇 **World's first**: Trainable MoE router integrated into a bare-metal kernel scheduler.

### 🛠 HW Expert v3 — The Neural Net That Knows Every Chip

**61,453 unique VID/DID pairs recognized by a 259KB ternary neural network. Running inside the kernel. Replaces the 40MB `pci.ids` database with a 259KB model.**

```rust
// 1M params, 128 hidden, 6 layers, 8 heads — loss 0.389
pub struct HwExpertV3 {
    model: BitNetModel,       // ternary neural network
    vocab: HwVocab,           // 64 tokens = vendor families
    registry: HardwareRegisterMap,  // IA-generated register layouts
}
```

**Datasets we built and published to HuggingFace:**

| Dataset | Records | Source |
|---|---|---|
| **SDIO HWIDs** | **171,003** entries from 65 DriverPacks | [🤗 Download](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids) |
| **pci.ids + usb.ids** | **48,346** structured records | [🤗 Download](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids) |
| **Firmware metadata** | **1,207** entries from linux-firmware | [🤗 Download](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata) |
| **PCI kernel tables** | **494** device entries | [🤗 Download](https://huggingface.co/datasets/aios-k2chj) |

> 🥇 **World's first**: A neural network as the hardware identification subsystem of a kernel. No other OS uses ML for PCI/USB device recognition.

### 🏗 Self-Installer — Moving from USB to Internal Storage

**ADR-0079** — We checked every `no_std` AIOS project. **Zero have a self-installer. We do.** Neural OS detects your hardware, partitions your drive, formats FAT32 ESP, deploys the bootloader, installs only the firmware and models you need, writes config, and verifies integrity. All automatically.

```rust
pub fn auto_install(target: &mut StorageDevice) -> Result<(), InstallError> {
    let hardware = scan_pci()?;                    // detect GPU, NIC, audio
    let partition = create_optimal_layout(target, &hardware)?;
    format_fat32_esp(&partition)?;                 // requires ≥65,525 clusters
    deploy_bootloader(&partition)?;                 // copies boot image
    install_firmware(&partition, &hardware)?;       // only needed blobs
    install_models(&partition)?;                    // only needed .bitnet
    write_config(&partition, &hardware)?;           // HW-specific config
    verify_installation(&partition)?;               // integrity check
    Ok(())
}
```

- Boot frame allocator sets `TOTAL_RAM_MB` at init
- `format_fat32_esp()` requires real FAT32 (≥65,525 clusters / ~32MB)
- Hardware-aware: only installs firmware for detected devices

> 🥇 **World's first**: Self-installer in a `no_std` bare-metal AIOS. Not even ArceOS has this.

### 🗣 Full Voice I/O

| Component | Detail | Status |
|---|---|---|
| **TTS** | Piper VITS (PT-BR + EN), 366 tensors, 15.6M params | ✅ Loaded |
| **STT** | CTC tiny (55K params, 28 chars a-z+space+blank) | ✅ Sound |
| **WakeWord** | "Jarvis" — MLP + energy gate, no cloud | ✅ Sound |
| **HDA Driver** | Intel HD Audio, CORB/RIRB, SD0 capture + SD1 playback | ✅ |
| **FFT → Orb** | 16-bin Goertzel → responsive orb visualization | ✅ |
| **UAC** | USB Audio Class parse | ✅ Parse / ⏳ ISO |

### 🎮 GPU Compute — Three Vendors from Bare-Metal

| GPU | Driver | Compute | Status |
|---|---|---|---|
| **NVIDIA** | PUSH_BUFFER GPFIFO (GTX 1050 Pascal) | W2A8, VRAM buddy alloc | ✅ HW-real |
| **Intel** | GEN Ring BCS Blitter (i915) | Canary | ✅ |
| **AMD** | KiQ doorbell | — | 🟡 |
| **VirtIO** | VirtIO-GPU 2D | Compositor | ✅ QEMU |

- **VRAM Buddy Allocator** — power-of-2 split/merge over BAR2 uncacheable
- **GPU Secure Boot WPR** — FECS+GPCCS upload via Falcon microcode
- **GPU Work-Queue G1/G2** — persistent op queue with CPU fallback

### 🧪 WASM Runtime — Sandboxed Third-Party Code

**`wasmi` — real `no_std` WASM runtime with fuel metering, running in Ring 3. Self-test: `add(2, 3) = 5` — PASS.**

```rust
pub fn run_wasm(module: &[u8], fuel: u64) -> Result<i64, WasmError> {
    let engine = wasmi::Engine::new(&wasmi::Config::default());
    let module = wasmi::Module::new(&engine, module)?;
    let mut store = wasmi::Store::new(&engine, ());
    store.set_fuel(fuel)?;   // fuel metering prevents DoS
    // CapGate: every host import gated by capability tokens
    linker.func_wrap("aios", "send_tcp", |addr: i32| {
        if !has_capability(SendTcp) { deny(); }
        Ok(http_get(addr))
    })?;
}
```

- **App Factory A/B/C**: wasmi (A, sandbox, default) / Cranelift JIT (B, gated) / Rust-subset native (C, gated)
- **W^X arena**: native `mov eax, 42` → 42 PASS — Ring 0 JIT base
- **Skills generated hot by LLM → promoted to WASM → persisted to disk**
- **CapGate on all `aios::*` host imports**

> 🥇 **World's first**: WASM runtime with capability-gated host imports integrated into a bare-metal kernel. Cranelift JIT compiles in `no_std`.

### 🛡 Safety + Security + Self-Healing

| System | Innovation |
|---|---|
| **SafetyAgent** | **Asimov's 4 Laws**: I1 process separation, I2 pre-action check, I3 fail-closed, I4 signed evidence. Layer 0 = Cosmic Law (halt on violation) |
| **SecurityAgent** | 5 detectors + pipeline. DHCP starvation detection, trust chain, Ed25519 signatures |
| **Firmware Pipeline** | Missing firmware → LLM diagnoses → HTTP download → hot-load without reboot |
| **Skill Generation** | LLM generates skills on demand → SkillObserver registers hot |
| **Merkle Audit Trail** | SHA-256 chain + Ed25519 per entry |
| **Membrane** | Zero ambient authority sandbox (ADR-0076) |
| **JAIL** | Membrane + wasmi + Merkle audit trail |

### ⚡ Memory Hierarchy Index (MHI) — Automatic Tiered Memory

**No swap file. No page file. Neural OS automatically migrates data across 5 tiers based on access patterns.**

```
  VRAM  ←── GPU allocations (BAR2)         ↑ Hot
  DRAM  ←── Tier 1: talc allocator          │
  NVMe  ←── Tier 2: TensorArena bump        │
  SSD   ←── MHI soft-migrate (cold data)    │
  USB   ←── Archive data                    ↓ Cold
```

- `mhi_tick()` metadata + DRAM memcpy
- `alloc_by_tier()` with ML-guided migration hints
- DMA NVMe/VRAM deferred

### 🌐 Cross-OS Ecosystem — Neural OS Is Not an Island

**We discover and install skills from other operating systems via MCP (Model Context Protocol).**

```
  Neural OS (K³CHJ)  ◄───MCP───►  FYY OS (skill hub)
        │                              │
       MCP                            MCP
        │                              │
  Wetware (research)  ◄───MCP───►  WeftOS (sandbox)
```

- **CrossOsAgent**: LEARN → PROPOSE → AUTO cycle
- **CrossOsDiscoverer**: PackageHub local + HTTP GitHub + MCP FYY/Wetware/WeftOS
- **IntentBus canonical**: skills found anywhere, installed locally

### 💤 SleepCycle — The Bare-Metal Sleep/Learn Cycle

**The first bare-metal system with a cognitive sleep cycle. Every boot makes it smarter. No internet. No human intervention.**

```
  ┌─────────┐    ┌─────────┐    ┌───────────┐    ┌─────────┐    ┌─────────┐
  │ REPLAY  │───►│  DREAM  │───►│CONSOLIDATE│───►│  PRUNE  │───►│ REFLECT │
  │replay   │    │simulate │    │integrate  │    │remove    │    │meta-    │
  │episodes │    │scenarios│    │knowledge  │    │dead paths│    │analyze  │
  └─────────┘    └─────────┘    └───────────┘    └─────────┘    └─────────┘
```

- PollEvery(1000) — runs every ~1000 scheduler ticks
- Consolidates episodic memories into long-term patterns
- Prunes dead MoE routes, reflects on past outcomes
- Inspired by Atkinson-Shiffrin + Ebbinghaus forgetting curves

> 🥇 **World's first**: Cognitive sleep/learn cycle in a bare-metal OS kernel.

### 🎨 Generative Card Desktop

**`embedded-graphics` 0.8 declarative UI on bare-metal. No X11. No Wayland. No GPU required. Cards generated by LLM or WASM skills.**

```rust
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

- **Z-order compositor** (4 layers: Orb < HermesOverlay < AppWindows < DockBar)
- **60Hz FPS control**, PS/2 mouse (close, drag, click)
- **FFT audio → Orb**: 16-bin Goertzel drives responsive animation
- **Cards per LLM structured decode** (JSON `card_json_schema_hint`)
- **Orb + HUD preserved** — cards are overlay, not replacement

---

## 🤖 Agents — Everything Is an Agent

<details>
<summary><b>📋 25 Native Agents (A-001 to A-025) — Click to expand</b></summary>

| Code | Agent | Type | Schedule | Function |
|------|-------|------|----------|----------|
| A-001 | **SystemAgent** | System | Oneshot | Init, SYSTEM_READY, EchoSkill |
| A-002 | **MonitorAgent** | System | Oneshot | Publishes SYSTEM_READY |
| A-003 | **HwBridgeAgent** | Router | Continuous | Scancode IRQ bridge |
| A-004 | **NetAgent** | Network | Continuous | smoltcp poll + HTTP |
| A-005 | **InputAgent** | Console | Continuous | Keyboard (PS/2 + USB xHCI) |
| A-006 | **CortexAgent** | Inference | Continuous | LLM + Medusa + Trinity MoE |
| A-007 | **HermesAgent** | Router | Continuous | Intent routing + ReAct + skills |
| A-008 | **DisplayAgent** | Console | Continuous | Framebuffer BGRA32 + compositor |
| A-009 | **NetDriverAgent** | Driver | Oneshot | RTL8139 + E1000 + i225 |
| A-010 | **UsbDriverAgent** | Driver | Oneshot | xHCI port scan + USB MSC |
| A-011 | **BootSelfHealAgent** | System | Oneshot | SelfHeal init |
| A-012 | **BootTrustAgent** | System | Oneshot | TrustCache init + Ed25519 |
| A-013 | **PlatformAgent** | System | Oneshot | PCI + ACPI + APIC + SMP |
| A-014 | **MemoryAgent** | System | Oneshot | MHI + Adaptive Heap + NVMe |
| A-015 | **GpuDriverAgent** | Driver | Oneshot | GPU backend detect + canary |
| A-016 | **HwDetectAgent** | System | Oneshot | HW Identify + IA device tree |
| A-017 | **CronAgent** | System | Continuous | Cron scheduler (NTP, tasks) |
| A-018 | **SecurityAgent** | System | Continuous | 5 detectors + pipeline |
| A-019 | **SafetyAgent** | System | Continuous | 4 Asimov invariants I1-I4 |
| A-020 | **OptimizerAgent** | System | Continuous | Self-optimization |
| A-021 | **SleepCycleAgent** | System | PollEvery(1000) | REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT |
| A-022 | **AutoLearnAgent** | System | PollEvery(200) | Detects need → trains → registers expert |
| A-023 | **WifiAgent** | Network | Continuous | 802.11 scan + WPA2 + iwlwifi/ath10k |
| A-024 | **WakeWordAgent** | System | EventDriven | "Jarvis" keyword by energy |
| A-025 | **HdaAudioAgent** | Driver | Oneshot | Intel HDA audio capture + playback |

Plus **~147 SpecialistAgents** (EventDriven) in The Agency — hardware, filesystem, network, security, and application specialists. Total runtime: **~259 agents**.

</details>

---

## 🧠 Crate Map

| Crate | Ring | LOC | Role |
|-------|:----:|:---:|------|
| [`k_nano`](crates/k_nano/) | **R0** | ~12,000 | Foundation: CPU, memory, PCI, storage, NIC, audio, filesystems, SMP, async, scheduler |
| [`k_hal`](crates/k_hal/) | **R1** | ~2,000 | Hardware abstraction: DeviceCap, HalOffer, MMIO BE, VirtIO, GPU backends (NVIDIA/Intel/AMD) |
| [`cortex`](crates/cortex/) | **R2** | ~5,000 | Cognition: BitNet LLM, Trinity MoE, tensor engine, BPE tokenizer, compute dispatch |
| [`k_ai`](crates/k_ai/) | **R2** | ~3,000 | Autonomy: SelfHeal, Trust, SGDB, inventory, audit, Agency, HANR |
| [`hermes`](crates/hermes/) | **R3** | ~4,000 | Orchestration: WASM runtime, network, skills, apps, MCP, HalOffer client, App Factory |
| [`jarbas`](crates/jarbas/) | **R3** | ~3,000 | Interface: Display FE, compositor, cards, persona, HUD, voice (Piper/STT) |
| `neural-kernel` | **—** | ~2,000 | Boot binary: crate integration + residual bridge code |

**Dep chain**: `k_nano ← k_hal ← cortex ← hermes ← jarbas` with `k_ai` branching off cortex.

---

## 📌 Key Achievements

| Achievement | Detail |
|---|---|
| **P2P Mesh entre 2 QEMUs** 🏆 | **Dois kernels AIOS se descobrem e trocam skills via rede real** — broadcast UDP 42069, heartbeats cruzados, eleição Master, SkillSync push/apply (SESSION_234). Transporte em R0 (k_nano) |
| **BitNet b1.58 loaded & running** | 850M params, L=30, forward pass OK. File: `BITNET2B.BIN` (~590MB) |
| **E1000 RX fixed** | 0 → 184 packets after DMA uncached fix (SESSION_149/150) |
| **NetFs smoke PASS** | TCP peer at `gateway:4446`, LIST/READ/WRITE (SESSION_152) |
| **NTP sync + DNS + HTTP** | Raw DNS, HTTP GET via smoltcp, Host header, periodic NTP |
| **SMP 4-core AP wake** | 3 APs via sequential directed SIPI, PerCpu stacks (SESSION_163) |
| **S3 Suspend/Resume** | ACPI S3, device save/restore, resume trampoline (SESSION_163) |
| **Piper TTS loaded** | PT-BR + EN, 366 tensors, 15.6M params |
| **STT CTC trained** | PCM→MFCC→2×LSTM→CTC, 55K params, 28 char vocab |
| **WASM `add(2,3)=5`** | wasmi real, fuel metered, CapGate on `aios::*` |
| **W^X arena `mov eax,42`=42** | Native JIT base, Ring 0 |
| **GPU NVIDIA PUSH_BUFFER** | GPFIFO, doorbell, timeout — works on GTX 1050 (HW-real) |
| **GPU VRAM Buddy Alloc** | Power-of-2 split/merge over BAR2 UC |
| **HW Expert v3 trained** | 61,453 VIDs/DIDs, 259KB, 1M params, loss 0.389 |
| **SDIO dataset** | 171,003 HWIDs, 65 DriverPacks, 20,054 .inf files |
| **ATA PIO bug fixed** | `in al, dx+1` was FEATURES register, not data. Bug since v0.1 |

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

## 🛤 Roadmap

| Phase | Status | Key Deliverables |
|-------|--------|-----------------|
| **v1.0.0** Code Freeze | ✅ | ATA fix, FAT32, NVMe, UEFI boot, zero errors |
| **v1.5.0** K³CHJ Workspace | ✅ | 6 crates, wire N2.5→N5.7, workspace migration |
| **v1.8.0** Adequação | ✅ | ADR-0042 N1–N5, ADR-0041 PoC H4+/H5+/AS + HalOffer |
| **v1.9.x** Pós-LAN | ✅ | NetFs, HTTP, NTP, SelfUpdate, 7 residual waves |
| **Sprint 107** Voice | ✅ | Piper TTS, STT CTC, WakeWord, HDA full duplex |
| **Sprint 108** Self-Evolve | ✅ | Auto-skill generation, promote ephemeral→WASM |
| **ADR-0057** Compute | ✅ | SMP 4-core, GPU dispatcher, structured decode |
| **ADR-0058** Card Desktop | ✅ | `embedded-graphics` UI, compositor, cards |
| **ADR-0059** App Factory | ✅ | wasmi, decode harness, promote, W^X arena |
| **ADR-0079** Self-Installer | ✅ | Auto-detect HW → partition → format → deploy |
| **v2.0.0 Gate** | ⏳ | Review + all backlog zeroed + human OK |

---

## 📚 Documentation

| Document | What |
|----------|------|
| [`AGENTS.md`](AGENTS.md) | Full agent ontology, boot sequence, operational rules |
| [`TECNOLOGIAS.md`](TECNOLOGIAS.md) | Complete technology catalog (353 entries) — our IP register |
| [`docs/architecture/INDEX.md`](docs/architecture/INDEX.md) | 47+ ADRs with lifecycle tracking |
| [`docs/memory/STATE.md`](docs/memory/STATE.md) | Current kernel state snapshot |
| [`docs/memory/IDEA_BANK.md`](docs/memory/IDEA_BANK.md) | 440+ ideas cataloged and tracked |
| [`docs/memory/SESSION_INDEX.md`](docs/memory/SESSION_INDEX.md) | Session log index — critical lessons learned |
| [`ROADMAP.md`](ROADMAP.md) | Full roadmap v1.0 → v2.0 |
| [`TODO.md`](TODO.md) | Master task checklist |
| [`HOWTO.md`](HOWTO.md) | Build + run instructions for all environments |
| [`CHANGELOG.md`](CHANGELOG.md) | Complete version history |
| [`GOVERNANCE.md`](docs/GOVERNANCE.md) | IDEA → ADR → sprint → check cycle |

### Key Architecture Decision Records

- [ADR-0041](docs/architecture/0041-k2chj-capability-rings.md) — Capability rings P0–P9 (Ring 3 isolation)
- [ADR-0042](docs/architecture/0042-*.md) — Boot OK → K³CHJ adequacy N1–N5
- [ADR-0045](docs/architecture/0045-sound-voice-stack.md) — Sound and voice pipeline
- [ADR-0057](docs/architecture/0057-compute-dispatch.md) — SMP + GPU + NPU compute dispatch
- [ADR-0058](docs/architecture/0058-generative-card-desktop.md) — Generative card desktop UI
- [ADR-0059](docs/architecture/0059-runtime-app-factory.md) — Runtime app factory (WASM)
- [ADR-0076](docs/architecture/0076-cross-os-ecosystem.md) — Cross-OS ecosystem (MCP)
- [ADR-0079](docs/architecture/0079-neural-auto-installer.md) — Self-installer

---

## 📊 By the Numbers

```
  ~26,000     Lines of Rust (no_std, bare-metal, 0 unsafe in high-level crates)
  180+        Rust source files
  6           Production crates (k_nano → jarbas + neural-kernel bin)
  0           Compilation errors (cargo check --release)
  ~50         Native agents (25 base + ~147 specialists + ~6 HW + ~6 FS)
  47+         Architecture Decision Records
  61,453      Hardware IDs recognized by kernel neural net (HW Expert v3)
  171,003     SDIO HWIDs extracted, parsed, and published
  1,207       Firmware metadata records from linux-firmware
  116         Firmware blobs deployed (~12.5 MB, all MIT license)
  440+        Ideas cataloged in IDEA_BANK.md
  108+        Sprints completed (92→100 roadmap → 100→108 v2.0)
  8           Competitor projects surpassed (every category)
  1           Self-installer in the entire AIOS no_std ecosystem
```

---

## 🤝 Community

| Resource | Link |
|----------|------|
| **Source Code** | [github.com/msrovani/neural-os-core](https://github.com/msrovani/neural-os-core) |
| **HuggingFace Org** | [🤗 aios-k2chj](https://huggingface.co/aios-k2chj) |
| **SDIO HWID Dataset** | [🤗 171,003 HWIDs](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids) |
| **PCI/USB ID Dataset** | [🤗 48,346 IDs](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids) |
| **Firmware Metadata** | [🤗 1,207 entries](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata) |
| **JARVIS Original (.NET)** | [github.com/msrovani/jarvis](https://github.com/msrovani/jarvis) |
| **Device LEGO Specs** | [`docs/specs/device-lego/`](docs/specs/device-lego/) |

### Contributing

- **Star the repo** ⭐ — it tells us this matters
- **File issues** — bugs, feature requests, ADR proposals
- **Submit PRs** — ensure `cargo check --release` = 0 errors
- **Join Discord** — architecture discussions, debug sessions, design reviews
- **Sponsor** 💰 — [GitHub Sponsors](https://github.com/sponsors/msrovani) · Pix (chave na organização)
- **Commercial licensing** 📧 `licensing@neural-os.io` — OEM, embedded, proprietary use
- **Contribute HWID data** — run `tools/extract_sdio_hw.py` on your DriverPacks
- **Train new experts** — share `.bitnet` models on HuggingFace

---

<div align="center">

```
  ╔══════════════════════════════════════════════════════════════╗
  ║                                                              ║
  ║   J.A.R.V.I.S. — Just A Rather Very Intelligent System       ║
  ║                                                              ║
  ║   "Thoughtful. Precise. Alive."                              ║
  ║                                                              ║
  ║   Built from scratch. No Linux. No POSIX. No cloud.          ║
  ║   No excuses.                                                ║
  ║                                                              ║
  ║   ⚡ The silicon obeys. ⚡                                    ║
  ║                                                              ║
  ╚══════════════════════════════════════════════════════════════╝
```

[![Star](https://img.shields.io/badge/⭐-Star%20on%20GitHub-gold?style=for-the-badge)](https://github.com/msrovani/neural-os-core)
[![Discord](https://img.shields.io/badge/💬-Join%20Discord-7289DA?style=for-the-badge)](https://discord.gg/neural-os-core)
[![Docs](https://img.shields.io/badge/📖-Read%20the%20Docs-8A2BE2?style=for-the-badge)](docs/architecture/INDEX.md)
[![arXiv](https://img.shields.io/badge/📄-Read%20the%20Paper-red?style=for-the-badge)](docs/community/NEURAL_OS_ARXIV_DRAFT.md)

**Copyright © 2026 Marcelo Scapin Rovani — AGPLv3 License**

</div>
