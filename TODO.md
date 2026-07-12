# 📋 TODO MASTER — neural-os-core v1.1.5

**Data:** 2026-07-12  
**Propósito:** Checklist mestre do roadmap v1.1.x.  
**Documento oficial:** `docs/sprint-plan-v1.1.x.md`  
**Legenda:** ✅ feito | 🟡 em andamento | 🔴 bloqueado | ⏳ agendado

---

## ✅ SPRINTS 1-91 + SOUND — COMPLETOS (~19.000 LOC)

| Sprint | v | Foco | LOC | Status |
|--------|---|------|-----|--------|
| 1-83 | v0.2-v0.83 | Fundação (boot, PCI, SMP, GPU, JARVIS, AHCI, NVMe) | ~12.000 | ✅ |
| 84 | v0.84.x | GPU Foundations (BAR UC, SPSC ring, VRAM buddy, secure boot) | ~1.700 | ✅ |
| 85 | v0.85.x | GPU Decode (prefill/decode, KV DMA, XQueue) | ~1.500 | ✅ |
| 86 | v0.86.x | JARVIS Persona (SOUL.md, IPW, Compression, Notification) | ~950 | ✅ |
| 87 | v0.87.x | JARVIS Security+AHCI (I1-I4, Audit, Fluid Persona, AHCI NCQ) | ~1.200 | ✅ |
| 88 | v0.88.x | JARVIS Emotion+Cache (ADE, Pipeline 16 stages, edge-dhcp) | ~1.200 | ✅ |
| 89 | v0.89.x | SleepCycle+Memory (5 fases, KG, BGE, Ebbinghaus, Atkinson) | ~2.500 | ✅ |
| 90 | v0.90.x | JARVIS Deep Cognitive (Dream, Ego, Heartbeat, AutoSkill) | ~1.200 | ✅ |
| 91 | v0.91.x | Polimento+Ecosystem (burn-flex, MSched, CFS, SkillManifest) | ~2.500 | ✅ |
| Sound | v0.Sound | Áudio (HDA, UAC, TTS, VAD, SER, Wake Word, RingBuffer, Mixer) | ~2.000 | ✅ |

---

## 🏆 B-01 RESOLVIDO (v0.109.3 — 2026-07-09)
Serial tunnel TCP bridge: `serial_bridge.py` (servidor) ← QEMU `-serial tcp:4444` (cliente) ← COM2 ← `slip.rs`.

---

## 🟡 SPRINT 92 — Fundação Estável (~2.000 LOC)
**Foco:** VirtIO MMIO fix, AHCI, serial/DNS hardening, code cleanup

| Item | LOC | Arquivo |
|------|-----|---------|
| VirtIO-GPU GET_DISPLAY_INFO fix | ~100 | `virtio_gpu.rs` |
| VirtIO-net MMIO page fault fix | ~200 | `virtio_net.rs` |
| AHCI disk reading verification | ~200 | `ahci.rs` |
| Serial tunnel DNS hardening (timeouts, retry) | ~150 | `slip.rs`, `netstack.rs` |
| Serial tunnel watchdog (reconexão auto) | ~100 | `serial_bridge.py` |
| Zero-Trust Syscall Categories (#364) | ~200 | `trust.rs` |
| Neural Cache per token (#365) | ~150 | `cognitive.rs` |
| Capability token crypto (#405) | ~200 | `trust.rs` |
| Code cleanup — unwrap() perigosos | ~300 | kernel todo |
| Code cleanup — debug prints | ~100 | kernel todo |
| Code cleanup — dead code | ~200 | kernel todo |

---

## 🟡 SPRINT 93 — WASM Runtime + IDE (~3.200 LOC)
**Foco:** wasmi embedder, sandbox, IDE, skill marketplace

| Item | LOC |
|------|-----|
| WASM embedder (wasmi no_std + fuel metering) | ~800 |
| WASM App Sandbox (PTE NX + fuel + rollback) | ~400 |
| BitNet IDE avançado (debug, preview, syntax) | ~500 |
| AgentManifest JSON format | ~200 |
| 15 WASI→skill mappings | ~350 |
| Performance budget table | ~100 |
| Skill Market / Plugin Hub | ~500 |
| WASM Host Function Interface | ~200 |
| Developer contract for WASM agents | ~80 |
| Hybrid agents (kernel + WASM) | ~100 |

---

## 🟡 SPRINT 94 — GPU Polish + Display (~2.000 LOC)

| Item | LOC |
|------|-----|
| MSched VRAM scheduling (Belady) | ~200 |
| GPU Display time-sharing | ~200 |
| Compositor multi-window (dock, menus, drag) | ~300 |
| LLM Icons via HWEXPERT_MODEL | ~200 |
| Observability (tracing/metrics) | ~500 |
| Human-in-the-Loop Approval | ~250 |
| Actor Registry | ~500 |

---

## 🟡 SPRINT 95 — Memory + VFS Final (~2.000 LOC)

| Item | LOC |
|------|-----|
| BGE HNSW index | ~400 |
| MHI+FS Bridge (VFS↔MHI) | ~600 |
| InferenceFsAgent | ~100 |
| HermesFsAgent | ~100 |
| RamFsAgent | ~100 |
| Auto tier migration (MhiScheduler) | ~200 |
| HwRegistry + LLM activation | ~200 |
| Agency Importer (147 agents) | ~600 |
| Observation Protocol | ~200 |

---

## 🟡 SPRINT 96 — GGUF + Model Loading (~1.500 LOC)

| Item | LOC |
|------|-----|
| GGUF loader mínimo (header + metadata + Q4_0) | ~500 |
| GGUF v3 streaming (ATA/USB, >4GB) | ~500 |
| RoPE + inner_attn_ln (BitNet v3.1) | ~300 |
| .bitnet v3 header extensível | ~200 |
| Model swap flow (/model \<path\>) | ~200 |

---

## 🟡 SPRINT 97 — Rede + AIOS Evolution (~3.000 LOC)

| Item | LOC |
|------|-----|
| WWW Agents (Email, Search, RSS, Download) | ~2.600 |
| Self-Update Agent (A/B slots) | ~500 |
| Update channels (stable/nightly/security) | ~200 |
| Rollback automático | ~100 |
| J.A.R.V.I.S. Context Window Manager | ~100 |
| Skill Marketplace signed packages | ~300 |
| Plugin Hub / MCP Index | ~400 |

---

## 🟡 SPRINT 98 — BitNet + Training Pipeline (~2.500 LOC)

| Item | LOC |
|------|-----|
| Train 100M/1.5B params (GPU) | ~800 |
| burn-flex Backend trait | ~300 |
| TrainingAgent (fine-tune/transfer) | ~500 |
| Self-Learning OS (DataCollector) | ~300 |
| Wake Word ML (modelo) | ~100 |
| Intel GEN shader matmul | ~300 |
| AMD PM4 / NVIDIA PFIFO stubs | ~400 |

---

## 🟡 SPRINT 99 — SkillOpt + Code Freeze Prep (~1.500 LOC)

| Item | LOC |
|------|-----|
| SkillOpt (MS Research) | ~145 |
| Structured Decoding (SGLang) | ~120 |
| Documentação técnica final | ~500 |
| ADRs review (39 docs) | ~300 |
| CHANGELOG v1.0 | ~100 |
| Dead code removal — pass 2 | ~200 |

---

## ✅ SPRINT 100 — Code Freeze & Release v1.0.0 (~500 LOC)

| Item | Check |
|------|-------|
| `cargo clean -p neural-kernel && cargo check --release` (3x) | ⬜ |
| QEMU BIOS boot (serial OK, PCI OK, agents OK) | ⬜ |
| QEMU UEFI boot | ⬜ |
| QEMU serial tunnel (DNS, ping, HTTP) | ⬜ |
| QEMU AHCI (FAT32 via ide-hd) | ⬜ |
| QEMU SMP 2 cores | ⬜ |
| VirtualBox boot test | ⬜ |
| Tag v1.0.0 + release notes | ⬜ |

---

## 📊 RESUMO v1.0

| Sprint | Foco | LOC | Status |
|--------|------|-----|--------|
| 92 | Fundação Estável | ~2.000 | 🟡 Ativa |
| 93 | WASM Runtime + IDE | ~3.200 | ⏳ |
| 94 | GPU Polish + Display | ~2.000 | ⏳ |
| 95 | Memory + VFS Final | ~2.000 | ⏳ |
| 96 | GGUF + Model Loading | ~1.500 | ⏳ |
| 97 | Rede + AIOS Evolution | ~3.000 | ⏳ |
| 98 | BitNet + Training Pipeline | ~2.500 | ⏳ |
| 99 | SkillOpt + Code Freeze Prep | ~1.500 | ⏳ |
| 100 | Code Freeze & Release v1.0.0 | ~500 | ⏳ |
| **Total v1.0** | | **~18.200 LOC** | |

**Após v1.0: v2.0 "Cognição" — Kernel, Cortex, Hermes, JARVIS como entidade viva.**
