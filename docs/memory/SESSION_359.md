# SESSION_359 — bin wave2/3 honesty + emagreçer BeiState/MoE

## Goal
Fechar residuals do bughunt amplo no `neural-kernel` (ondas 2–3) e emagreçer os dois monólitos ainda no bin: BeiState e MoE director.

## HIGH / honesty
| Bug | Fix |
|---|---|
| Cap P4–P9 `Ok` só com Cap (sem backend) | `Err` Cap-only |
| AirLLM streaming inventava tokens | stub honesto / refuse |
| N5 `fb_ready` com Cap sem FB físico | exige gpu/present; Cap-only=`ABSENT` |
| SkillSync / marketplace “0 callers” | reativados; wire em `bei_tick` |
| Boot/shutdown hang em TIMER spin | `sleep_us` / TSC budget |
| Dual `MemoryAgent` no RESPAWN | `memory` vs `memory_budget` |
| slog `n4`/`n5`/`info`/`dbg` mudos | `ok`/`warn`/`fail` (ADR-0092) |
| `/chat` mentia LLM pronto | honesty no gate cortex |
| Órfãos `fs/` `vfs/` `shell.rs` no bin | delete; verdade em hermes/k_nano |
| heap `expect` → panic | halt honesto + slog fail |

## Emagreçer (ADR-0057+)
| Residual | Destino | Bin |
|---|---|---|
| BeiState / `init_bei` / `bei_tick` (~550 LOC) | `hermes::bei` | `bei_init.rs` = `pub use` |
| MoE director `generate_via_model` (+ `_with_route`) | `cortex::cortex` | `cortex.rs` = `pub use` |
| Volume HwControl | `register_audio_volume_setter` | wire pós-`audio::init_audio()` |

`hermes` Cargo: `cortex` com `features = ["p2p"]` (mesh self-test no `bei_tick`).

## Verify
- `cargo check -p hermes` — 0 erros
- `cargo check -p cortex --features p2p` — 0 erros
- `cargo check --release -p neural-kernel` — 0 erros

## Aberto
- InferQueue decode 1-tok sob lock (latência UI) — residual s358
- TQ2_0 layout local ≠ ggml oficial — residual s358
- QEMU/HW re-boot pós-emagreçer AWAITING

## Lição
Feature `p2p` do cortex **não** unifica sozinha no hermes: mover código que chama `mesh_matmul_self_test` exige `features = ["p2p"]` na dep do hermes, senão o item some no `cfg`.
