# Roadmap — neural-os-core v2.0 🏆

**Última atualização:** 2026-07-14  
**Estado:** ~26.000 LOC, 180+ arquivos Rust, 247+ agentes, 0 erros — **v2.0.0**  
**Objetivo:** Ecossistema de anéis lógicos isolados (k_nano, k_ai, cortex, hermes, jarbas) — Sprint 106 concluída

---

## ✅ Sprints 1-105 — Completos (v1.0 → v1.5.3)

| Bloco | Sprints | v | Foco | Status |
|-------|---------|---|------|--------|
| 1-15. Foundation | 1-57 | 0.1-0.57 | Kernel, PCI, Rede, Transformer, Self-Heal, Agents | ✅ |
| 16. HW Real + USB | 58 | 0.58 | Boot HW real, xHCI HID, FAT12, ATA | ✅ |
| 17. Bootloader 0.11 | 59 | 0.59 | Framebuffer UEFI 1280×720 | ✅ |
| 18. Security | 74 | 0.74.x | TPM TIS, Ed25519, Partition mask 0x1C | ✅ |
| 19. Disk Intelligence | 75 | 0.75.x | NVMe, SMART, ARC cache, GPT, FAT32 | ✅ |
| 20. Memory + Tick | 76 | 0.76.x | Adaptive heap, Dynamic tick, Hermes event-driven | ✅ |
| 21. Foundation Quick Wins | 77 | 0.77.x | Prompt >, Pre-Flight, FanOut, TaskSchema | ✅ |
| 22. Agentic Evolution | 78 | 0.78.x | Crew/Flow, Cache, Workflow, GGUF, WASM | ✅ |
| 23. LLM Infrastructure | 79-80 | 0.79-0.80 | AVX2 BitNet, Trinity MoE, BPE, KV Cache | ✅ |
| 21a. SMP Foundation | 81 | 0.81.x | SPSC ring, IPI, PerCpu, x2APIC | ✅ |
| 21b. Work-Stealing + Matmul | 82 | 0.82.x | Chase-Lev, parallel-for, CFS | ✅ |
| 21e. Polimento | 83 | 0.83.x | CFS, bugfixes, integração | ✅ |
| 21c. GPU Foundations | 84 | 0.84.x | BAR UC, SPSC ring, VRAM buddy, secure boot | ✅ |
| 21d. GPU Decode | 85 | 0.85.x | Prefill/decode split, KV DMA, XQueue | ✅ |
| 30. JARVIS Persona | 86 | 0.86.x | SOUL.md, IPW, Compression, Notification, SlabBuddy | ✅ |
| 31. JARVIS Security | 87 | 0.87.x | I1-I4 invariants, Merkle Audit, Fluid Persona, AHCI | ✅ |
| 32. JARVIS Emotion | 88 | 0.88.x | ADE, Pipeline 16 stages, Emotion Analysis, edge-dhcp | ✅ |
| 33. SleepCycle + Memory | 89 | 0.89.x | 5 fases, KG bitemporal, BGE, Ebbinghaus | ✅ |
| 34. Deep Cognitive | 90 | 0.90.x | Dream, Ego, Heartbeat, Babel, AutoSkill | ✅ |
| 35. Polimento + Ecosystem | 91 | 0.91.x | burn-flex, MSched, SkillManifest macro | ✅ |
| 36. WASM Runtime | 93 | 0.93.x | MemoryPool, 15 WASI→Skill, BitNet IDE | ✅ |
| 37. Vision | 94 | 0.94.x | UVC camera, YOLO, TTF engine | ✅ |
| 38. Cognitive Engine | 95 | 0.95.x | 25+ itens: IntentPlanner, CodebookVQ, BitNetTrainer | ✅ |
| 39. Self-Healing | 96 | 0.96.x | FailureTaxonomy, CorrectivePrompting, SFS | ✅ |
| 40. RustCoder Expert | 97 | 0.97.x | 1.6M params, 444KB, loss 0.34 | ✅ |
| 41. Trinity MoE no LLM | 98 | 0.98.x | generate_via_model() roteia internamente | ✅ |
| 42. SDIO Dataset | 99 | 0.99.x | 95.812 entradas, 45 packs, pefile | ✅ |
| 43. RegMap IA | 100 | 0.100.x | 3 níveis: HWID→IA→Heurística | ✅ |
| 44. MoE Router + Boot Agent | 101 | 0.101.x | Router IA, Boot Agent IA | ✅ |
| 45. Trinity AutoLearn | 102 | 0.102.x | Detecta→treina→registra expert on-device | ✅ |
| 46. SmileyOS Nativo | 103 | 0.103.x | 55+ cmd, drag, resize, wasm exec, llm icons | ✅ |
| —. Sprint Sound | — | — | HDA, USB stub, Piper TTS, VAD, SER, Wake Word (código), Mixer — ver ADR-0045 | ✅ base / ▶️ Sprint 107 loop |
| 47. v1.1.1 GPU+Firmware+HW Expert | 104 | 1.1.1 | ACR WPR, 61K VID/DID, firmware loading | ✅ |
| 48. v1.1.2 SelfHealing | 105 | 1.1.2 | I3/I4, hot_load_firmware, HEALTH_ISSUE | ✅ |
| 49. v1.1.3 Visual+Audio | 106 | 1.1.3 | 3 layers, HDA playback, BrowserAgent | ✅ |
| 50. v1.1.4 WiFi AX200 | 107 | 1.1.4 | iwlwifi ucode loading, CSR/HBUS/SRAM | ✅ |
| 51. v1.1.5 Integração | 108 | 1.1.5 | Docs, release, sprint plan final | ✅ |

---

## ✅ Sprint 106 — v2.0 Cognição: Refatoração para Ecossistema de Anéis Lógicos

**Status:** ✅ Concluído (10/10 sub-sprints)  
**Objetivo:** Desacoplamento do monólito bare-metal (v1.0) para ecossistema de anéis lógicos isolados com orquestração IA nativa (WASM + MicroPython sandbox)

### Sprint 106-1: Estruturar Cargo workspace estrito
- **Objetivo:** Criar workspace com 5 membros (k_nano, k_ai, cortex, hermes, jarbas)
- **Status:** ✅ Concluído
- **Ações:**
  - Criado crates/k_ai (backup de k_ia)
  - Criado crates/jarbas (backup de jarvis)
  - Atualizado Cargo.toml raiz com members: k_nano, k_ai, cortex, hermes, jarbas
  - Atualizados Cargo.toml internos (k_ai:2.0.0, jarbas:2.0.0)
  - Atualizadas dependências cross-crate (k_ia → k_ai, jarvis → jarbas)

### Sprint 106-2: Renomear crates k_ia→k_ai e jarvis→jarbas
- **Objetivo:** Alinhar nomes ao ADR v2.0
- **Status:** ✅ Concluído
- **Ações:**
  - Copiado crates/k_ia → crates/k_ai
  - Copiado crates/jarvis → crates/jarbas
  - Atualizados nomes de pacotes (Cargo.toml)
  - Atualizados `use` statements em todos os arquivos

### Sprint 106-3: Corrigir SOUL.md parser (dependência ring2→ring0)
- **Objetivo:** Remover dependência circular (jarbas acessando k_nano diretamente)
- **Status:** ✅ Concluído
- **Ações:**
  - `jarbas` usa `neural_kernel::fs::read_vfs()` em 4 arquivos (jarvis.rs, gpu/firmware.rs, audio/skills.rs, audio/neural.rs)
  - 0 referências a `ATA_DRIVER`/`fat32` em jarbas — isolamento ring2→ring0 validado

### Sprint 106-4: Corrigir Trinity MoE Router
- **Objetivo:** Trinidade deve rotear para Hermes agents (WASM/Python), não para K-Nano drivers
- **Status:** ✅ Concluído
- **Ações:**
  - Verificar ExpertKind enum (não acessar k_nano)
  - Remover dependência circular Trinity→k_nano

### Sprint 106-5: RustPython no_std (Rota Nativa - Python Bare-Metal)
- **Objetivo:** Embed RustPython com `#![no_std]`, bridge via `abi_x86_interrupt`
- **Status:** ✅ Concluído
- **Ações:**
  - Criado hermes/src/rustpython_no_std.rs
  - Embed RustPython com `#![no_std]`
  - Bridge rust→python via abi_x86_interrupt
  - Agentes efêmeros Python descartáveis

### Sprint 106-6: MicroPython via WASM (Rota Sandbox)
- **Objetivo:** Compilar MicroPython para .wasm, sandbox dentro de sandbox
- **Status:** ✅ Concluído
- **Ações:**
  - Compilado MicroPython para .wasm
  - Hermes: WASM executor com sandbox isolado

### Sprint 106-7: Corrigir page faults (ordem de inicialização)
- **Objetivo:** Inicialização correta: allocator → events → agents
- **Status:** ✅ Concluído
- **Ações:**
  - Reordenada inicialização: allocator → events → agents
  - Adicionado lazy_init!() para agentes dependentes de heap
  - Validado com cargo run --release (sem page faults)

### Sprint 106-8: AIOS API para Python (RAG + System Prompt injection)
- **Objetivo:** Bibliotecas internas (aios_net, aios_fs) injetadas no RustPython via RAG
- **Status:** ✅ Concluído
- **Ações:**
  - Criado hermes/src/aios_api.rs
  - Bibliotecas internas (aios_net, aios_fs)
  - Injeção via RAG/System Prompt

### Sprint 106-9: Escalonamento Evolutivo de Código (JIT Cognitivo)
- **Objetivo:** Python efêmero → WASM cravado em pedra via SkillOpt + Knowledge Graph
- **Status:** ✅ Concluído
- **Ações:**
  - SkillOpt + Knowledge Graph implementado
  - Evolução de código: Python efêmero → WASM cravado

### Sprint 106-10: SkillOpt - Tradução Python→Rust no_std
- **Objetivo:** Geração de Rust no_std a partir de Python via Cortex LLM
- **Status:** ✅ Concluído
- **Ações:**
  - Criado hermes/src/skill_opt.rs
  - Geração de Rust no_std a partir de Python via Cortex LLM

---

## 📊 RESUMO v2.0 "Cognição"

| Sprint | Foco | LOC | Status |
|--------|------|-----|--------|
| 100 | Code Freeze v1.0.0 | ~500 | ✅ |
| 101 | TTS+STT+ATA fix+NVIDIA GPU | ~2.000 | ✅ |
| 102 | GPU Compute + HW Expert v3 + Firmware | ~1.500 | ✅ |
| 103-104 | K²CHJ Workspace Migration | ~500 | ✅ |
| 105 | Ponytail Audit + v1.5.1..v1.5.3 | ~200 | ✅ |
| 106-1 | Estruturar workspace estrito | ~100 | ✅ |
| 106-2 | Renomear crates k_ia→k_ai, jarvis→jarbas | ~200 | ✅ |
| 106-4 | Corrigir Trinity MoE Router | ~300 | ✅ |
| 106-5 | RustPython no_std | ~400 | ✅ |
| 106-6 | MicroPython/WASM sandbox | ~300 | ✅ |
| 106-7 | Corrigir page faults | ~200 | ✅ |
| 106-8 | AIOS API para Python | ~300 | ✅ |
| 106-9 | Escalonamento Evolutivo de Código | ~500 | ✅ |
| 106-10 | SkillOpt - Tradução Python→Rust | ~400 | ✅ |
| 106-3 | Corrigir SOUL.md parser | ~300 | ✅ |
| 106-11 | Heap address HW real + boot diagnostics | ~100 | ✅ |
| **Total v2.0** | | **~9.000 LOC** | |

---

## ▶️ Sprint 107 — v2.0 Voice I/O Pipeline

**Status:** ▶️ PASS parcial forte (v1.7.2); Voice I/O loop parcial  
**Objetivo:** Pipeline TTS→STT→LLM→TTS, VAD, wake word ML.  
**Capability (2026-07-14):** Boot A/B + P0–P9 PoC no monólito — ver ADR-0041 / SESSION_107. Follow-ups: Ring3 QEMU, QUEUE_NOTIFY, SFI #426.

---

## 🔴 Próximos Passos — Boot HW Real + v2.0

| Item | % | Esforço | Descrição |
|------|---|---------|-----------|
| **B-01** DHCP/RX funcional | ~500 LOC | smoltcp DHCP nunca completa | ⏳ |
| Wake Word ML | 90% | ~100 LOC | Heurística→modelo simples | ⏳ |
| burn-flex Backend | 70% | ~300 LOC | Integrar burn::Backend trait | ⏳ |
| MSched VRAM | 70% | ~200 LOC | Conectar predictor ao scheduler GPU | ⏳ |
| GPU Display sharing | 70% | ~200 LOC | Context switch iGPU/dGPU | ⏳ |
| BGE HNSW index | 60% | ~400 LOC | Substituir busca linear por HNSW | ⏳ |
| v86 browser (#279e) | 0% | ~500 LOC | Emulador x86 WASM | ⏳ |
| Desktop Cube 3D | 50% | ~200 LOC | Transições 3D GPU | ⏳ |
| BitNet IDE avançado | 40% | ~500 LOC | Debug WASM, syntax highlight | ⏳ |
| Skill Market | 0% | ~500 LOC | Marketplace de skills | ⏳ |

---

## 📊 ADR v2.0 — Topologia do Workspace

**Status:** ✅ Migração concluída (Sprint 106) — integração gradual via `neural-kernel` bin

```
[workspace]
members = [
    "crates/k_nano",    # Ring 0 Estrito (HAL, drivers, PCI, memory)
    "crates/k_ai",      # Ring 1 Lógico (Sondagem, SelfHeal, Trust)
    "crates/cortex",    # Cognição e MoE (Trinity, BitNet, BPE)
    "crates/hermes",    # Executor (WASM, RustPython, Rede, Intent)
    "crates/jarbas",    # HCI, UI e Persona (Display, Audio, CLI)
]
```

**Isolamento de Camadas:**
- Ring 0 (k_nano): HAL, drivers, PCI, memory — acesso direto ao hardware
- Ring 1 (k_ai): Sondagem, SelfHeal, Trust — lógica de autogestão
- Ring 2 (cortex+hermes+jarbas): Cognição, orquestração, UI

---

**Detalhes completos:** `TODO.md`
**Catálogo de tecnologias:** `docs/TECNOLOGIAS.md`
**Plano de sprints detalhado:** `docs/sprint-plan-92-100.md`
