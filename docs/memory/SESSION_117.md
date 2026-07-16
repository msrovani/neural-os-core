# SESSION 117 — ADR-0042 N3.5 (cortex wired no bin)

**Data:** 2026-07-16  
**Versão:** v1.7.9  
**Pista:** Monolith cleanup pós-N2.5 (`8740bfd`)

## Objetivo

Wire crate `cortex` no bin `neural-kernel` via dep direta + `pub use cortex_crate::{…}`; remover espelhos compatíveis; manter integração bin-only onde API diverge.

## Feito

| Item | Status |
|------|--------|
| `cortex-crate = { package = "cortex", path = "../cortex" }` em `neural-kernel/Cargo.toml` | ✅ |
| `pub use cortex_crate::{arena, bitnet_avx2, burn_flex, delta, nn, r3, tensor, trinity, tv_dsl}` | ✅ |
| Alias `cortex-crate` evita shadow com `mod cortex` (integração LLM/EventBus) | ✅ |
| Trinity Sprint 107 sync no crate (`PartialEq`, generator-first, `moe_router_loaded`) | ✅ |
| `cargo clean -p neural-kernel && cargo nk` | ✅ 0 erros |
| Espelhos removidos (9 arquivos) | ✅ |

## Deletados

`arena.rs`, `bitnet_avx2.rs`, `burn_flex.rs`, `delta.rs`, `nn.rs`, `r3.rs`, `tensor.rs`, `trinity.rs`, `tv_dsl.rs`

## Residual monólito (N3.5)

| Arquivo | Motivo |
|---------|--------|
| `cortex.rs` | Integração bin: `load_status`, `EVENT_BUS`, `demo_flags`, `allocator`, weather-e2e generate |
| `bpe.rs` | BPB1 + FAT/QEMU loader + lexicon clima Sprint 107 (`Vec<u32>`) |
| `global_arena.rs` | `set_pending_route` / `take_pending_route` Hermes→Cortex |
| `cortex_mmap.rs` | ADR-0041 P5/P7 — não existe no crate `cortex` |

## Próximo

- **N4.6** wire `hermes` (padrão N3.5)
- **N5.7** wire `jarbas`
