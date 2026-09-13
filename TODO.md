# 📋 TODO — neural-os-core

**Versão:** v1.9.99-s328 TEST
**Data:** 2026-09-10
**Fonte:** ADRs 0089–0103 (`docs/architecture/`) + SESSION_316/321/328
**Legenda:** ✅ feito | 🟡 em andamento | `[~]` parcial | 🔴 bloqueado | ⏳ agendado | ▶️ AWAITING_HW | `[ ]` pendente

---

## 🎯 OBJETIVOS

1. **Aceite metal pós-s328** — flash `usb_hw.img` (6271MB READY) e validar InferQueue/UI/BOOT.LOG no Alienware.
2. **Gate v2.0.0** — fechar ADR-0100 Ondas 0–3 + review formal + OK maintainer.
3. **Emagrecer `k_nano`** — primitivos R0 + hooks (ADR-0103; FASE A–H feita, S1 metal pendente).
4. **Sandbox Ring3 CPL=3** — um sandbox para blob nativo B/C; wasmi default até T-055 (ADR-0102).
5. **Kernel ternário-nativo** — Falcon3-3B 1.58-bit com ADD/SUB/SKIP packed no metal (ADR-0101).
6. **Observabilidade de boot** — 3 canais + `BOOT SCORE` + instrumentos FB (ADR-0092).
7. **Desktop Jarbas v2** — production-grade (ADR-0090).
8. **SMP per-CPU runqueue** — feature ON; falta aceite metal (ADR-0089).

---

## 🔥 ABERTOS NO METAL (s316–s328)

| Item | Evidência | Estado |
|------|-----------|--------|
| Freeze determinístico @ tick 1370 `network_agent` | SESSION_316 / STATE s327 | ABERTO — discriminador heartbeat (s326) aguarda boot no metal |
| xHCI MSC `CCS=0` pós-HCRST (HCRST deixa PP=0) | s327 (fix PP=1 RMW) | 🟡 wired; validar metal |
| UI/splash first-frame com MSC sem teto | s315/s296 | 🟡 wired; validar metal |
| BOOT.LOG `E:\BOOT.LOG` real | 0103 S1 | ▶️ AWAITING_OPERATOR |

## ⚠️ SAÚDE DE TESTES / CI

- [ ] Corrigir 6 testes host falhando: `hermes::wasm_build::{compile_and_run_real_skill,dsl_print_cmp}`, `hermes::cognitive_bridge::session_load_respects_cap`, `jarbas::jarvis::soul_{describe,default_jarbas,fluid_update_joy}`
- [ ] Fixture do teste `cortex`: `python tools/gen_test_gguf.py` → `target/test_tq2_0.gguf` (sem ela `cargo test --workspace` não compila o teste)
- [ ] CI verde: nesta revisão `cargo test … --no-fail-fast` = **784 pass / 6 fail** (sem `--no-fail-fast` o cargo para na 1ª suíte que falha)

## 📌 PÓS-s328 (ordem do STATE)

`aceite metal → Prefill AirLLM layer-yield → métricas/budget TSC → Layer S (GPU/NPU WS-D/E)`

- [ ] Aceite metal: orb+mouse+mic vivos durante o generate; 1ª frase TTS antes de `LLM_RESPONSE`
- [ ] Prefill AirLLM: layer-yield (evitar hiccup no prompt)
- [ ] Métricas/budget por TSC
- [ ] Layer S: GPU/NPU W2A8 (WS-D/E) — só após o aceite (dispatcher já existe; falta Ready/KernelPack/FW). Não confundir CPU `bitnet_w2a8` (ADR-0084) com GPU BitLinearW2A8 (WS-D)

---

## ITENS (priorizados: mais nova → mais antiga)

### 1. ADR-0103 — k_nano microkernel modular
**Goal:** `k_nano` = primitivos R0 + hooks; política em `k_hal`/`k_ai`/`cortex`/`hermes`/`jarbas`. Sem virar process-OS.

- [x] **FASE A** — 13 módulos mortos deletados (~1800 LOC)
- [x] **FASE B** — NIC drivers → `k_hal::net` (facades e1000/rtl8139/i225/virtio_net)
- [x] **FASE C** — FS primitivos → `k_hal::fat_assets` canônico (+ `legacy read_mbr` re-exports)
- [x] **FASE D** — sgdb canônico em `k_ai`; telemetry mantida R0 (8 callers hermes)
- [~] **FASE G+H** — analisadas, **não migradas** (dependências + risco)
- [ ] **S1** — `k_hal::usb` hub→MSC + early `probe_and_install` → `E:\BOOT.LOG` real (▶️ AWAITING_OPERATOR; ver "Abertos no metal")
- [ ] **S3** — Leitores FS não-boot órfãos (ntfs/btrfs/ext2 read-only) → crate ou delete
- [ ] **S4** — Storage cognitivo (tickv FE, rollback UI) → k_ai/hermes
- [ ] **S5** — Podar exports mortos + `check_duplication.py` limpo
- [ ] **S6** — (Opcional) esqueleto `arch/memory/scheduler/` sem mover lógica
- [ ] **Fase 2** — Schemes CPL=3 (🔴 gated: ADR-0102 aceite HW)

### 2. ADR-0102 — Ring3 sandbox CPL=3
**Goal:** um sandbox CPL=3 para blob B/C; `isolation_ring_available()==false` ⇒ só wasmi.

- [x] **H1** — Feature `ring3` propaga no bin (`neural-kernel` `default = ["ring3","cap-demos","smp-runqueue"]`)
- [x] **H2** — Demos P6 reais no tree (`demo_ring3*` em `k_nano::paging`; SESSION_302)
- [x] **H3** — `ring3_can_iretq()` + `can_register_native()` cindidos + self-test wired no boot
- [~] Demos P6 rodam no QEMU 4c (`ring3_can_iretq=true` + Jarbas greeting; SESSION_305); faltas contidas non-fatal
- [ ] **T-051** — Separar `#GP` OVMF de `#GP` kernel (WHPX = `int 0x90`)
- [ ] **T-052** — Metal: iretq+CPL3 + fault-containment (🔴 depende Onda 2 SMP metal)
- [ ] **T-053** — Checklist 0077 §6 em HW
- [ ] **T-054** — `register_native_ring` + HITL Escalate
- [ ] **T-055** — `isolation_ring_available()==true` só então
- [ ] **T-056** — Fronteira xmm: verificador opcode **ou** XSAVE (não `#UD` por CR0.EM)
- [ ] **T-057** — `SYS_PIN_DMA` pós T-055; CapGate deny DMA a CPL=3

### 3. ADR-0101 — Falcon3-3B Cognitive Lab
**Goal:** decode m=1 do 3B faz ADD/SUB/SKIP packed no SIMD do `x86_64-unknown-none`.

- [x] **Onda 0** — Inventário 3B-first (`falcon3_boot_names`, `fat_names_for(Active)` com `FALCON3.V6`)
- [x] **Onda 0** — SSE2 skip-native + scalar ternário-nativo (`bitnet_sse.rs`, paridade vs scalar)
- [x] **Onda 0** — GGUF inferência wired (TQ2_0 + BF16 + auto-config; SESSION_309)
- [~] **Onda 0** — AVX2 no target `none` (defer explícito da ADR; metal→scalar documentado, não reescrever ainda)
- [ ] **Onda 1** — Shortlist logits + n-gram/Medusa wired no 3B + KV H2O medido
- [ ] **Onda 2** — Difficulty gate no `generate_next`; early-exit só com KL treinado
- [ ] **Onda 3** — Composição: gate + spec + kernel nativo + KV compress + SGDB

### 4. ADR-0100 — Backlog unificado K³CHJ (plano-mestre)
**Goal:** gate v2.0.0 = Onda 0 + Onda 1 (mín. T-011) + Onda 2 (um metal) + Onda 3 (A2 ou A3).

- [x] **Onda 0** — Honesty `BOOT_AI` + freeze HardwareInfo (T-001–T-006)
- [x] **Onda 1** — `measure_bandwidth` + `/hw/storage|gpu|net` (T-007–T-016)
- [ ] **Onda 2** — Metal K23 `online==madt-1` (T-017 img ✅; T-018–T-021 ▶️ metal)
- [ ] **Onda 3** — 0086 A2–A8 (T-022–T-032; A1/A9 HITL)
- [ ] **Onda 4** — `ap_pollable` + runqueue 0089 (T-033–T-044) 🟡 (feature 0089 já ON)
- [~] **Onda 6** — Ring3: H1–H3 + demos P6 no QEMU (s302/s305); metal T-051–T-057 ▶️
- [ ] **Onda 7** — W2A8 gated + 0078 só Fase 1 (T-058–T-065)
- [ ] **Onda 8** — Golden GPU/SDMA/NPU (T-066–T-069) ▶️
- [ ] **Onda 9** — 0058 S5 um widget + A/V (T-070–T-072)
- [ ] **Onda 10** — AirLLM DMA/e2e (T-073–T-075) ▶️

### 5. ADR-0094 — Hermes Cleanup ✅
**Goal:** dead code excluído + testes host nos módulos ativos.

- [x] Comentar 35 módulos mortos (7.446 LOC, -23% build)
- [x] 22 testes `cognitive_bridge` + 11 testes `memory_store`
- [ ] Residual: deletar arquivos mortos do disco (manual)
- [ ] Residual: 2 testes `wasm_build` + 1 `cognitive_bridge` falhando (ver "Saúde de testes")

### 6. ADR-0093 — Jarbas Optimization ✅
**Goal:** lock-free render + caps + dirty-rect + PT-BR TTS.

- [x] 45 host tests (`jarvis.rs`) — 3 `soul_*` falhando (ver "Saúde de testes")
- [x] Mouse/theme lock-free (Atomic)
- [x] Memory caps (NotificationQueue 32, ChatWindow 100/64)
- [x] HUD string cache + dirty-rect gating
- [x] 8 fonemas PT-BR no formant
- [x] Orb JARVIS hex-grid + anti-flicker (SESSION_291); UI liveness/anti-black-screen (SESSION_315)

### 7. ADR-0092 — Boot observability
**Goal:** 3 canais (dmesg/produto/placar) + `BOOT SCORE` + instrumentos FB.

- [x] **O0** — Contrato `sev` ok|warn|fail|trace + filtro consola
- [x] **O1** — Banner `=== PHASE n= ===` + fase 8 PostRuntime
- [x] **O2** — Mudos BPB/INIT1/e1000/SIPI/scan/PnP
- [x] **O3** — `BOOT SCORE` + `tools/parse_boot_score.py`
- [x] **O4** — Sem K* no ecrã; HUD produto
- [x] **O5** — Profile qemu vs hw no placar
- [x] **Instrumentos FB** — `diag_mark`/`diag_stamp_agent`/`diag_stamp_exception`/`tick_stage`/heartbeat `T=` (s318–s327)
- [ ] **Metal** — Canal A em `E:\BOOT.LOG` (🟡 = ADR-0103 S1)

### 8. ADR-0091 — Migração neural-sgdb ✅
**Goal:** neural-sgdb externo como substrato de memória cognitiva.

- [x] Fase 0 — Dependência no_std
- [x] Fase 1 — TickvStorageAdapter
- [x] Fase 2 — NSGDB bridge + fallback
- [x] Fase 2.5 — Hits tipados, embedder seam, lexical default, lifecycle, scoping, cognitive ops
- [x] Fase 3 — Cortex/Hermes memory-aware
- [x] Self-Heal closed-loop via NSGDB (SESSION_316)
- [ ] Residual: migrar 75 callers gradualmente

### 9. ADR-0090 — Jarbas Desktop v2.0
**Goal:** desktop production-grade (4 Tiers / 15 features).

- [x] **Tier 1** — Glyph cache + grid pre-render (LUT seno ✅ e dock ✅ já no tree; `cargo check -p jarbas` 0 erros, 57 testes)
- [x] **Soul/Emotion** — unificação `hermes` (Emotion, Soul, SER→Affect, LoopPhase) delegando em `jarbas` (SESSION_319)
- [ ] **Tier 2** — Window animations, chat scrollback, hover states, voice waveform (~12d)
- [ ] **Tier 3** — Per-window back buffers, desktop real (~20d)
- [ ] **Tier 4** — Transformacional (~30d)

### 10. ADR-0089 — Per-CPU Run-Queues SMP
**Goal:** distribuição cooperativa de agents entre cores (`smp-runqueue`).

- [x] Run-queue slot-based + steal min-1 + telemetria (código)
- [x] Feature `smp-runqueue` no `default` do bin + `MAX_CORES=256` (s307)
- [ ] Aceite metal K23: `online==madt-1` + hybrid P/E

---

## 🔗 DEPENDÊNCIAS (ordem obrigatória)

```
0100 Onda 2 (SMP metal) ──► 0102 T-052 (iretq metal) ──► 0103 Fase 2 (schemes)
        │
        └──► 0092/0089 aceite metal (0089 feature já ON)
```

## 🔴 BLOQUEIOS (não são trabalho de código)

| Item | ADR | O que falta |
|------|-----|-------------|
| S1 BOOT.LOG metal | 0103 | Operador: pendrive + `E:\BOOT.LOG` no Alienware |
| Flash `usb_hw.img` pós-s328 | — | Operador: Rufus (DD) + boot no Alienware |
| Onda 2 SMP metal | 0100 | Dois notebooks (i5 7ª + Core 7 240H), `online==madt-1` |
| T-052 iretq metal | 0102 | Depende de Onda 2 SMP metal |
| Fase 2 schemes | 0103 | Depende de 0102 aceite HW |

**Diagnóstico instrumentado (s316):** watchdog de tick lento (`[Sched] warn`), TSC visível (`ok`/`warn`), `cmd TIMEOUT ms=` — próximo boot no metal disambigua H1–H6. Checklist: `docs/memory/HW_DIAG_s316.md`.

## ✅ CONCLUÍDO PÓS-09-05 (rastreabilidade)

- s306 Dual QEMU 4c mesh Master/Worker + slog P2P visível
- s309 Falcon3 GGUF inferência wired (TQ2_0 + BF16 + auto-config)
- s315 Jarbas UI liveness + anti-black-screen (Alienware)
- s316 Self-Heal closed-loop + NSGDB + detectores de segurança; escada de instrumentos FB (s318→s327)
- s317 k_ai/cortex: ReAct + H2O + CodebookVQ
- s318 hermes/cortex FASE 1–4 (KvCache, MoE 0.05, dead code)
- s319 hermes/jarbas unificação (Emotion, Soul, SER→Affect, LoopPhase)
- s321 k_nano slimming FASE A–D + facades
- s327 freeze bisector + MSC Port Power
- s328 Full Infer D+B+C (InferQueue WS-H; ADR-0057)

## 🧾 DÍVIDA DE DOC/CONSISTÊNCIA

- [ ] Licença: `LICENSE` = **AGPL-3.0**, mas `TECNOLOGIAS.md` declara "código próprio MIT" — decidir a correta e alinhar
- [x] Métricas alinhadas ao **medido** em `AGENTS.md`, `SUMMARY.md`, `ROADMAP.md`, `codemap.md`, `HOWTO.md` (+ `TECNOLOGIAS.md`/`README.md`): ~148K LOC / ~671 `.rs` (12 crates do workspace) / 41 nativos / v1.9.99-s332 / 829 testes host

---

**Detalhes completos:** `docs/architecture/0100-k3chj-backlog-custo-anel.md` (T-001–T-075) · `AGENTS.md` · `docs/architecture/INDEX.md`
