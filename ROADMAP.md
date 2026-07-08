# Roadmap — neural-os-core v0.109.0 🏆

**Última atualização:** 2026-07-08
**Estado:** ~19.000 LOC, 165+ arquivos Rust, 247+ agentes, 0 erros

---

## ✅ Sprints 1-103 — Completos

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
| —. Sprint Sound | — | — | HDA, USB, TTS, VAD, SER, Wake Word, Mixer | ✅ |

## 🔵 Sprint 92 — Itens Não Bloqueados (~3.200 LOC)

| Item | % | Esforço | Descrição |
|------|---|---------|-----------|
| Wake Word ML | 90% | ~100 LOC | Heurística→modelo simples |
| burn-flex Backend | 70% | ~300 LOC | Integrar burn::Backend trait |
| MSched VRAM | 70% | ~200 LOC | Conectar predictor ao scheduler GPU |
| GPU Display sharing | 70% | ~200 LOC | Context switch iGPU/dGPU |
| BGE HNSW index | 60% | ~400 LOC | Substituir busca linear por HNSW |
| v86 browser (#279e) | 0% | ~500 LOC | Emulador x86 WASM |
| Desktop Cube 3D | 50% | ~200 LOC | Transições 3D GPU |
| BitNet IDE avançado | 40% | ~500 LOC | Debug WASM, syntax highlight |
| Skill Market | 0% | ~500 LOC | Marketplace de skills |

## 🔴 B-01 — Único Bloqueador Real

| Item | Esforço | Descrição |
|------|---------|-----------|
| **B-01** DHCP/RX funcional | ~500 LOC | smoltcp DHCP nunca completa |
| WWW Agents | ~2.600 | Email, Search, RSS, Download |
| Self-Update Agent | ~800 | A/B slots, channels |
| Cross-OS compat | ~2.000 | PE/ELF/Mach-O/APK |
| Federated Cluster | ~300 | Mesh multi-máquina |
| Multi-device sync | ~300 | CRDT |
| AppForge | ~3.000 | Apps multi-usuário |

## ⏳ Pós-MVP

| Item | Esforço |
|------|---------|
| GGUF v3 loader (modelos 9B+) | ~500 LOC |
| NPU AMD XDNA driver (💰 sponsor) | ~2.000 LOC |
| ARM/RISC-V port (💰 sponsor) | ~5.000 LOC |

---

**Detalhes completos:** `docs/TODO.md`
**Catálogo de tecnologias:** `docs/TECNOLOGIAS.md`
