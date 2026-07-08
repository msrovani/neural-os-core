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

## 🟡 SPRINT 92 — Itens não bloqueados por B-01 (~4.700 LOC)

| Item | % | Esforço | Descrição | Arquivo |
|------|---|---------|-----------|---------|
| Wake Word ML | 90% | ~100 LOC | Substituir heurística por modelo simples | `audio/wakeword.rs` |
| burn-flex Backend trait | 70% | ~300 LOC | Integrar FlexBackend com burn::Backend trait | `burn_flex.rs` |
| MSched VRAM scheduling | 70% | ~200 LOC | Conectar predictor Belady ao scheduler GPU real | `gpu/msched.rs` |
| GPU Display time-sharing | 70% | ~200 LOC | Context switch iGPU/dGPU | `gpu/display_coex.rs` |
| BGE HNSW index | 60% | ~400 LOC | HNSW approximate nearest neighbor | `memory_systems.rs` |
| BitNet IDE avançado | 40% | ~500 LOC | Debug WASM, preview, syntax highlight | `wasm_rt.rs` |
| Skill Market / Plugin Hub | 0% | ~500 LOC | Marketplace de skills 1-click | Novo |
| **60.5** Train 100M/1.5B params | 0% | ~800 LOC | Treino on-device GPU física | `tools/train_*.py` |
| **61.5** LLM Icons | 50% | ~200 LOC | Ícones via HWEXPERT_MODEL | `display/compositor.rs` |
| **61.6** WASM App Sandbox | 40% | ~400 LOC | Sandbox fuel/perm/cap | `wasm_rt.rs` |
| **62.2-6** MHI+FS Bridge | 30% | ~600 LOC | Integração VFS↔MHI completa | `vfs/`, `fs/` |
| **63** GGUF model swap | 20% | ~500 LOC | Modelos 9B+ heap dinâmico | `gguf.rs` |
| **66** AMD PM4 / NVIDIA PFIFO | 30% | ~400 LOC | GPU ring buffer por vendor | `gpu/{nvidia,amd}.rs` |
| **66** Intel GEN shader matmul | 20% | ~300 LOC | GPU matmul via shader | `gpu/intel.rs` |
| **67** Agency Importer | 30% | ~600 LOC | Parser .md→AgentManifest | `agency.rs` |
| **67** Observation Protocol | 40% | ~200 LOC | skill_observer persistente | `skill_observer.rs` |
| **ADR-0019** RoPE + inner_attn_ln | 30% | ~300 LOC | BitNet v3.1 features | `cortex.rs` |
| **ADR-0028** GGUF v3 streaming | 20% | ~500 LOC | Streaming ATA/USB 9B+ | `gguf.rs` |
| **#364** Zero-Trust Syscall | 0% | ~200 LOC | 4 classes de syscall c/ budget | `trust.rs` |
| **#365** Neural Cache per token | 0% | ~150 LOC | Cache avaliações LLM por capability | `cognitive.rs` |
| **#405** Capability token crypto | 0% | ~200 LOC | Token criptografado scoped | `trust.rs` |
| **#406** VirtIO-GPU fix | 0% | ~100 LOC | GET_DISPLAY_INFO QEMU TCG | `virtio_gpu.rs` |
| **#277a** HwRegistry+LLM | 60% | ~200 LOC | PCI→HwAgent→LLM activation | `hw_agents.rs` |
| **#279d** Compositor drag/menus | 60% | ~300 LOC | Drag, close btn, dock bar refinar | `display/compositor.rs` |
| **#282e** InferenceFsAgent | 40% | ~100 LOC | LLM gera arquivos | `vfs/` |
| **#282f** HermesFsAgent | 40% | ~100 LOC | Chat como FS | `vfs/` |
| **#282g** RamFsAgent | 40% | ~100 LOC | Cache DRAM | `vfs/` |
| **#282h** Auto tier migration | 20% | ~200 LOC | MHI promove/demove | `vfs/` |

---

## 🔴 BLOQUEADO — B-01 DHCP/RX (dependem de rede funcional)

| Item | Esforço | Descrição |
|------|---------|-----------|
| **B-01** DHCP/DNS/HTTP funcional | ~500 LOC | smoltcp DHCP nunca completa. RX fix RTL8139/E1000 |
| WWW Agents (Email, Search, RSS, Download) | ~2.600 | Dependem de rede funcional |
| Self-Update Agent (A/B slots, channels) | ~800 | Depende de download |
| Cross-OS compat (PE/ELF/Mach-O/APK) | ~2.000 | Depende de download de amostras |
| Federated Cluster / Mesh | ~300 | Depende de rede |
| Multi-device sync (CRDT) | ~300 | Depende de rede |
| AppForge / Multi-User | ~3.000 | Depende de coordenação em rede |
| Actor Registry | ~1.000 | Depende de comunicação entre nós |
| Observability (tracing/metrics) | ~1.500 | Depende de exportação via rede |

---

## ⏳ PÓS-MVP — Itens sem bloqueio mas não priorizados

| Item | Esforço | Descrição |
|------|---------|-----------|
| GGUF v3 loader (#278) | ~500 LOC | Modelos 9B+ com heap >5GB |
| NPU AMD XDNA driver (#43-52) | ~2.000 | NPU on-chip (+ sponsor) |
| ARM/RISC-V port (#116) | ~5.000 | Portabilidade cross-arch |
| **#279e** v86 browser demo | ~500 LOC | Emulador x86 em WASM |
| **#283a+b** Workspace Cube 3D + crossfade | ~200 LOC | Transições 3D GPU (#283a) + fallback sem FPU (#283b fallback não-GPU do cube) |

---

## ❌ SUBSTITUÍDOS / SUPERSEDED

| Item | Motivo |
|------|--------|
| **#360** Kokoro-82M TTS | Substituído pelo Pocket TTS 100M implementado em `audio/neural.rs` (GPU offload, loss 0.38) |

---

## 💰 SPONSOR

| Item | Esforço |
|------|---------|
| NPU AMD XDNA driver | ~2.000 LOC |
| Cross-arch (ARM/RISC-V) | ~5.000 LOC |
| Federated learning | ~1.000 LOC |

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
