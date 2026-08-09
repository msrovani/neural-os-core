# Neural OS Hermes — K³CHJ Core

A bare-metal operating system written in Rust (`no_std` + `no_main`), booting from
UEFI, with an in-kernel ternary (BitNet b1.58) inference engine. Networking, storage,
audio, a sandboxed WASM runtime and a small UI stack run natively — no POSIX, no
Linux, no cloud dependency.

[![CI](https://github.com/msrovani/neural-os-core/actions/workflows/ci.yml/badge.svg)](https://github.com/msrovani/neural-os-core/actions)
[![Build](https://img.shields.io/badge/build-0%20errors-brightgreen?style=flat-square)](https://github.com/msrovani/neural-os-core/actions)
[![License](https://img.shields.io/badge/license-AGPLv3-blue?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-nightly_1.98-orange?style=flat-square)](https://www.rust-lang.org/)

---

## What this is

An experimental OS kernel and its application layer, organized as a Rust workspace
of six crates plus a boot binary:

```
k_nano (R0, foundation) ← k_hal (R1, hardware abstraction) ← cortex (R2, inference)
                                                          ← k_ai (R2, autonomy)
                                                          ← hermes (R3, orchestration) ← jarbas (R3, interface)
```

The core design decision is that **everything is an Agent or a Skill**: drivers,
daemons and services are all agents with explicit manifests (name, type, schedule,
trust tokens). Boot is an 8-phase event-driven sequence (SafeHarbor → MemoryCore →
SystemBringup → Diagnostics → HardwareDiscovery → DriverInit → AgentFleet →
Runtime) that publishes each phase on an in-kernel event bus.

The inference engine is a ternary (BitNet b1.58) language model — 850M parameters,
2-bit packed weights, ADD/SUB-only matmul, AVX2/SSE kernels — loaded from disk and
run inside the kernel address space, plus a small Mixture-of-Experts router
(Trinity MoE) with specialized experts (hardware identification, code generation,
security, speech).

## Status

The project is honest about what is done and what is not. Anything marked
"gated" or "pending" below will fail or is disabled at runtime — we prefer an
explicit gate over a silent promise.

### Working (validated in QEMU and/or real hardware)

| Area | State |
|---|---|
| UEFI boot (Limine) | Booting through all 8 phases in QEMU (WHPX/TCG); BIOS legacy boot is **not** supported |
| Memory management | AIOS self-adapting heap: modest 512MB floor, `grow_bump_auto` expands on demand (256MB steps) up to 75% of detected RAM; Limine kernel stack reserved in the frame allocator (fixes a `#PF ip=0` with large QEMU-loader models at 4GB+) |
| Networking | Intel e1000 driver (TX/RX, DMA fixed via uncached mapping), raw DNS, HTTP GET via smoltcp, NTP, TLS 1.3 (`embedded-tls`, wired through hermes), NetFs (TCP file server, smoke-tested) |
| P2P mesh | Two QEMU instances discover each other over UDP broadcast (port 42069), exchange skills; selective ACK per fragment, 16-slot reassembly, HMAC-SHA256/Ed25519 crypto tiers, token-bucket rate limiting |
| Storage | ATA PIO, FAT32 read/write (data partition), exFAT (opt-in) |
| WASM | `wasmi` `no_std` runtime with fuel metering, capability-gated host imports (`aios::*`); self-test `add(2,3)=5` passes |
| Audio/voice | Intel HDA capture + playback, Piper TTS (PT-BR/EN), CTC STT (55K params), wake word "Jarvis" |
| GPU compute | NVIDIA PUSH_BUFFER (validated on a GTX 1050), VirtIO-GPU 2D (QEMU), Intel GEN ring (canary) |
| SMP | 4-core AP wake (3 APs) via directed sequential SIPI, per-AP stacks |
| UI | `embedded-graphics` card desktop, Z-order compositor, PS/2 mouse, FFT-driven orb |
| Security | Ed25519 trust chain, capability gates, fail-closed mesh authentication |
| Self-installer (ADR-0079) | Detects PCI hardware, partitions, formats FAT32 ESP, deploys bootloader and only the needed firmware/models |
| Host tests | `cargo test --workspace` runs on the host (139 tests passing) |
| CI | GitHub Actions: `cargo check --release` (0 errors), host tests, boot smoke test in QEMU |

### In development / experimental

| Area | State |
|---|---|
| Ring 3 user-mode isolation | Implemented but **gated off** (`TRY_ENTER_RING3=false`): enabling it currently triple-faults the kernel. Work tracked in ADR-0060/0082 |
| Native JIT (Cranelift / Rust-subset) | App Factory paths B/C compile but are **gated** behind the isolation ring; only path A (wasmi sandbox) is active |
| On-device learning | AutoLearn (detect novel intent → train → register expert) exists but training convergence on ternary weights is unproven |
| Memory tiering (MHI) | Tier metadata + adaptive heap active; NVMe/VRAM migration deferred |
| Cross-OS ecosystem (MCP) | Research-stage; skills hub integration with other `no_std` projects is speculative |
| SleepCycle | Implemented (REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT); no measured effectiveness data yet |

### Not yet

| Area | State |
|---|---|
| WiFi | iwlwifi/ath10k drivers written, **awaiting real hardware** to validate |
| NVMe/AHCI | Drivers present, **awaiting hardware**; NVMe descriptor layouts flagged as suspicious vs spec |
| AMD GPU compute | Pending |
| UAC (USB audio class) | Parse only; isochronous transfer pending |
| v2.0.0 release gate | Not passed: requires formal ADR review + zero open backlog items + maintainer sign-off |

---

## Quick Start

### Prerequisites

- Rust **nightly 1.98** (pinned in `rust-toolchain.toml`, which is cross-platform —
  no env-var workaround needed on Linux/macOS)
- Python 3.10+ — only for the boot-image step
- QEMU 8+ — to run it

### Build

```bash
cargo build --release          # compiles the kernel and generates target/uefi.img
cargo check --release          # must be 0 errors (dead-code warnings are expected)
python3 tools/build_image.py   # optional: generates the FAT32 data disk (models, firmware)
```

### Run (QEMU)

```bash
# Linux/macOS (UEFI + TCG)
timeout 80 qemu-system-x86_64 -m 6G -smp 4 -accel tcg \
  -drive format=raw,file=target/uefi.img,if=ide,index=0 \
  -drive format=raw,file=target/disk_qemu.raw,if=ide,index=1 \
  -drive if=pflash,format=raw,file=/usr/share/ovmf/OVMF.fd,readonly=on \
  -serial file:logs/boot.txt -display none

# Windows (WHPX accelerator + window)
.\run-qemu-whpx.ps1 -Window
```

Boot is healthy when the serial log shows `[BOOT] Phase 6: AgentFleet`, the
scheduler reports runtime agents, and `[TIMER] tick=` keeps incrementing.

### Tests

```bash
cargo test --workspace --exclude neural-kernel --exclude boot
```

(`neural-kernel` and `boot` are the bare-metal binaries — they are compiled for
`x86_64-unknown-none` and never host-tested.)

### Real hardware

Generate a bootable USB image and flash with Rufus (DD mode):

```bash
cargo build --release
python3 tools/build_image.py --hw --unified   # target/usb_hw.img (ESP + FAT32 data)
```

Boots any x86_64 UEFI machine (Secure Boot off). See `HOWTO.md` for details.

---

## Architecture

### Rings (logical dependency layers — all code runs at CPL 0 today)

```
  RING 3 — APPLICATION                      hermes · jarbas
     Orchestration · WASM · Net · Skills · Display · Cards · Orb
  RING 2 — COGNITION                        cortex · k_ai
     BitNet LLM · Trinity MoE · Tensors · SelfHeal · Trust · Agency
  RING 1 — HARDWARE ABSTRACTION             k_hal
     DeviceCap · HalOffer · MMIO Backend · VirtIO · GPU backends
  RING 0 — FOUNDATION                       k_nano
     Memory · PCI · ACPI · APIC · SMP · IDT · DMA · ATA/NVMe/FAT32
     E1000 · RTL8139 · xHCI · PS/2 · HDA · Scheduler · W^X arena

  neural-kernel (bin) = crate integrator + boot wiring (minimal)
```

Note: these rings are an organizational dependency order, not a hardware
privilege boundary. Real isolation of untrusted code is the wasmi sandbox (active)
and Ring 3 user mode (gated, see Status).

### Boot sequence

```
  SafeHarbor → MemoryCore → SystemBringup → Diagnostics
        ↓
  HardwareDiscovery → DriverInit → AgentFleet → Runtime
  (PCI+ACPI+SMP)     (E1000+xHCI+ATA)  (~259 agents)  (Hermes+Cortex+Display)
```

### Design pillars

- Everything is an Agent — no tasks, no services, no standalone drivers
- Manifest-first — name, type, schedule, trust tokens explicit in code
- Activation on demand — idle continuous agents revert to event-driven
- Trust per (token, agent, skill)
- Rings R2/R3 never touch BAR MMIO directly — only via k_hal (HalOffer)
- Slim binary — logic lives in crates; `neural-kernel` is wire + `pub use`

---

## Working subsystems

### Networking

- e1000 driver with working DMA in both directions (the RX path was fixed by
  mapping DMA pages as uncacheable — see session notes)
- Raw DNS resolution and HTTP GET over smoltcp; periodic NTP
- TLS 1.3 via `embedded-tls` (ECDSA + RSA-PSS), exposed through `hermes::fetch_url`
- NetFs: a minimal TCP file server (LIST/READ/WRITE, smoke-tested end to end)
- P2P mesh: UDP broadcast transport in k_nano, heartbeats, master election,
  skill sync/apply between two kernels, crypto tiers (HMAC-SHA256 on trusted
  segments, Ed25519 for control/TOFU), selective ACK + 16-slot reassembly,
  exponential-backoff probing, JSON health dashboard

### Storage

ATA PIO (with the historic 16-bit read fix), FAT32 read/write, optional exFAT,
NVMe/AHCI drivers (hardware-pending), GPT protective handling so the ESP is never
reformatted. The boot image builder (`tools/build_image.py`, stdlib-only Python)
generates the FAT32 data disk with models, firmware blobs and config.

### WASM runtime

`wasmi` compiled `no_std`, run with fuel metering (anti-DoS), every host import
(`aios::*`) capability-gated. Skills can be generated by the LLM, compiled to
WASM and persisted. A W^X exec arena demonstrates native JIT (Ring 0); Cranelift
and Rust-subset paths exist but are gated behind the isolation ring.

### Inference & cognition

- **BitNet b1.58** — 850M ternary params (`BITNET2B.BIN`, ~590MB, loaded via
  QEMU loader or FAT32), 30 layers, hidden 2560, GQA, Medusa speculative decode,
  AVX2/SSE4.2 kernels with runtime dispatch
- **Trinity MoE** — router + 6 experts (hardware ID, code generation, disk
  diagnostics, security, speech, text completion); router weights trained offline,
  on-device AutoLearn training experimental
- **HW Expert v3** — 61,453 PCI/USB VID/DID pairs recognized by a 259KB ternary
  network replacing a 40MB `pci.ids` table
- **SleepCycle** — scheduled REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT cycle over
  episodic memories (no effectiveness benchmark yet)

### Audio & voice

Intel HDA capture + playback (CORB/RIRB, SD0/SD1), Piper VITS TTS (PT-BR + EN),
CTC STT (PCM→MFCC→2×LSTM→CTC), energy-gated "Jarvis" wake word, 16-bin Goertzel
FFT driving the orb visualization.

### GPU compute

NVIDIA PUSH_BUFFER/GPFIFO (validated HW-real on Pascal), VRAM buddy allocator,
Intel GEN ring (canary), VirtIO-GPU for QEMU, GPU work queues with CPU fallback.

### Security & safety

Ed25519 trust chain (TOFU in mesh), per-agent trust tokens, capability gates,
fail-closed packet authentication, safety invariants (I1–I4) with halt-on-violation,
self-healing firmware pipeline (missing blob → diagnose → download → hot-load).

---

## Agents

Everything in the system is an agent. 25 native agents are wired in the kernel:

| Code | Agent | Type | Schedule | Function |
|------|-------|------|----------|----------|
| A-001 | SystemAgent | System | Oneshot | Init, SYSTEM_READY, EchoSkill |
| A-002 | MonitorAgent | System | Oneshot | Publishes SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | Continuous | Scancode IRQ bridge |
| A-004 | NetAgent | Network | Continuous | smoltcp poll + HTTP |
| A-005 | InputAgent | Console | Continuous | Keyboard (PS/2 + USB xHCI) |
| A-006 | CortexAgent | Inference | Continuous | LLM + Medusa + Trinity MoE |
| A-007 | HermesAgent | Router | Continuous | Intent routing + ReAct + skills |
| A-008 | DisplayAgent | Console | Continuous | Framebuffer + compositor |
| A-009 | NetDriverAgent | Driver | Oneshot | RTL8139 + E1000 + i225 |
| A-010 | UsbDriverAgent | Driver | Oneshot | xHCI port scan + USB MSC |
| A-011 | BootSelfHealAgent | System | Oneshot | SelfHeal init |
| A-012 | BootTrustAgent | System | Oneshot | TrustCache init + Ed25519 |
| A-013 | PlatformAgent | System | Oneshot | PCI + ACPI + APIC + SMP |
| A-014 | MemoryAgent | System | Oneshot | MHI + adaptive heap |
| A-015 | GpuDriverAgent | Driver | Oneshot | GPU backend detect |
| A-016 | HwDetectAgent | System | Oneshot | HW identification |
| A-017 | CronAgent | System | Continuous | Cron scheduler |
| A-018 | SecurityAgent | System | Continuous | 5 detectors + pipeline |
| A-019 | SafetyAgent | System | Continuous | 4 invariants I1–I4 |
| A-020 | OptimizerAgent | System | Continuous | Self-optimization |
| A-021 | SleepCycleAgent | System | PollEvery(1000) | REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT |
| A-022 | AutoLearnAgent | System | PollEvery(200) | Detect need → train → register expert |
| A-023 | WifiAgent | Network | Continuous | 802.11 scan + WPA2 (HW-pending) |
| A-024 | WakeWordAgent | System | EventDriven | "Jarvis" keyword by energy |
| A-025 | HdaAudioAgent | Driver | Oneshot | HDA audio capture + playback |

Plus ~147 event-driven specialists (hardware, filesystem, network, security,
application) registered at boot — about 259 agents total at runtime.

---

## Crate map

| Crate | Ring | Role |
|-------|:----:|------|
| [`k_nano`](crates/k_nano/) | R0 | Foundation: memory, PCI, ACPI, APIC, SMP, IDT, DMA, storage, NICs, audio, filesystems, scheduler |
| [`k_hal`](crates/k_hal/) | R1 | DeviceCap, HalOffer, MMIO backend, VirtIO, GPU backends |
| [`cortex`](crates/cortex/) | R2 | BitNet LLM, Trinity MoE, tensor engine, BPE, compute dispatch |
| [`k_ai`](crates/k_ai/) | R2 | SelfHeal, Trust, SGDB, inventory, audit, Agency |
| [`hermes`](crates/hermes/) | R3 | Orchestration, WASM runtime, network, skills, app factory |
| [`jarbas`](crates/jarbas/) | R3 | Display front-end, compositor, cards, persona, voice |
| `neural-kernel` | — | Boot binary: crate integration + wiring |

---

## Roadmap

| Milestone | Status |
|-----------|--------|
| v1.0.0 Code Freeze (ATA fix, FAT32, NVMe, UEFI, zero errors) | Done |
| v1.5.0 K³CHJ workspace (6 crates, wiring) | Done |
| v1.8.0 Adequação (ADR-0041/0042 capability PoC) | Done |
| v1.9.x Pós-LAN (NetFs, HTTP, NTP, self-update, residual waves) | Done |
| Voice pipeline (Piper TTS, STT, wake word, HDA) | Done |
| ADR-0057/0058/0059/0079/0081 (compute, cards, app factory, installer, mesh) | Done |
| Ring 3 isolation (ADR-0060/0082) | Gated — triple-fault to fix |
| WiFi/TLS hardware validation | Pending hardware |
| v2.0.0 gate (formal review + zero backlog + maintainer OK) | Not passed |

---

## Testing & CI

- `cargo check --release` — 0 errors required (dead-code warnings are the
  project's Known Warnings policy)
- `cargo test --workspace --exclude neural-kernel --exclude boot` — host unit
  tests (139 passing)
- QEMU boot smoke test (UEFI + TCG) — validates the full 8-phase boot to the
  runtime tick loop
- CI (GitHub Actions) runs all three on every push and PR

---

## Documentation

| Document | What |
|----------|------|
| [`AGENTS.md`](AGENTS.md) | Agent ontology, boot sequence, operational rules |
| [`HOWTO.md`](HOWTO.md) | Build + run instructions for all environments |
| [`TECNOLOGIAS.md`](TECNOLOGIAS.md) | Technology catalog (300+ entries) |
| [`docs/architecture/INDEX.md`](docs/architecture/INDEX.md) | 47+ ADRs with lifecycle tracking |
| [`docs/memory/STATE.md`](docs/memory/STATE.md) | Current kernel state |
| [`docs/memory/SESSION_INDEX.md`](docs/memory/SESSION_INDEX.md) | Session log index + lessons learned |
| [`ROADMAP.md`](ROADMAP.md) | Full roadmap v1.0 → v2.0 |
| [`CHANGELOG.md`](CHANGELOG.md) | Version history |

Key ADRs: [0041 capability rings](docs/architecture/0041-k2chj-capability-rings.md),
[0042 K³CHJ adequacy](docs/architecture/0042-*.md),
[0057 compute dispatch](docs/architecture/0057-compute-dispatch.md),
[0058 card desktop](docs/architecture/0058-generative-card-desktop.md),
[0059 app factory](docs/architecture/0059-runtime-app-factory.md),
[0079 self-installer](docs/architecture/0079-neural-auto-installer.md),
[0081 P2P mesh](docs/architecture/0081-malha-cognitiva-distribuida-p2p.md).

Datasets (HW IDs, PCI/USB IDs, firmware metadata) are published on
[HuggingFace](https://huggingface.co/aios-k2chj).

---

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Contributions are accepted under the
AGPL-3.0 license with a Developer Certificate of Origin (DCO) sign-off on
commits.

License: [AGPL-3.0](LICENSE). For commercial licensing, contact
`licensing@neural-os.io`.

---

*Copyright © 2026 Marcelo Scapin Rovani — AGPLv3 License*
