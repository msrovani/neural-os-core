# ADR-0075: Emagrecer neural-kernel — cutover cirúrgico bin→crates

**Data:** 2026-07-23
**Status:** Proposed — plano aprovado, E0 (freeze) em execução.
**Lifecycle (INDEX):** `fazendo`
**Estende:** ADR-0042 (K³CHJ workspace), ADR-0062 (ClaudioOS), IDEA #467/#511
**Base:** v1.9.9 TEST (a216ca8), checkpoint sprints 178-214 (4d8f0d5)
**SESSION:** SESSION_215

---

## 1. Contexto e problema

O crate `neural-kernel` (bin) carrega ~29.431 LOC em ~150 arquivos. Destes, **~12.000 LOC (41%)** são `bin_ahead` — módulos que já têm versão canônica em crates K³CHJ mas não estão wired. O produto cognitivo já vive nos crates (~68k LOC K³CHJ), e o bin duplica caminhos críticos (LLM, fleet, net, audio, FS) que divergem silenciosamente.

### 1.1 Estado atual

| Classe | LOC | % | Descrição |
|--------|----:|---|-----------|
| bin_ahead | ~12.000 | 41% | Módulos que existem no bin e também (melhor) nos crates |
| role_diff | ~6.500 | 22% | Módulos que só o bin pode ter (HW state, page tables, IRQs) |
| glue | ~5.000 | 17% | main.rs, interrupt handlers, global_allocator, shell |
| audio | ~2.900 | 10% | Truth no bin per ADR-0045; deferido para E4 |
| stubs | ~100 | 0,3% | pub use puro — já cutover |

### 1.2 Problemas conhecidos

1. **Dual-source LLM:** `crates/neural-kernel/src/cortex.rs` (versão antiga, VOCAB=99) vs `crates/cortex/src/cortex.rs` (canônica, AirLLM+FlashAttention, 2.463 LOC). Divergência silenciosa.
2. **Dual-source fleet:** `crates/neural-kernel/src/agents.rs` (versão antiga, 2.388 LOC) vs `crates/hermes/src/agents.rs` (canônica, 2.332 LOC).
3. **Código órfão:** `bpe.rs`, `gguf.rs`, `gguf_streaming.rs` só existem no bin — crates não têm.
4. **Net stack role_diff:** `net.rs` + `netstack.rs` + `network_agent.rs` (~2.458 LOC) só existem no bin com versão rica; crates têm subset.
5. **Audio truth no bin:** `audio/*` (~2.900 LOC, 21 arquivos) — mirror no jarbas crate per ADR-0045.

## 2. Solução proposta

Cutover cirúrgico em 5 fases (E0–E4), migrando código legado para `LEGACY/`, nunca apagando nada.

### 2.1 Princípios

1. **Promover antes de stub:** código `bin_ahead` é movido para o crate antes de substituir por `pub use`.
2. **Zero perda:** código migrado vai para `LEGACY/` — snapshot preservado.
3. **1 commit por wave:** rollback é `git revert`.
4. **Gate universal:** `cargo nk` 0 erros + boot 8 fases + `[TIMER] tick=` + `diff_bin_crate.py limpo`.
5. **Ordem forte:** E0 → E1a → E1b → E1c → E2 → E3 → E4. Paralelo só com handoff trait.

### 2.2 E0 — Freeze (imediato)

- PR policy: módulos >200 LOC novos no bin exigem ADR.
- CI gate: `python docs/archive/migration/diff_bin_crate.py --strict`.
- Nenhum módulo `bin_ahead` pode ser ampliado sem promover primeiro.

### 2.3 E1a — Wave 7a: cortex/bpe/gguf/gguf_streaming/model_hub → cortex crate

| Ação | LOC migrado | LOC removido do bin | Risco |
|------|-----------:|-------------------:|------:|
| Mover bpe.rs → cortex crate | 990 | 990 | 🔴 API incompatível |
| Mover gguf.rs → cortex crate | 895 | 895 | 🔴 crate não tem |
| Mover gguf_streaming.rs → cortex crate | 757 | 757 | 🔴 crate não tem |
| Stub cortex.rs → pub use | 0 | 2.300 | 🔴 API diferente |
| Stub model_hub.rs → pub use | 0 | 264 | 🟡 duplicado |
| **Total** | **2.642** | **5.206** | |

### 2.4 E1b — Wave 7b: agents/neural_fs/vfs/fs → hermes crate

| Ação | LOC migrado | LOC removido do bin | Risco |
|------|-----------:|-------------------:|------:|
| Verificar/corrigir lista agentes (25×25) | ~56 | 2.388 | 🔴 |
| Promover neural_fs/agent → hermes | 657 | 657 | 🟡 |
| Stub vfs/* → pub use | 0 | 256 | 🟡 |
| Stub fs/* → pub use | 0 | 149 | 🟡 |
| **Total** | **713** | **3.450** | |

### 2.5 E1c — Wave 7c: boot_logger/virtio_net/usb_msc → k_nano crate

| Ação | LOC migrado | LOC removido do bin | Risco |
|------|-----------:|-------------------:|------:|
| Promover boot_logger.rs → k_nano | 507 | 507 | 🟡 |
| Stub virtio_net.rs → pub use | 0 | 408 | 🟡 |
| Stub usb_msc.rs → pub use | 0 | 205 | 🟢 |
| **Total** | **507** | **1.120** | |

### 2.6 E2 — Limine handoff trait (P1)

- Criar trait `BootHandoff` com métodos: `physical_memory_offset()`, `framebuffer()`, `rsdp()`.
- Implementar `Bootloader011Handoff` + `LimineHandoff`.
- Refatorar `kernel_boot()` para aceitar `dyn BootHandoff`.
- Feature `limine-boot` já existe como gated (`limine_boot.rs`).
- **Pré-requisito:** E1 completo.

### 2.7 E3 — Infra ADR-0062 nos crates (P2/P3/P5/P6)

- P2 VFS, P3 AHCI/NVMe, P5 GPU, P6 WiFi: todos já nos crates.
- Só wire `pub use` no bin (~10 LOC).

### 2.8 E4 — Audio cutover (P2, ADR-0045 revisado)

- Mover `audio/*` (21 arquivos, ~2.900 LOC) → jarbas crate.
- Atualizar imports, verificar contract sync (`jarbas_bridge.rs`).
- **Pré-requisito:** ADR-0045 revisado e aprovado.

## 3. Alvo final

| Fase | Bin LOC | Acumulado removido |
|------|--------:|-------------------:|
| Hoje | 29.431 | 0 |
| Pós-E1a | ~24.200 | 5.206 |
| Pós-E1b | ~20.750 | 8.656 |
| Pós-E1c | ~19.630 | 9.776 |
| Pós-E2 | ~19.780 | 9.726 (+200 Limine adapter) |
| Pós-E3 | ~19.790 | 9.716 (+10 wire) |
| Pós-E4 | ~16.890 | 12.616 (com audio) |

**Alvo realista mínimo:** **~11.000 LOC** sem audio (~16.890 - 2.900 audio - 3.000 main.rs encolhido + stubs).
**O dashboard diz 3-5k:** inviável — main.rs (3.102) + glue (2.000) + role_diff (6.500) = 11.600 LOC mínimo.

## 4. Anti-padrões

1. **Portar ClaudioOS P2–P7 para dentro do bin** — engorda o monólito. Implementar só nos crates.
2. **Limine no meio de promote cortex/agents** — dois refactors no mesmo caminho de boot. Isolar handoff OU terminar E1 antes.
3. **Stub cego de net.rs** — role_diff: bin=stack, k_nano=nic_globals. Manter bridge; promover stack p/ hermes se um dia unificar.
4. **Copiar AGPL ClaudioOS kernel** — preferir crates MIT/Apache publicadas.

## 5. Gate checklist (por wave)

1. `python docs/archive/migration/diff_bin_crate.py --onda N --strict` — sem `bin_ahead` nos alvos
2. `cargo clean -p neural-kernel && cargo nk` = 0 erros
3. Boot WHPX: 8 fases + `[TIMER] tick=`
4. Rollback: 1 commit atômico por wave, `git revert` se gate falha

## 6. Rollback path

| Wave | Rollback | Verificação |
|------|----------|-------------|
| E0 | git revert policy | CI gate |
| E1a | git revert cortex/bpe/gguf | boot LLM carrega modelo |
| E1b | git revert agents/neural_fs | AgentFleet 25 agentes |
| E1c | git revert boot_logger/virtio_net/usb_msc | BOOT.LOG flush |
| E2 | git revert Limine adapter | boot 0.11 OK |
| E3 | git revert GPU/WiFi wire | HW detect |
| E4 | git revert audio move | audio init |

## 7. Referências

- SESSION_215 — análise profunda + dados do recon
- SESSION_163 — emagrecer ondas 0-6
- IDEA #467 / #511 — emagrecer neural-kernel
- BIN_CRATE_DIFF.md — diff tool output
- AGENTS.md — plano diretor K³CHJ
- .cursor/rules/neural-emagrecer-bin.mdc — regra de emagrecimento
