# SESSION_329 — Doc reconciliation: top-level docs + ADR INDEX + archive de legados (2026-09-10)

**Objetivo:** reconciliar a documentação de topo e o índice de ADRs com o estado real do tree (pós-s328), removendo drift de métricas/versões, corrigindo erros factuais e arquivando documentos legados em conflito de ID.

**Escopo:** docs only. Nenhuma linha de código de kernel alterada.

---

## 1. Medição do tree (fonte da verdade)

| Métrica | Doc antigo (drift) | Medido no repo |
|---|---|---|
| LOC Rust (12 crates do workspace) | ~26.000 | **~148.000** |
| Arquivos `.rs` (workspace) | 180+ | **~671** |
| Agentes nativos | ~50 | **41** seeds (`skills/agents/*/SKILL.md`) |
| Versão | v1.9.5 / v1.9.7 | **v1.9.99-s328** |
| Testes host | 168 | **784 pass / 6 fail** |
| ADRs (`docs/architecture/`) | 47+ | **~100** |

Comandos: `Cargo.toml` (members) · contagem `.rs`+LOC · `skills/agents/` · `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast` (com fixture `python tools/gen_test_gguf.py`).

## 2. Correções factuais

- **Falcon3-3B** — README dizia `30 layers / hidden 2560`; canônico (`tools/convert_falcon3_to_v6.py`) = **22 layers / hidden 3072 / 12 heads / 4 KV / intermediate 9216 / vocab 131072**.
- **Boot** — docs citavam crate `bootloader` 0.11; o tree usa **Limine** (`tools/limine`, `crates/boot/build.rs`), crate removida (SESSION_232).
- **Agentes** — "25 nativos" → 41 seeds; "~147/214 Agency specialists" era hardcoded → hoje **data-driven** via PackageHub (AGENT.md assinados).
- **GPU compute** — "validated" → PUSH_BUFFER submit HW-real, mas kernels ternários **W2A8 pendentes** (KernelPack); matmul é CPU até então.
- **Ring3** — "triple-fault / gated off" → **Onda 6 wired** (ADR-0102, demos P6 no QEMU, faltas non-fatal); registro nativo continua gated (metal-only).
- **10.1 SDIO** — 171.003 **strings raw** / ~16.126 únicos / ~1.005 `(vid,did)`.
- **IDs duplicados** em TECNOLOGIAS → `2.10`(ACPI)=2.26, `2.10g`(Adequação)=2.27, `9.4`(DHCP)=9.6; linha `1.8` reordenada.

## 3. Arquivos atualizados

| Arquivo | Mudança |
|---|---|
| `TECNOLOGIAS.md` | métricas, versão, boot=Limine, agência, IDs, SDIO, +7.10 InferQueue |
| `README.md` | Falcon3 22L/3072, 41 seeds/Agency data-driven, testes 784/6, Ring3, GPU honesto, +InferQueue |
| `TODO.md` | reconciliado c/ s328: abertos no metal, saúde de testes/CI, pós-s328, ADR-0103 A–H, 0089 feature ON, dívida de doc |
| `docs/architecture/INDEX.md` | +0090/0091/0093/0094, `0076` plano, vazios 0095–0099, lacuna 0066–0073, Lote 2 de arquivados |
| `docs/architecture/0102-*.md` | referências dos legados → `docs/archive/notes/` |
| `docs/memory/SESSION_243.md` | nota de movimentação (registro histórico preservado) |
| `AGENTS.md` / `IDEA_BANK.md` / `STATE.md` / `SESSION_INDEX.md` | lições/ideias/estado desta sessão |

## 4. Archive de legados (`git mv`, histórico preservado)

`docs/architecture/` → `docs/archive/notes/`:
- `0060-ring3-isolation-ring.md` (cópia histórica → ADR-0077)
- `0082-ring3-isolation-production.md` + `0082-ring3-isolation-registry.md` (checklists conflito_id → ADR-0077; 0082 canônico = HardwareInfo)
- `PLAN_KAI_CORTEX_FIXES.md` (executado s317/318)

`docs/architecture/`: 100 → **96 arquivos**. Única menção ao path antigo restante = linha histórica da SESSION_243, já anotada.

## 5. Achados abertos (não codificados nesta sessão)

1. **6 testes host falhando** (pré-existentes): `hermes::wasm_build::{compile_and_run_real_skill,dsl_print_cmp}`, `hermes::cognitive_bridge::session_load_respects_cap`, `jarbas::jarvis::soul_{describe,default_jarbas,fluid_update_joy}`.
2. **Fixture** `cortex` (`target/test_tq2_0.gguf`) não é gerada no CI — `cargo test --workspace` sem ela não compila o teste de integração.
3. **Licença inconsistente:** `LICENSE`=AGPL-3.0 e README=AGPL, mas `TECNOLOGIAS.md` §15 declara "código próprio MIT". **Pendente decisão do maintainer** (texto de licença não alterado).
4. **Métricas velhas remanescentes** em `SUMMARY.md`, `ROADMAP.md`, `codemap.md`, `HOWTO.md` (os 3 top corrigidos: TECNOLOGIAS/README/TODO).

## 6. Lições

- **Doc não é fonte — medir o repo.** Todo número de doc envelhece; a evidência é `Cargo.toml`/contagem/`skills`/testes.
- **INDEX de ADRs é inventário vivo:** ADR nova = linha nova; legados/`conflito_id` → archive via `git mv` + atualizar INDEX e quem os cita (0102).
- **Test suite:** `--no-fail-fast` é obrigatório p/ ver todas as suítes; integração `cortex` precisa da fixture.
