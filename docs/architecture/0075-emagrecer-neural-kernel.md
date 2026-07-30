# ADR-0075: Emagrecer neural-kernel — relatório pós-execução + revisão

**Data original:** 2026-07-23
**Revisão:** 2026-07-29 — SESSION_217 executada, diagnóstico pós-fato
**Status:** Em implementação — E0 parcial, E1a/E1c/E2/E3/E4 concluídos, E1b pendente
**Lifecycle (INDEX):** `fazendo`
**Estende:** ADR-0042 (K³CHJ workspace), ADR-0062 (ClaudioOS), IDEA #467/#511
**Base:** v1.9.9 TEST → v1.9.11-emagrecer
**Sessões:** SESSION_215 (plano), SESSION_217 (execução E1a/E1c/E4), SESSION_163 (ondas 0-6)

---

## 1. Contexto e problema (revisado)

O crate `neural-kernel` (bin) carrega **~20.430 LOC em 136 arquivos** `.rs`. Destes, **~4.943 LOC (24%)** ainda são `bin_ahead` — módulos que têm versão canônica em crates K³CHJ mas não foram stubados/promovidos. O produto cognitivo vive nos crates (~68k LOC K³CHJ), e o bin historicamente duplicava caminhos críticos; ~8.998 LOC já foram removidos por cutover.

### 1.1 Estado atual (Jul 2026)

| Classe | LOC | % | Descrição |
|--------|----:|---|-----------|
| bin_ahead | ~3.971 | 19% | Módulos no bin que têm versão canônica melhor nos crates (agents, shutdown, boot_log_agent, memory_agent, aios_api, vga_buffer) |
| role_diff | ~4.500 | 22% | Só o bin pode ter: net stack real (751), smp AP worker (421), HW state, page tables, IRQs, boot_handoff |
| glue | ~5.500 | 27% | main.rs, IDT, GDT, allocator, shell, PCI init, APIC init — não migrável |
| já cutover (stubs + crate_ahead) | ~6.500 | 32% | 34 módulos stub + 12 crate_ahead que podem virar pub use — já redirecionados para crates |

**Mudanças desde o plano original:**
- audio (antes ~2.900 LOC, 10%) → re-export de jarbas (4 LOC) — E4 concluído
- bin_ahead (antes ~12.000 LOC, 41%) → ~3.971 LOC (19%) — redução de ~67%
- stubs (antes ~100 LOC) → ~6.500 LOC (32%) — cutover consolidado

### 1.2 Problemas conhecidos (atualizado)

| # | Problema | Status |
|---|----------|--------|
| 1 | **Dual-source LLM** — `neural-kernel/src/cortex.rs` vs `crates/cortex/src/cortex.rs` | ✅ Resolvido. Bin é stub (182 LOC, só boot path); crate é truth (2.892 LOC) |
| 2 | **Dual-source fleet** — `agents.rs` bin vs hermes crate | 🟡 Aberto. bin=2.620 LOC, crate=2.484 LOC. Drift: +136 LOC, 20 impls Agent |
| 3 | **Código órfão** — `bpe.rs`, `gguf.rs` no bin sem crate | ✅ Resolvido. Ambos stub/promovidos |
| 4 | **Net stack role_diff** — `net.rs` + `netstack.rs` + `network_agent.rs` (~2.458 LOC) | 🟡 Mantido. `net.rs` (751 LOC) é role_diff — bin tem stack real (smoltcp) |
| 5 | **Audio truth no bin** — `audio/*` (~2.900 LOC, 21 arquivos) | ✅ Resolvido. Tudo via `pub use jarbas_crate::audio::*` |

---

## 2. O que foi feito

### 2.1 Ondas 0–6 (prévias, SESSION_163)

- 40 stubs cutover: `identity.rs`, `memory.rs`, `mhi.rs`, `sync/`, `gpt.rs`, `exfat.rs`, `tpm.rs`, `hw_rng.rs`, `slip.rs`, `rtl8139.rs`, `ahci.rs`, `pci.rs`, `serial.rs`, `xhci.rs`, `usb_msc.rs`, `simd.rs`, `fat32.rs`, `ata.rs`, `e1000.rs`, `acpi.rs`, `apic.rs`, `interrupts.rs`, e mais.
- `diff_bin_crate.py` — ferramenta de diff criada.

### 2.2 E1a — cortex/bpe/gguf/gguf_streaming/model_hub → cortex crate ✅

| Ação | LOC removido | Resultado |
|------|-------------:|-----------|
| `bpe.rs` stub | 990 | ✅ stub (2 LOC) |
| `gguf.rs` stub | 895 | ✅ stub (2 LOC) |
| `gguf_streaming.rs` mantido (net-dependent) | 0 | ✅ mantido no bin |
| `model_hub.rs` stub | 264 | ✅ stub (2 LOC) |
| `cortex.rs` → thin boot path | 2.300 | ✅ só 182 LOC (vs 2.892 no crate) |
| **Total removido** | **~4.449** | |

### 2.3 E1b — agents/neural_fs/vfs/fs → hermes crate ❌ NÃO EXECUTADO

| Ação | Previsto | Status |
|------|----------|--------|
| agents.rs → hermes crate sync | 2.388 | ❌ bin ainda tem 2.620 LOC (divergência +136) |
| neural_fs → crate | 657 | 🟡 crate_ahead (809 bin vs 2.318 crate) |
| vfs/* → pub use | 256 | ❌ mantido (257 LOC) |
| fs/* → pub use | 149 | ❌ mantido (602 LOC) |

### 2.4 E1c — boot_logger/virtio_net/usb_msc → k_nano ✅

| Ação | LOC | Resultado |
|------|----:|-----------|
| `boot_logger.rs` → crate | 507 | ✅ crate_ahead (118 bin vs 395 crate) |
| `virtio_net.rs` stub | 408 | ✅ stub (3 LOC) |
| `usb_msc.rs` stub | 205 | ✅ removido do bin |
| **Total removido** | **~1.117** | |

### 2.5 E2 — BootHandoff trait ✅

`boot_handoff.rs` implementa wrapper de `Bootloader011Handoff` + trait `BootHandoff`. Não pode mover p/ crate (depende de `bootloader_api::BootInfo` — dep do bin). Mantido no bin.

### 2.6 E3 — GPU/WiFi wire k_hal ✅

`pub use jarbas_crate::{display, gpu, ...}` em main.rs. Wire completo.

### 2.7 E4 — Audio → jarbas crate ✅

- `audio/mod.rs` virou re-export: `pub use jarbas_crate::audio::*`
- `jarbas_bridge.rs` mantido para verificação de contract sync
- **~2.900 LOC removidos** do bin

### 2.8 Resumo

| Fase | Previsto | Real | Status |
|------|---------:|-----:|:------:|
| Ondas 0-6 | ~40 stubs | 40 stubs | ✅ |
| E1a | -5.206 | -4.449 | ✅ (gguf_streaming ficou) |
| E1b | -3.450 | 0 | ❌ |
| E1c | -1.120 | -1.117 | ✅ |
| E2 | +200/-50 | ±0 | ✅ (wrapper mantido) |
| E3 | +10 | +10 | ✅ |
| E4 | -2.900 | -2.900 | ✅ |
| **Total** | **-12.516** | **-8.998** | **~72% do plano** |

---

## 3. O que caducou / não se sustentou

### 3.1 E0 — Freeze CI gate (nunca implementado)

- A política de CI `diff_bin_crate.py --strict` **nunca foi integrada** a nenhum pipeline.
- O script estava quebrado (`parents[1]` em vez de `parents[3]` no path do ROOT) até 2026-07-29.
- O arquivo `.cursor/rules/neural-emagrecer-bin.mdc` **não existe mais** — perdido em checkpoint/merge.

### 3.2 Alvo 16.890 LOC → 20.430 LOC

O alvo de ~16.890 LOC (pós-E4) não foi atingido porque:
- Crescimento orgânico: main.rs cresceu (novos agents, bridges, boot paths)
- Glue aumentou (SMP AP worker, async executor, isolation ring, etc.)
- O piso real (~11.600 glue+role_diff) sempre foi subestimado — só glue+role_diff hoje soma ~10.000 LOC

### 3.3 Ordem forte E0→E1a→E1b→E1c→E2→E3→E4

A execução real foi E1a+E1c+E4 em paralelo (SESSION_217), E2 e E3 já existiam independentemente. A ordem forte não se sustentou na prática — cada fase era mais independente do que o plano supunha.

---

## 4. O que ainda pode ser feito (~3.771 LOC recuperáveis)

### 4.1 Módulos bin_ahead — promover conteúdo duplicado

Onde o bin tem uma cópia que diverge do crate. Ação: sync drift + stub.

| Módulo | LOC bin | LOC crate | Diferença | Esforço | Risco |
|--------|--------:|----------:|----------:|---------|:-----:|
| **`agents.rs`** | 2.620 | 2.484 | +136 | Alto (~4h) | 🟡 20 impls Agent; boot path complexo |
| **`shutdown.rs`** | 360 | 233 | +127 (parcial) | Médio | 🟡 split deliberado (HW exec no bin, cause/arm no crate) |
| **`boot_log_agent.rs`** | 118 | 147 | stub compatível | Baixo | 🟢 ~118 LOC de FAT walk; crate tem versão maior (147) |
| **`memory_agent.rs`** | 193 | 189 | +4 | Baixo | 🟢 divergência mínima |
| **`aios_api.rs`** | 92 | 83 | +9 | Baixo | 🟢 quase idêntico |
| **`vga_buffer.rs`** | 216 | 204 | +12 (macros) | Médio | 🟡 macros bin-specific precisam port p/ crate |
| **Total** | **~3.599** | | | | |

### 4.2 Módulos crate_ahead (já efetivamente stubs)

12 módulos onde o crate tem mais código que o bin. Em geral já são stubs (3-4 LOC de `pub use`) ou thin wrappers legítimos. **Não vale o esforço de mexer:**

- Já stubs/triviais (3-4 LOC): `gpt.rs`, `exfat_write.rs`, `dma.rs`, `io_scheduler.rs`, `storage_manager.rs`, `chunker.rs`, `context_window.rs`, `training_agent.rs` ✅
- Thin wrapper justificado: `neural_fs/` (809 LOC no bin, mas 2.318 no crate — bin tem o agent que consome o crate), `interrupts.rs` (441 LOC — role_diff legítimo), `cortex.rs` (182 LOC — boot path), `boot_logger.rs` (118 LOC — FAT walk, crate maior)

### 4.3 Missing_bin (já removidos, verificar stubs)

| Módulo | LOC crate | Ação |
|--------|----------:|------|
| `sync/` | 234 | Já movido p/ k_nano; bin usa via dep |
| `slab.rs` | 152 | Já movido p/ k_nano |
| `usb_msc.rs` | 239 | Já movido p/ k_nano |

---

## 5. O que NÃO pode ser feito

### 5.1 Role-diff estrutural (~4.500 LOC)

| Módulo | LOC | Motivo |
|--------|----:|--------|
| `net.rs` | 751 | Bin tem stack real (smoltcp + HTTP + DNS raw + TLS). `k_nano` só tem nic_globals (7 LOC). Unificação exigiria refatorar smoltcp p/ dentro da crate — decisão adiada. |
| `smp/` (parallel_matmul.rs) | ~200 | AP worker acoplado ao gating `ap_pollable()` do bin. Mover quebraria dependência circular. |
| `boot_handoff.rs` | ~150 | Depende de `bootloader_api::BootInfo` (dep do bin, não da crate). |
| `interrupts.rs`, `apic.rs` | ~1.100 | Role_diff legítimo — HW state, page tables, IRQ handlers não migram. |

### 5.2 Glue (~5.500 LOC)

main.rs (boot sequence, agent registration, integração), IDT, GDT, allocator, shell, PCI scan, APIC init — tudo que cola as crates. **Não emagrece além de fatoração marginal.**

### 5.3 Piso mínimo

glue (5.500) + role_diff (4.500) = **~10.000 LOC irreduzíveis**. O bin nunca será mais magro que isso.

---

## 6. Anti-padrões (atualizado)

1. ~~Portar ClaudioOS P2–P7 para dentro do bin~~ → Nunca ocorreu. ✅
2. ~~Limine no meio de promote E1~~ → E2 já estava independente. ✅
3. **Stub cego de net.rs** — role_diff mantido. Decisão correta. ✅
4. **Copiar AGPL ClaudioOS kernel** — Não ocorreu. ✅
5. **Ampliar bin_ahead sem promover primeiro** — Ocorreu parcialmente (agents.rs e main.rs cresceram). ⚠️ E0 não gateou.

---

## 7. Alvo realista revisado

| Cenário | LOC bin | Redução |
|---------|--------:|--------:|
| Atual | 20.430 | — |
| Pós-promover agents.rs + boot_log_agent + memory_agent + aios_api | ~17.400 | −3.000 |
| Pós-sincronizar shutdown.rs (parcial) + vga_buffer.rs | ~16.850 | −550 |
| **Alvo realista** | **~16.500–17.500** | **(após promover bin_ahead restante)** |
| **Piso intransponível** | **~10.000** | **(glue + role_diff)** |

---

## 8. Gate checklist revisado

1. `python docs/archive/migration/diff_bin_crate.py --onda N` — sem `bin_ahead` nos alvos
2. `cargo clean -p neural-kernel && cargo nk` = 0 erros
3. Boot WHPX: 8 fases + `[TIMER] tick=`
4. Rollback: 1 commit por promoção, `git revert` se gate falha
5. **Novo:** Todo PR que adiciona >200 LOC ao bin deve incluir justificativa por que não pode ir para crate

---

## 9. Referências

- SESSION_215 — análise profunda + plano E0–E4 original
- SESSION_217 — execução E1a/E1c/E4 + P001 + boot path + checkpoint
- SESSION_163 — emagrecer ondas 0-6 + diff_bin_crate
- BIN_CRATE_DIFF.md — diff tool output (pré-SESSION_217)
- `diff_bin_crate.py` — ferramenta de diff (corrigida 2026-07-29)
- AGENTS.md — plano diretor K³CHJ
- IDEA #467 / #511 — emagrecer neural-kernel
