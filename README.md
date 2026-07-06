# Neural OS Hermes v0.93.0 — AI-native Bare-metal Operating System 🏆

**The first AI-native operating system in the world. Bare-metal Rust. No Linux. No POSIX. No legacy. 0 errors.**

```
╔══════════════════════════════════╗
║  J.A.R.V.I.S. vNeural OS v0.93 ║
║  "Thoughtful. Precise. Alive."  ║
╚══════════════════════════════════╝

  ✦ JARVIS Desktop: multi-window, status bar, F1-F4 app switcher
  ✦ BitNet IDE (F4): gera WASM skills → ícones dinâmicos no desktop
  ✦ Avatar com partículas + 4 estados (Idle/Listening/Processing/Speaking)
  ✦ Análise emocional (7 emoções + sarcasmo)
  ✦ Personalidade adaptativa (SOUL.md + Fluid Persona)
  ✦ Memória contextual (256 interações + Atkinson-Shiffrin 3-tier)
  ✦ SleepCycle (5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT)
  ✦ Ego Layer (confidence tracking por domínio)
  ✦ Proactive Heartbeats (alertas de disk/mem/net)
  ✦ Auto-Skill Generation (observa padrões, gera skills)
  ✦ Babel-Index (monitor de entropia da memória)
  ✦ Fail-Closed Safety (4 invariantes SMT-proof)
  ✦ Merkle Audit Trail (SHA-256 chain, ring 4096)
  ✦ AHCI driver (SATA 6G NCQ)
  ✦ GPU Foundations: BAR mapping, SPSC job ring, VRAM buddy allocator
  ✦ WASM Runtime: MemoryPool (256KB/skill), 15 WASI→Skill mappings
  ✦ Voz: Kokoro-82M TTS (ferramenta de conversão ONNX→.bitnet pronta)
  ✦ Embedding: BGE-Small-EN-v1.5 (384-dim, convertido para .bitnet)

  → Port do JARVIS .NET MAUI para bare-metal Rust
  → github.com/msrovani/jarvis (app original)
```

## Status Atual

| Métrica | Valor |
|---------|-------|
| Versão | v0.93.0-wasm |
| Sprints completos | **93** (1-93) |
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

**Plano completo:** `docs/sprint-plan-92-99.md`

---

```
J.A.R.V.I.S. — Just A Rather Very Intelligent System
"Thoughtful. Precise. Alive."
```
