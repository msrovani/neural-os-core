# Neural OS Hermes v0.65.0 - AI-native Bare-metal Operating System

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
28/06/2026 — Boot em hardware real (SDHC USB)
29/06/2026 — Framebuffer UEFI 1280x720 + bootloader 0.11
30/06/2026 — Desktop com Compositor + Dock + 3 Apps
30/06/2026 — 107 arquivos Rust, ~11.800 LOC, 0 erros
```

```
"We don't need an OS that runs AI. We need an OS that IS AI."
```

---

## Architecture: Everything is an Agent or a Skill

**173+ agents.** No tasks, no services, no drivers — only agents with manifests, capabilities, and lifecycle.

```
20 native agents  +  147 The Agency specialists  +  6 HW agents  +  6 FS agents
```

| Agent | Type | Function |
|---|---|---|
| HermesAgent | Router | Intent routing, ReAct 7 phases, Council, Handoff |
| CortexAgent | Inference | LLM transformer (BitNet + PTRM + Medusa), Model trait |
| DisplayAgent | Console | Framebuffer 1280x720 + DoubleBuffer + Compositor |
| MouseAgent | Console | PS/2 IRQ12, MOUSE_MOVED/CLICK no EventBus |
| BrowserAgent | Skill | fetch_page, extract_text, PageViewerApp |
| SafetyAgent | System | Hard blocklist, Asimov 4 Laws, check_command() |
| CronAgent | System | Cron jobs, heartbeat, MHI auto-tier |
| + 147 specialist agents across 12 divisions | | |

---

## Desktop Environment (COSMIC-inspired)

```
+----------------------------------------------------------+
| [Hermes] [Settings] [Power]                    [14:32]   |
|                    Dock Bar                               |
+----------------------------------------------------------+
|                                                          |
|  3 Workspaces: main, dev, chat                           |
|  Layout modes: Floating, Tiled, Grid, Maximized          |
|  Notification overlay (severity: info/warn/error)         |
|  Mouse drag, Window focus, Auto-tiling                    |
|  PageViewer for web content                               |
|                                                          |
+----------------------------------------------------------+
```

---

## Filesystem: 8 VFS Mounts + MHI ARC Tiering

```
/          root tree        /mnt/hdd/   ATA disk
/chat/     Hermes chat      /mnt/ram/   DRAM cache
/dev/      PCI + devices    /mnt/sdhc/  USB Mass Storage
/proc/     agents + memory  /inference/ LLM-generated files
/system/   config + skills
```

**MHI ARC Tiering** (ZFS-inspired): hot data promove para DRAM, cold demove para HDD. Movido por `arc_suggest_tier()` + `MhiScheduler` a cada 1000 ticks.

---

## Cognitive Memory Stack

```
EventBus IPC
  -> MemoryTree (hierarchic, TTL, Ebbinghaus decay)
  -> KnowledgeGraph (subject-predicate-object)
  -> Kanerva Machine (sparse distributed, Hamming distance)
  -> Atkinson-Shiffrin (Sensory -> STM -> LTM)
  -> Curated Context (4KB budget, Anatomy-style)
```

---

## LLM Cortex: Swappable Model Engine

```
pub trait Model: Send {
    fn generate(&self, prompt: &str) -> String;
    fn embed_dim(&self) -> usize;
    fn vocab_size(&self) -> u16;
    fn max_seq(&self) -> usize;
}
```

| Engine | Status | Params | Speed |
|---|---|---|---|
| BitNet (ternary) | Active | 272K | 5-15 tok/s |
| PTRM (probabilistic) | Active | 7M | ~5 tok/s |
| GGUF Qwen3.5 | Planned | 9B | Blocked (heap) |

PTRM adds: Gaussian noise injection, Q-head confidence, 3 parallel trajectories.

---

## Self-Healing Kernel

```
[PANIC] -> FailureClass::classify() -> SelfHeal::analyze() -> RecoveryAction
  -> restart_daemon | create_skill | log_and_continue
  -> KERNEL_ERROR no EventBus -> LLM sugere correcao
  -> Se falhar: lessons.push() -> estrategia ALTERNATIVA
```

---

## Voice + Web + Tools

| Skill | Function |
|---|---|
| `speak(text, profile)` | Hermes fala (8 preset voices) |
| `fetch(url)` | Baixa pagina web, extrai texto |
| `search(query)` | Busca semantica (Exa-style) |
| `verify_skill(code)` | eBPF-style verifier para skills |
| `ranked_query(text)` | Gbrain reranker no KnowledgeGraph |

---

## Project Stats (v0.65.0)

| Metric | Value |
|---|---|
| Rust files | 107 |
| Total LOC | ~11,800 |
| Crates | 5 (neural-kernel, agent-core, event-bus, skill-registry, ticket-lock) |
| Agents | 173+ (20 native + 147 The Agency + 6 HW + 6 FS) |
| VFS mounts | 8 |
| Workspaces | 3 (main, dev, chat) |
| Tema | 5 (hermes-dark, dracula, matrix, solarized, hermes-light) |
| Apps | 3 (Hermes, Settings, Power) |
| Compile | 0 errors, cargo check --release |

---

## Quick Start

```powershell
# Build
cargo build --release
python tools/build_image.py --bios

# Run (QEMU with WHPX acceleration)
qemu-system-x86_64 -m 4G -nic user,model=rtl8139 `
  -drive format=raw,file=target/neural-os-bios.img `
  -no-reboot -smp 4 -accel whpx

# With serial log
qemu-system-x86_64 -m 4G -serial stdio -nic user,model=rtl8139 `
  -drive format=raw,file=target/neural-os-bios.img `
  -no-reboot -smp 2 -nographic -accel tcg

# Boot from SDHC (Rufus: DD image, MBR, BIOS/CSM)
```

---

## License

MIT

**106+ sprints, 22+ blocos. De um bootloader a um SO cognitivo com desktop, agents, LLM e memoria associativa — em 10 dias.**
