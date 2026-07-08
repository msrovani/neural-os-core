# 📋 TODO MASTER — neural-os-core v0.109.0

**Data:** 2026-07-08  
**Propósito:** Checklist mestre do estado real do projeto.  
**Legenda:** ✅ feito | 🟡 parcial | 🔴 bloqueado | ⏳ pós-MVP | 💰 sponsor | ❌ descartado

---

## ✅ SPRINTS 84-103 — COMPLETOS (~19.000 LOC, 165+ arquivos Rust, 0 erros)

Todos os sprints de 84 a 103 foram implementados e verificados. Detalhes em `docs/TECNOLOGIAS.md`.

| Sprint | v | Foco | LOC | Status |
|--------|---|------|-----|--------|
| 84 | 0.84.x | GPU Foundations (BAR UC, SPSC ring, VRAM buddy, secure boot) | ~1.700 | ✅ |
| 85 | 0.85.x | GPU Decode (prefill/decode, KV DMA, XQueue) | ~1.500 | ✅ |
| 86 | 0.86.x | JARVIS Persona (SOUL.md, IPW, Compression, Notification) | ~950 | ✅ |
| 87 | 0.87.x | JARVIS Security+AHCI (I1-I4, Audit, Fluid Persona, AHCI NCQ) | ~1.200 | ✅ |
| 88 | 0.88.x | JARVIS Emotion+Cache (ADE, Pipeline 16 stages, edge-dhcp) | ~1.200 | ✅ |
| 89 | 0.89.x | SleepCycle+Memory (5 fases, KG, BGE, Ebbinghaus, Atkinson) | ~2.500 | ✅ |
| 90 | 0.90.x | JARVIS Deep Cognitive (Dream, Ego, Heartbeat, AutoSkill) | ~1.200 | ✅ |
| 91 | 0.91.x | Polimento+Ecosystem (burn-flex, MSched, CFS, SkillManifest) | ~2.500 | ✅ |
| 92-94 | 0.92-0.94 | LAN+WASM+Vision (smoltcp, WASM Runtime, UVC, YOLO, TTF) | ~3.000 | ✅ |
| 95 | 0.95.x | Cognitive Engine (25+ itens: IntentPlanner, CodebookVQ, etc) | ~510 | ✅ |
| 96 | 0.96.x | Self-Healing (FailureTaxonomy, CorrectivePrompting, SFS) | ~350 | ✅ |
| 97 | 0.97.x | RustCoder Expert + Trinity MoE (1.6M params, 444KB) | ~300 | ✅ |
| 98 | 0.98.x | Trinity MoE no LLM (generate_via_model roteia internamente) | ~50 | ✅ |
| 99 | 0.99.x | SDIO Dataset Pipeline (95.812 entradas, 45 packs) | ~500 | ✅ |
| 100 | 0.100.x | Register Map IA (3 níveis: HWID→IA→Heurística) | ~250 | ✅ |
| 101 | 0.101.x | MoE Router + Boot Agent IA | ~130 | ✅ |
| 102 | 0.102.x | Trinity AutoLearn (detecta→treina→registra expert) | ~170 | ✅ |
| 103 | 0.103.x | SmileyOS Nativo (55+ cmd, drag, resize, wasm exec, llm icons) | ~450 | ✅ |
| Sound | — | Áudio (HDA, UAC, TTS, VAD, SER, Wake Word, RingBuffer, Mixer) | ~2.000 | ✅ |

---

## 🔴 BLOQUEADO — B-01 DHCP/RX (Único bloqueador real)

| Item | Esforço | Descrição |
|------|---------|-----------|
| **B-01** DHCP/DNS/HTTP funcional | ~500 LOC | smoltcp DHCP nunca completa (Configured nunca é recebido). RX fix RTL8139/E1000. **Único bloqueador de ~18K LOC de sprints 92+.** |

### Bloqueados por B-01:
| Item | LOC | Descrição |
|------|-----|-----------|
| WWW Agents (Email, Search, RSS, Download) | ~2.600 | Dependem de rede funcional |
| Self-Update Agent (A/B slots, channels) | ~800 | Depende de download |
| Cross-OS compat (PE/ELF/Mach-O/APK) | ~2.000 | Depende de download de amostras |
| Federated Cluster / Mesh | ~300 | Depende de rede |
| Multi-device sync (CRDT) | ~300 | Depende de rede |

---

## 🟡 PARCIAL — Itens com implementação parcial

| Item | % | Descrição | Arquivo |
|------|---|-----------|---------|
| Wake Word "Jarvis" | 90% | Heurístico (energia), sem ML | `audio/wakeword.rs` |
| burn-flex backend | 70% | Stub funcional, sem integração com burn::Backend trait | `burn_flex.rs` |
| MSched VRAM eviction | 70% | Predictor Belady/OPT, sem scheduling real | `gpu/msched.rs` |
| GPU Display Co-existence | 70% | Planner, sem time-sharing real | `gpu/display_coex.rs` |
| BGE Embedding semantic_search | 60% | index_embedding + cosine, sem HNSW index real | `memory_systems.rs` |

---

## ⏳ PÓS-MVP — Planejados mas não priorizados

| Item | Esforço | Descrição |
|------|---------|-----------|
| **#279e** v86 browser demo | ~500 LOC | Emulador x86 em WASM |
| NPU AMD XDNA driver (#43-52) | ~2.000 | NPU on-chip (+ sponsor) |
| ARM/RISC-V port (#116) | ~5.000 | Portabilidade cross-arch |
| AppForge / Multi-User (#186-189) | ~3.000 | Ambiente de apps multi-usuário |
| Actor Registry (#210-227) | ~1.000 | Framework de atores concorrentes |
| Observability (#241-247) | ~1.500 | Tracing, metrics, logging avançado |
| GGUF v3 loader (#278) | ~500 | Modelos 9B+ com heap >5GB |
| Skill Market / Plugin Hub | ~1.000 | Marketplace de skills | 
| Desktop Cube 3D + crossfade | ~200 | GPU workspace transitions |
| BitNet IDE avançado | ~500 | IDE WASM com debug + preview |

---

## 💰 SPONSOR — Itens que dependem de financiamento

| Item | Esforço | Descrição |
|------|---------|-----------|
| NPU AMD XDNA driver | ~2.000 LOC | Driver para NPU on-chip |
| Cross-arch (ARM/RISC-V) | ~5.000 LOC | Portabilidade |
| Federated learning | ~1.000 LOC | Gradientes compartilhados entre AIOS |

---

## ❌ DESCARTADOS

| Item | Motivo |
|------|--------|
| HDMI audio (Intel HDA legado) | ✅ Implementado em `audio/hda.rs` (não descartar) |
| USB Audio Class legado | ✅ Implementado em `audio/usb.rs` (não descartar) |

---

## 📊 RESUMO

| Categoria | Items | LOC |
|-----------|-------|-----|
| ✅ Completos (Sprints 84-103 + Sound) | ~200 | ~19.000 |
| 🔴 Bloqueado (B-01) | 1 | ~18.000 bloqueados |
| 🟡 Parciais | 5 | ~500 |
| ⏳ Pós-MVP | ~10 | ~10.000 |
| 💰 Sponsor | ~3 | ~8.000 |
| **Total** | ~220 | **~19.000 implementados** |

**Único bloqueador real:** B-01 (DHCP/RX). Resolva isso e ~18K LOC são destravados.
