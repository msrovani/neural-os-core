# Repository Atlas: neural-os-core (K³CHJ Core)

## Project Responsibility
AI-native bare-metal OS written in Rust (`no_std` + `no_main`, x86_64). Everything is an Agent or a Skill — no tasks, no services, no standalone drivers. Hardware rings: Ring 0 (NPU — intent routing), Ring 1 (GPU — tensor), Ring 2 (CPU — agents/skills). ~26.000 LOC, 180+ files, ~50 native agents. Boot via Limine UEFI (bootloader 0.11.15), 8-phase event-driven sequence (SafeHarbor → MemoryCore → SystemBringup → Diagnostics → HardwareDiscovery → DriverInit → AgentFleet → Runtime). Current line: v1.9.99-s297 TEST, K³CHJ = k-nano + k-hal + k-ai + Cortex + Hermes + Jarbas. 168 host tests passing.

## System Entry Points
- `crates/neural-kernel/` — boot binary: Limine `_start` → `kernel_boot()` (limine_boot.rs), 8-phase boot, agent fleet, K³CHJ wiring via `pub use` re-exports.
- `crates/boot/` — image builder (bindeps + mk_esp_fat.py → `target/uefi.img`).
- `crates/k_nano/` — Ring 0 foundation: memory, IRQ/IDT, PCI, SMP, drivers (RTL8139/E1000/VirtIO/xHCI/ATA/AHCI/NVMe), FS (FAT32/exFAT/ext2/NTFS/Btrfs/NeuralFS), VFS, scheduler, sync, canonical statics (GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET, EVENT_BUS, SKILL_REGISTRY, nic_globals).
- `tools/build_image.py` — disk image generation for QEMU and HW USB (unified GPT image).

## Directory Map (Aggregated)
| Directory | Responsibility Summary | Detailed Map |
|-----------|------------------------|--------------|
| `crates/k_nano/` | Ring 0 hardware foundation — the crate every product crate depends on. Owns CPU/platform bring-up (bitmap frame allocator, 512MB heap, IDT/GDT/TSS/APIC/ACPI, PCI, SMP directed wake), all device drivers (NICs with UC-mapped DMA rings, xHCI, storage, virtio-blk), filesystems + VFS, scheduler/sync primitives, async runtime, MHI tiering, P2P mesh transport, and the canonical cross-crate singletons (GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET, EVENT_BUS, SKILL_REGISTRY, nic_globals, smp::ap_entry). | [View Map](crates/k_nano/codemap.md) |
| `crates/k_hal/` | Ring 1 HAL between R0 silicon and R3 agents (ADR-0041 §9). DeviceCap/DeviceTree discovery, per-class HalOffer bind with EventBus topics, ring-gated HalCap (`cap_gate`), DeviceRecipe trust/FW gate (ADR-0056), MMIO backends: GPU (detect → display_coex plan → canary), Intel HDA audio (SD0/SD1 DMA), WiFi (ath10k CE/BMI/WMI, iwlwifi ucode, generic engine). VirtIO transport only. Depends on `k_nano::memory::` — never `crate::memory::`. | [View Map](crates/k_hal/codemap.md) |
| `crates/k_ai/` | Ring 2 autonomy: SelfHeal (checkpoint save/restore, VID-gated firmware/skill scan, silent-failure detection), TrustCache ((token, agent, skill) triples), Agency/AgentSpec catalog + embedded SKILL.md seed agents, hardware inventory, on-device ternary fine-tuning + federated gradients, SGDB cognitive path DB (HANR/audit/pkg/skills/episodic/RAG over ART+BQ+MemoryDoc). | [View Map](crates/k_ai/codemap.md) |
| `crates/cortex/` | Ring 2 intelligence: BitNet LLM (ternary −1/0/+1, 2-bit packed, ADD/SUB matmul, KV-cached GQA + RoPE + FlashAttention), Trinity MoE (trainable ternary router + experts, R3 arena replay), compute dispatch ladder (NPU/GPU/SMP-AP/AVX-512/AVX2/SSE/scalar, FeatureGate-gated) + mesh P2P, speculative + grammar-constrained decoding, .bitnet v1–v5 loader/saver, ModelHub 8-slot, GGUF dequant + FAT streaming. | [View Map](crates/cortex/codemap.md) |
| `crates/hermes/` | Ring 3 orchestration: intent routing + ReAct 7-phase loop → typed Commands → skills or Cortex LLM. WASM runtime (ADR-0059): app_factory A/B/C (wasmi sandbox default with CapGate + PermissionGate + fuel; Cranelift JIT and native Rust-subset gated by isolation ring ADR-0060), wasm_build from op-IR. Skills ecosystem (SKILL.md loaders/generators/observers, mesh skill sync, package_hub). Function-pointer bridges (net_bridge, VfsBridge) into kernel NETSTACK/VFS; NeuralFS CoW FS; zero-ambient-authority membrane + HITL gates. | [View Map](crates/hermes/codemap.md) |
| `crates/jarbas/` | Ring 3 user-facing: BGRA32 framebuffer (DoubleBuffer) + layered compositor (JarbasDesktop, 4-level Z-order, tiling/floating WM), declarative UI cards (ADR-0058: UiDeclaration/Widget → FbTarget embedded-graphics, CardWindow retention from UI_SPEC JSON), JARVIS persona (SoulProfile/Emotion, DisplayAgent), full voice pipeline (VAD → wake-word → CTC STT → USER_INTENT; HERMES_RESPONSE → Piper/formant TTS → mixer), GPU FE re-export of k_hal::gpu + cube demo. | [View Map](crates/jarbas/codemap.md) |
| `crates/agent-core/` | no_std agent model: Agent trait, AgentInstance/AgentRegistry, cooperative scheduler (init_phase + run, affinity rings R0→R1→R2, goal-aware ordering), budget watchdog, tick hooks, crews (CrewAI-style), StateGraph (LangGraph-style). All agent-hosting crates depend on it. | [View Map](crates/agent-core/codemap.md) |
| `crates/boot/` | Workspace default-member image builder: build.rs pulls kernel ELF via bindeps, assembles Limine ESP tree (kernel.elf + BOOTX64.EFI + limine.conf), runs mk_esp_fat.py → `target/uefi.img`; rerun-if-changed guards stale images. | [View Map](crates/boot/codemap.md) |
| `crates/event-bus/` | no_std pub/sub EventBus (topic → fan-out queues) + MessageBus mailboxes + BoundedChannel, gated by CapabilityToken (u64 or Ed25519). LatentBus for [f16;256] hidden states, FNV-1a DedupWindow. Canonical topics: BOOT_PHASE, P2P_PACKET, HEALTH_ISSUE, AUDIO_IN, CARD_ACTION, THOUGHT_LLM. | [View Map](crates/event-bus/codemap.md) |
| `crates/skill-registry/` | no_std Skill model: Skill trait + McpManifest, registry with ToolPolicy, capability-token auth, CompletionContracts, OutputCache, SkillIndex/McpCatalog, JobPreconditions, DynamicSkill (LLM-generated, optional WASM), FanOutPool. Singleton lives in k_nano (SKILL_REGISTRY); list_skills() → Vec<(String, ToolPolicy)>. | [View Map](crates/skill-registry/codemap.md) |
| `crates/ticket-lock/` | Single-file no_std FIFO ticket spinlock (ticket/serving AtomicUsize, spin_loop, const fn new, guard Drop handoff). Fair, dependency-free; used by event-bus and elsewhere. | [View Map](crates/ticket-lock/codemap.md) |
| `tools/` | ~90 host-side Python scripts: boot/disk image generation (build_image.py, build_usb_unified.py, mkfat32/exfat), HF→.bitnet conversion + PyTorch training (STT CTC, wake-word MLP, HW Expert v3/v4), linux-firmware download + GSP pinning, SDIO/DriverStore HWID extraction, dev bridges (serial_bridge, netfs), preflight_wave validation. | [View Map](tools/codemap.md) |
| `tools/limine/` | Limine UEFI assets: mk_esp_fat.py (FAT32-only ESP ≥65525 clusters, LFN), limine.conf, vendor BOOTX64.EFI binaries, legacy esp/ reference tree. Consumed by crates/boot build.rs. | [View Map](tools/limine/codemap.md) |

## Dependency Chain (Rings)
```
k_nano (R0) ← k_hal (R1) ← cortex (R2) + k_ai (R2) ← hermes (R3) ← jarbas (R3)
                 └───────────────────────────────────────────┘
neural-kernel (bin) = integration + residuals, re-exports statics via `pub use`
```

## State Tracking
Change detection state lives in `.slim/codemap.json`. To refresh after edits:
```bash
node ~/.config/opencode/skills/codemap/scripts/codemap.mjs changes --root ./
node ~/.config/opencode/skills/codemap/scripts/codemap.mjs update --root ./
```
