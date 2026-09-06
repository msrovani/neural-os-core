# 📋 TODO — neural-os-core

**Versão:** v1.9.99 TEST
**Data:** 2026-09-05
**Fonte:** ADRs 0089–0103 (`docs/architecture/`)
**Legenda:** ✅ feito | 🟡 em andamento | 🔴 bloqueado | ⏳ agendado | ▶️ AWAITING_HW | `[ ]` pendente

---

## 🎯 OBJETIVOS

1. **Gate v2.0.0** — fechar ADR-0100 Ondas 0–3 (honesty, I/O, SMP metal, OTA) + review formal + OK maintainer.
2. **Emagrecer `k_nano`** — primitivos R0 + hooks; política no anel certo (ADR-0103 Fase 1).
3. **Sandbox Ring3 CPL=3** — um sandbox para blob nativo B/C; wasmi default até T-055 (ADR-0102).
4. **Kernel ternário-nativo** — Falcon3-3B 1.58-bit com ADD/SUB/SKIP packed no metal (ADR-0101 Onda 0).
5. **Observabilidade de boot** — 3 canais + placar `BOOT SCORE` parseável (ADR-0092).
6. **Desktop Jarbas v2** — production-grade (ADR-0090).
7. **SMP per-CPU runqueue** — distribuição cooperativa de agents (ADR-0089).

---

## ITENS (priorizados: mais nova → mais antiga)

### 1. ADR-0103 — k_nano microkernel modular (Fase 1)
**Goal:** `k_nano` = primitivos R0 + hooks; política em `k_hal`/`k_ai`/`cortex`/`hermes`/`jarbas`. Sem virar process-OS.

- [x] **S0** — Dedupe `multi_user`→k_ai, `hnsw`→cortex (`cargo check --release` 0 erros)
- [ ] **S1** — `k_hal::usb` hub→MSC + early `probe_and_install` → `E:\BOOT.LOG` real (🟡 código wired; ▶️ AWAITING_OPERATOR)
- [ ] **S2** — Drivers "política em k_hal, MMIO em k_nano" (NIC status/offer)
- [ ] **S3** — Leitores FS não-boot órfãos (ntfs/btrfs/ext2 read-only) → crate ou delete
- [ ] **S4** — Storage cognitivo (tickv FE, rollback UI) → k_ai/hermes
- [ ] **S5** — Podar exports mortos + `check_duplication.py` limpo
- [ ] **S6** — (Opcional) esqueleto `arch/memory/scheduler/` sem mover lógica
- [ ] **Fase 2** — Schemes CPL=3 (🔴 gated: ADR-0102 aceite HW)

### 2. ADR-0102 — Ring3 sandbox CPL=3
**Goal:** um sandbox CPL=3 para blob B/C; `isolation_ring_available()==false` ⇒ só wasmi.

- [x] **H1** — Feature `ring3 = ["k-nano/ring3"]` propaga no bin
- [x] **H2** — Demos P6 reais no tree (`demo_ring3*` em `k_nano::paging`; SESSION_302)
- [x] **H3** — `ring3_can_iretq()` + `can_register_native()` cindidos + self-test wired no boot
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
- [ ] **Onda 0** — AVX2 no target `none` (defer explícito da ADR; metal→scalar documentado, não reescrever ainda)
- [ ] **Onda 0** — Inventário FAT 3B-first (`falcon3_boot_names`, `fat_names_for(Active)`)
- [ ] **Onda 1** — Shortlist logits + n-gram/Medusa wired no 3B + KV H2O medido
- [ ] **Onda 2** — Difficulty gate no `generate_next`; early-exit só com KL treinado
- [ ] **Onda 3** — Composição: gate + spec + kernel nativo + KV compress + SGDB

### 4. ADR-0100 — Backlog unificado K³CHJ (plano-mestre)
**Goal:** gate v2.0.0 = Onda 0 + Onda 1 (mín. T-011) + Onda 2 (um metal) + Onda 3 (A2 ou A3).

- [x] **Onda 0** — Honesty `BOOT_AI` + freeze HardwareInfo (T-001–T-006)
- [x] **Onda 1** — `measure_bandwidth` + `/hw/storage|gpu|net` (T-007–T-016)
- [ ] **Onda 2** — Metal K23 `online==madt-1` (T-017 img ✅; T-018–T-021 ▶️ metal)
- [ ] **Onda 3** — 0086 A2–A8 (T-022–T-032; A1/A9 HITL)
- [ ] **Onda 4** — `ap_pollable` + runqueue 0089 (T-033–T-044)
- [ ] **Onda 5** — Mesh 2c + CRDT (T-045–T-050)
- [ ] **Onda 6** — Ring3 HW + `register_native_ring` + PIN_DMA (T-051–T-057)
- [ ] **Onda 7** — W2A8 gated + 0078 só Fase 1 (T-058–T-065)
- [ ] **Onda 8** — Golden GPU/SDMA/NPU (T-066–T-069) ▶️
- [ ] **Onda 9** — 0058 S5 um widget + A/V (T-070–T-072)
- [ ] **Onda 10** — AirLLM DMA/e2e (T-073–T-075) ▶️

### 5. ADR-0094 — Hermes Cleanup ✅
**Goal:** dead code excluído + testes host nos módulos ativos.

- [x] Comentar 35 módulos mortos (7.446 LOC, -23% build)
- [x] 22 testes `cognitive_bridge` + 11 testes `memory_store`
- [ ] Residual: deletar arquivos mortos do disco (manual)

### 6. ADR-0093 — Jarbas Optimization ✅
**Goal:** lock-free render + caps + dirty-rect + PT-BR TTS.

- [x] 45 host tests (`jarvis.rs`)
- [x] Mouse/theme lock-free (Atomic)
- [x] Memory caps (NotificationQueue 32, ChatWindow 100/64)
- [x] HUD string cache + dirty-rect gating
- [x] 8 fonemas PT-BR no formant

### 7. ADR-0092 — Boot observability
**Goal:** 3 canais (dmesg/produto/placar) + `BOOT SCORE` parseável.

- [x] **O0** — Contrato `sev` ok|warn|fail|trace + filtro consola
- [x] **O1** — Banner `=== PHASE n= ===` + fase 8 PostRuntime
- [x] **O2** — Mudos BPB/INIT1/e1000/SIPI/scan/PnP
- [x] **O3** — `BOOT SCORE` + `tools/parse_boot_score.py`
- [x] **O4** — Sem K* no ecrã; HUD produto
- [x] **O5** — Profile qemu vs hw no placar
- [ ] **Metal** — Canal A em `E:\BOOT.LOG` (🟡 = ADR-0103 S1)

### 8. ADR-0091 — Migração neural-sgdb ✅
**Goal:** neural-sgdb externo como substrato de memória cognitiva.

- [x] Fase 0 — Dependência no_std
- [x] Fase 1 — TickvStorageAdapter
- [x] Fase 2 — NSGDB bridge + fallback
- [x] Fase 2.5 — Hits tipados, embedder seam, lexical default, lifecycle, scoping, cognitive ops
- [x] Fase 3 — Cortex/Hermes memory-aware
- [ ] Residual: migrar 75 callers gradualmente

### 9. ADR-0090 — Jarbas Desktop v2.0
**Goal:** desktop production-grade (4 Tiers / 15 features).

- [x] **Tier 1** — Glyph cache + grid pre-render (LUT seno ✅ e dock ✅ já no tree; `cargo check -p jarbas` 0 erros, 57 testes)
- [ ] **Tier 2** — Window animations, chat scrollback, hover states, voice waveform (~12d)
- [ ] **Tier 3** — Per-window back buffers, desktop real (~20d)
- [ ] **Tier 4** — Transformacional (~30d)

### 10. ADR-0089 — Per-CPU Run-Queues SMP
**Goal:** distribuição cooperativa de agents entre cores (gated `smp-runqueue`).

- [x] Run-queue slot-based + steal min-1 + telemetria (código)
- [ ] Ligar feature no bin (🔴 depende Onda 4 → Onda 2 SMP metal)
- [ ] Aceite metal K23: `online==madt-1` + hybrid P/E

---

## 🔗 DEPENDÊNCIAS (ordem obrigatória)

```
0100 Onda 2 (SMP metal) ──► 0102 T-052 (iretq metal) ──► 0103 Fase 2 (schemes)
        │
        └──► 0100 Onda 4 (ap_pollable) ──► 0089 runqueue (feature ON)
```

## 🔴 BLOQUEIOS (não são trabalho de código)

| Item | ADR | O que falta |
|------|-----|-------------|
| S1 BOOT.LOG metal | 0103 | Operador: pendrive + `E:\BOOT.LOG` no Alienware |
| Onda 2 SMP metal | 0100 | Dois notebooks (i5 7ª + Core 7 240H), `online==madt-1` |
| T-052 iretq metal | 0102 | Depende de Onda 2 SMP metal |
| Fase 2 schemes | 0103 | Depende de 0102 aceite HW |

## ⚡ LANES PARALELAS (código, sem HW — rodam agora)

| Lane | ADR | Crate(s) | Escopo |
|------|-----|----------|--------|
| A | 0101 Onda 0 | `cortex` | Prova forward ternário-nativo + `FALCON3.V6` 3B-first |
| B | 0092 O0–O3 | `k_nano` slog/report + bin | Contrato `sev`, capitão+fase 8, placar |
| C | 0090 Tier 1 | `jarbas` | 4 quick wins de render |
| D | 0102 H2/H3 | `k_nano` paging + bin | Demos P6 reais + self-test `can_iretq` |
| E | 0100 Onda 0 | `k_ai` + bin | T-001..T-006 (métrica `BOOT_AI`) |

**Conflito de write:** nenhum (módulos distintos). B e D tocam `k_nano` em arquivos separados (`slog.rs`/`boot_report.rs` vs `paging.rs`); serializar se quiser zero risco.

---

**Detalhes completos:** `docs/architecture/0100-k3chj-backlog-custo-anel.md` (T-001–T-075) · `AGENTS.md` · `docs/architecture/INDEX.md`