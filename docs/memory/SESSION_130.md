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
| `crates/jarbas/src/display/fb.rs` | `GpuDevice::from_probe` + `resolve_bytes_per_pixel` + `DoubleBuffer::from_gpu` — bpp dinâmico do GOP |
| `crates/jarbas/src/display/agent.rs` | DisplayAgent usa `DoubleBuffer::from_gpu` |
| `crates/jarbas/src/display/avatar.rs` / `virtio_gpu.rs` | consumers via helpers; VirtIO bpp=4 = protocolo |
| `crates/neural-kernel/src/vga_buffer.rs` | console FB lê `bytes_per_pixel()`/`stride_bytes()` |
| `crates/hermes/src/{mcp,wasm_rt,micropython_wasm}.rs` | imports sem uso removidos; braço `fd_write` duplicado/inalcançável removido |
| `crates/jarbas/src/display/compositor.rs` | braço `WasmSkill` duplicado/inalcançável removido |
| `crates/neural-kernel/src/main.rs` | import `Receiver` sem uso removido |
| `crates/boot/build.rs` | mensagem informativa deixou de usar o canal `cargo:warning` |
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
| `cargo clean -p neural-kernel && cargo check --release` (`CARGO_TARGET_DIR=target/check-zero-warn`) | ✅ 0 erros, 0 warnings |

Testes unitários em `wasm_rt.rs` documentam o contrato (ordem desc + NaN sem panic); execução host/none permanece limitada pelo toolchain do kernel — validação efetiva = `cargo check --release`.

## Limites

- Não toca outros `partial_cmp` no workspace (cortex, jarbas, vfs, etc.) — fora do residual confirmado.
- Não introduz tecnologia nova; TECNOLOGIAS.md sem alteração.
- Unit tests não executáveis neste ambiente bare-metal (mesma restrição de outros `#[test]` em hermes).

## Próximo

- Opcional: varrer outros `partial_cmp(...).unwrap()` remanescentes em sprints de polish.
