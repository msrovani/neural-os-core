# SESSION_376 — neural-kernel bughunt AIOS (bin wire honesty)

**Sprint:** v1.9.99-s376 TEST  
**Data:** 2026-09-21  
**Foco:** Premissas emagreçer + honesty High→Low no bin `neural-kernel`  
**Canvas:** `neural-kernel-bughunt-s376.canvas.tsx`

## Premissas relidas

- Bin = integração + residuals (ADR-0057+ / `.cursor/rules/neural-emagrecer-bin.mdc`)
- Soft power / BootLog tick / TLS → crates (`k_ai` / `hermes`)
- slog ADR-0092: `ok|warn|fail|trace` — DENY PoC esperado = `ok` (SESSION_360)
- AIOS-first ADR-0088: Observe→Plan→Act→Verify→Remember; sem stub que mente

## Findings aplicados

| ID | Sev | Fix |
|----|-----|-----|
| H1 | HIGH | `shutdown.rs` thin: soft API = `pub use k_ai::shutdown`; bin só `begin_orderly_*` + EventBus drain |
| H2 | HIGH | BootLog Agent → `k_ai` (+ BOOT_PHASE + SelfHeal); bin = FAT reader + bridges `register_read_boot_log` / `register_push_periodic` |
| H3 | HIGH | Delete `link_watcher.rs` (morto) + `email_agent.rs` / `rss_agent.rs` órfãos |
| H4/M3 | HIGH/MED | CapGate DENY `info`→`ok`; WARN/FAILED/SKIP slog em `main` + demos |
| M1 | MED | Cargo trim: uart_16550, linked_list_allocator, spinning_top, embedded-tls+p256+rsa+… (canônico hermes/k_nano) |
| M4 | MED | `codemap.md` honesty wire/residuals |
| L1 | LOW | netfs dual FE/BE documentado (não merge) |
| L2 | LOW | smoltcp 0.14 / spin 0.12 / x86_64 0.15 = IDEA residual (não bump cego soft-float) |

## Verificação

- `cargo check -p neural-kernel --release` → 0 erros (34 warnings Known)
- `cargo test -p k_ai --lib -- --test-threads=1` → 47 pass / 0 fail / 1 ignored

## Não feito (honesto)

- Bump smoltcp/spin/x86_64 workspace (API + soft-float)
- Split `main.rs` 4.6k LOC
- AUDIO_FRAME → SPSC (IDEA #562; event-bus cap 8 já mitiga)

## Lição

Dual-truth bin↔k_ai em power/BootLog é pior que stub: hermes já lia k_ai enquanto o drain HW lia statics divergentes. Emagreçer = `pub use` soft + HW residual, não cópia.
