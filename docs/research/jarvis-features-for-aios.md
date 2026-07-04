# J.A.R.V.I.S. para o Neural AIOS — Pesquisa e Aderência

**Data:** 2026-07-03
**Fontes:** 11 projetos (GitHub, crates.io, docs, artigos, fóruns)

---

## O que já temos (e a pesquisa validou)

| Nossa feature | Validado por | Score |
|---|---|---|
| **Offline-first / privacy-by-design** | Priler/jarvis, OpenJarvis (Stanford) | 🟢 10/10 |
| **SafetyAgent (Asimov Laws)** | TabNews artigo, KitsuneSemCalda | 🟢 10/10 |
| **Agent/Skill specialization (173 agents)** | SynkraAI (12 agents), OpenJarvis (8 agents) | 🟢 10/10 |
| **CLI First (Hermes Chat Console)** | SynkraAI CLI First, terminal-jarvis | 🟢 9/10 |
| **WASM sandbox (ADR-0032)** | OpenJarvis + TabNews (LuaJIT) | 🟢 9/10 |
| **Model orchestration (Trinity MoE)** | TabNews "small > one big", OpenJarvis router | 🟢 9/10 |
| **Event-driven cognitive cycle** | OpenJarvis learning loop | 🟢 9/10 |
| **Capability tokens / Trust** | terminal-jarvis `command_on_path` | 🟢 8/10 |

## O que ADOTAR para o J.A.R.V.I.S.

| O que | Fonte | Viabilidade | Esforço |
|---|---|---|---|
| **IPW — Intelligence Per Watt** | OpenJarvis (Stanford) | 10/10 | ~100 LOC no MemoryAgent |
| **Session compression** | OpenJarvis | 8/10 | ~200 LOC no Hermes buffer |
| **Skill discovery optimization** | OpenJarvis (DSPy/ACE) | 9/10 | ~300 LOC no SkillRegistry |
| **RAPL/ACPI energy telemetry** | OpenJarvis | 8/10 | ~150 LOC na telemetria |
| **CLI Harness capability contract** | terminal-jarvis | 8/10 | ~100 LOC no SkillRegistry |
| **Plugin IPC bridge (MCP)** | dyoburon-jarvis | 7/10 | Já temos MCP Layer |
| **Vosk/Whisper local STT** | Priler/jarvis | 8/10 | Futuro (pós B-01) |

## O que NÃO ADOTAR

| Ideia | Fonte | Motivo |
|---|---|---|
| Tauri desktop UI | Priler | Somos bare-metal, não desktop app |
| Multiplayer presence | dyoburon | Irrelevante para OS single-user |
| n8n/OpenClaw (Node.js) | Medium article | Ecossistema Node, não cabe no kernel |
| Python backend | OpenJarvis | Contradiz nosso Rust no_std |
| LuaJIT | TabNews | WASM é melhor sandbox |
| YOLO capability | terminal-jarvis | Perigoso — viola SafetyAgent |

## Stack proposta para o J.A.R.V.I.S. do Hermes

```
User
  ↕ (teclado / futuro voz)
J.A.R.V.I.S. (1100 LOC)
  ├── SOUL.md personalidade
  ├── Session compression (conversas longas)
  ├── IPW monitoring (eficiencia energetica)
  ├── Skill discovery (sugere novas skills)
  └── Notification Gate (alertas proativos)
  ↕
Hermes (intent routing + ReAct)
  ↕
Cortex (BitNet 1.5B via GGUF)
  ↕
Kernel (173 agents + skills + hardware)
```
