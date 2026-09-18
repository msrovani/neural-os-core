# SESSION_357 — neural-kernel análise + bughunt profundo (bin honesty)

## Goal
Mapear papel/premissas do bin `neural-kernel` (integração + residuals) e bughunt amplo em duas ondas: spin/false-OK/urgency; depois RESPAWN/BEI/GGUF/boot_log dual.

## Papel (resumo)
Bin = Limine entry + 8 fases + wire de bridges (`register_*`) + residuals (shutdown HW, CapGate demos, ELF/Ring3, NetFS, BEI, model ladders). Lógica de produto nas crates K³CHJ via `pub use`. Emagreçer: proibido engordar com espelhos.

## HIGH corrigidos (onda 1 + 2)
| Bug | Fix |
|---|---|
| PS/2 `while` eterno em 0x64 | Budget TSC 50 ms/passo; timeout → warn |
| USB QEMU skip / StorageBus live-USB logados `ok` | → `warn` |
| slog TRACE (`aios`/`deny`/`smoke`/`loader`/`xhci`/onda/msg) | → `ok`/`warn`/`fail` |
| TICKV `"FAIL or skip"` | FAIL e SKIP separados |
| `set_urgency` antes do register + `audio_pipeline` morto | pós-register; nome `audio_input` |
| `smp/spsc.rs` cópia byte-igual | `pub use k_nano::smp::spsc::*` |
| BootLog USB-MSC / dual k_ai | skip + bridge `register_read_boot_log` |
| NETFS `VERDICT=FAIL` com `info` (=Sev::Ok) | → `fail`/`ok`/`warn` |
| `link_watcher` MMIO fantasma `0x10000000` | Down honesto; sem write #PF |
| RESPAWN `hermes_console` → DisplayAgent | → `ConsoleAgent` |
| RESPAWN incompleto (voz/infer/…) | Arms + warn em nome desconhecido |
| `gguf_streaming` Ok com Range truncado | `Err` se written < total |
| `bei_init` `.expect` + “connections established” | `try_new` DEGRADED; slog PARTIAL/noop |
| Cap-only demos slog SUCCESS | PARTIAL/`warn` |

## Verify
- `cargo check -p neural-kernel -p k_ai --release` — 0 erros

## Aberto (residual)
- Dois MemoryAgent (hermes `"memory"` + bin `"MemoryAgent"`)
- `isolation_ring` exit code 0 no path ELF; `init_connectors` 2×
- Cap demos Cap-only ainda `Ok` (slog PARTIAL)
- Órfãos `shell.rs` / `fs/`/`vfs/` no disco sem `mod`
- Codemap bin atrasado (`agents`/`net` já facade)

## Lições
1. `set_urgency` com nome ausente = no-op silencioso (SESSION_258) — sempre pós-`register`.
2. RESPAWN wrong-type (`hermes_console`≠Display) pior que missing arm.
3. Bridge fn: transmute do endereço (não `read_volatile` no código) — mesmo padrão SelfHeal s356.
4. slog sub desconhecido = TRACE; `info` = Ok — FAIL com `info` = honesty invertida.
5. Timeout/`Ok` parcial (GGUF Range, Cap-only SUCCESS) = classe timeout≠success.
