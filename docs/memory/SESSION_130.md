# SESSION_130 — Fix pontual: SkillMarket ranking total_cmp

**Data:** 2026-07-16  
**Versão:** v1.8.5 TEST (pós)  
**ADR:** — (fix pontual; GOVERNANCE Regra A)  
**IDEA:** —

## Objetivo

Eliminar residual de panico em ordenação de scores float em
`crates/hermes/src/wasm_rt.rs` (`SkillMarket::top`), alinhando ao padrão já
usado em `hermes::skill_market`.

## Mudanças

| Arquivo | Alteração |
|---------|-----------|
| `crates/hermes/src/wasm_rt.rs` | `partial_cmp(...).unwrap()` → `total_cmp`; testes unitários de ordem e NaN |
| `CHANGELOG.md` | entrada sob `[Unreleased]` |
| `docs/memory/SESSION_130.md` | esta sessão |
| `docs/memory/SESSION_INDEX.md` | linha SESSION_130 |

## Espelhos / DRY (K²CHJ)

- Truth: `hermes` (N4.6 removeu espelho monólito `wasm_rt`).
- `neural-kernel` não possui `wasm_rt.rs` ativo.
- `LEGACY/v1.5-neural-kernel-src/wasm_rt.rs` é snapshot histórico — **não** editado.
- Padrão canônico já existente: `crates/hermes/src/skill_market.rs` (`total_cmp`).

## Evidência

| Check | Resultado |
|-------|-----------|
| `cargo check -p hermes --release --target-dir target/check-wasm-rt` | ✅ 0 erros (warnings pré-existentes) |
| `cargo clean -p neural-kernel && cargo check --release -p neural-kernel --target x86_64-unknown-none --target-dir target/check-wasm-rt-nk` | ✅ 0 erros |
| `cargo test -p hermes --lib skill_market_sort_tests` (host) | ❌ bloqueado: `k-nano` host → `offset is not a multiple of 16` (soft-float / N1.0) |
| `cargo test … --target x86_64-unknown-none --no-run` | ❌ `can't find crate for test` (esperado em bare-metal) |

Testes unitários em `wasm_rt.rs` documentam o contrato (ordem desc + NaN sem panic); execução host/none permanece limitada pelo toolchain do kernel — validação efetiva = `cargo check --release`.

## Limites

- Não toca outros `partial_cmp` no workspace (cortex, jarbas, vfs, etc.) — fora do residual confirmado.
- Não introduz tecnologia nova; TECNOLOGIAS.md sem alteração.
- Unit tests não executáveis neste ambiente bare-metal (mesma restrição de outros `#[test]` em hermes).

## Próximo

- Opcional: varrer outros `partial_cmp(...).unwrap()` remanescentes em sprints de polish.
