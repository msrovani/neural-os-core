# Neural OS Hermes v1.2.0 — AI-native Bare-metal Operating System 🏆

**The first AI-native operating system in the world. Bare-metal Rust. No Linux. No POSIX. No legacy. 0 errors.**

```
╔══════════════════════════════════╗
║  Neural OS Hermes v1.2.0        ║
║  "ATA Liberation"               ║
╚══════════════════════════════════╝

  ✦ v1.2.0 — 0 erros, ~26.000 LOC
  ✦ ATA PIO bug CORRIGIDO — disco finalmente lê!
  ✦ HW Expert v3 (61.453 VID/DID, 1M params)
  ✦ SelfHealing Firmware Pipeline (I3/I4)
  ✦ WiFi Intel AX200/AX210 com ucode loading real
  ✦ 3 camadas visuais: Orb + Hermes CLI + Window Manager
  ✦ HDA capture + playback (microfone + auto-falante)
  ✦ BrowserAgent real: HTTP GET via smoltcp TCP
  ✦ FFT audio → animação do Orbe em tempo real
  ✦ Zero-Trust Syscall Categories (ReadOnly/Ephemeral/Persistent/Hardware)
  ✦ GGUF streaming (>4GB models via chunky FAT32 read)
  ✦ RssAgent + EmailAgent (WWW agents via HTTP + SMTP)
  ✦ Serial bridge com watchdog + DNS healthcheck
  ✦ Frame allocator 8GB (bitmap estendido)
  ✦ GPU compute: Intel ring, NVIDIA PFIFO, AMD PM4, VirtIO-GPU
  ✦ WASM Runtime: bytecode VM, PluginHub, SkillMarket, fuel metering
  ✦ JARVIS: sleep cycle, ego layer, emoção, memória episódica
  ✦ SkillOpt + Structured Decoding (compressed FSM)
  ✦ Self-Update A/B slots + rollback
  ✦ Embedding: BGE-Small-EN-v1.5 (384-dim, convertido para .bitnet)

  → Port do JARVIS .NET MAUI para bare-metal Rust
  → github.com/msrovani/jarvis (app original)
```

## Status Atual

| Métrica | Valor |
|---------|-------|
| Versão | v0.97.0-rustcoder |
| Sprints completos | **97** (1-97) |
| Arquivos Rust | ~200 |
| LOC total | ~42.000 |
| Agentes | 248+ |
| IDEIAS no Banco | **408** (#1-#408) |
| Erros de compilação | **0** (warnings: dead code policy) |
| GPU suportadas | NVIDIA, AMD, Intel (30+ modelos) |
| Kernel boot | QEMU, VirtualBox, HW real (Intel 6xx) |

## O que torna único

### JARVIS Desktop com IDE e Ícones WASM
DisplayAgent renderiza o Compositor diretamente — sem X11, sem Wayland:
- **F1**: Hermes Chat (comandos, histórico)
- **F2**: Settings (tema, voz, memória, avatar)
- **F3**: Power (shutdown, reboot, hibernate)
- **F4**: **BitNet IDE** — digite `[GEN]` para criar WASM skills → viram ícones no desktop
- Avatar JARVIS pulsante no canto inferior direito

### WASM Runtime + Hybrid Agents
- `wasm_rt.rs`: WASM Skill Runtime com MemoryPool de 256 KB por skill
- 15 WASI→Skill mappings (fd_read→FileAgent, poll_oneoff→EventBusAgent, etc)
- `HybridRegistry`: agentes kernel + WASM + MCP externo

### GPU Compute Bare-metal (Sprint 84-85)
- BAR0/BAR1 mapping UC para NVIDIA/AMD/Intel
- SPSC job ring com doorbell por vendor
- VRAM buddy allocator power-of-2 (4KB-4GB)
- Secure Boot GPU: ACR (NVIDIA), PSP (AMD), GuC (Intel)

### JARVIS Cognitive Engine
- SOUL.md Personality + Fluid Persona (3 modos: Coach/Tutor/Tool)
- Emotion Analysis (7 emoções + sarcasmo)
- Session Compression (4 estratégias) + Notification Gate (4 urgências)
- Dreaming/Consolidation (insights sintéticos)
- Ego Layer (confidence tracking, `can_answer()`)
- Auto-Skill Generation (≥3 repetições → skill)
- Babel-Index (monitor de entropia da memória)

### Memory Systems (Sprint 89)
- SHA-256 Dedup + Privacy Filter + TTL Eviction
- Ebbinghaus Decay + BM25 Hybrid Search
- 4-Tier Consolidation (Working→Episodic→Semantic→Procedural)
- Metacognitive Guard + Draft→Review→Merge
- Atkinson-Shiffrin 3-tier (Sensory→STM→LTM)
- Bi-temporal Knowledge Graph
- BGE-Small-EN-v1.5 embedding (384-dim, convertido ONNX→.bitnet)

### Boot Agent-First (8 fases)
O LLM carrega ANTES do PCI scan. Cortex acompanha o hardware sendo descoberto.

### Self-Healing com LLM
Panics são classificados, analisados e recuperados com ação sugerida pelo LLM.

---

## Roadmap Resumido

| Sprint | Bloco | Status |
|--------|-------|--------|
| 84-85 | GPU Foundations + Decode | ✅ |
| 86 | JARVIS Persona | ✅ |
| 87 | Security + AHCI | ✅ |
| 88 | Emotion + Cache | ✅ |
| 89 | Memory + BGE | ✅ |
| 90 | Deep Cognitive | ✅ |
| 91 | Desktop UI + Polimento | ✅ |
| 92 | LAN + Dependências | ✅ |
| 93 | WASM Runtime + IDE | ✅ |
| **94+** | Voice, Vision, NPU, AIOS | 🟡 |
| **97** | RustCoder Expert + Trinity MoE | ✅ |

**Plano completo:** `docs/sprint-plan-92-99.md`

---

```
J.A.R.V.I.S. — Just A Rather Very Intelligent System
"Thoughtful. Precise. Alive."
```
