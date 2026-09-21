# SESSION_375 — event-bus bughunt AIOS (bounded queues + honesty)

**Foco:** Premissas ADR-0088 · SESSION_352 unbounded EventBus · Capability fail-closed · LatentBus stats.

**Canvas:** [event-bus-bughunt-s375](file:///C:/Users/msrov/.cursor/projects/c-DEV-neural-os-core-latest/canvases/event-bus-bughunt-s375.canvas.tsx)

## Achados → fixes

| Sev | Achado | Fix |
|-----|--------|-----|
| H1 | VecDeque ilimitada × N subs | DEFAULT=64 / AUDIO_*=8 + drop_oldest |
| H2 | publish Ok(()) com 0 subs | `Result<usize>` Ok(0)=ninguém |
| H3 | LatentBus recv_count no enqueue | contar só em try_receive |
| H4 | Receiver drop = zumbi | Arc::strong_count prune + unsubscribe |
| H5 | as_legacy(Ed25519)→1 | →0 fail-closed |
| H6 | capacity(0)→1 | Err("capacity_zero") |
| M1 | AUDIO_* stream | STREAM_QUEUE_DEPTH + comment voice |
| M2 | libm unused | removido Cargo.toml |
| M3 | BoundedChannel “SPSC” | doc shared mailbox |
| L1 | DedupWindow morto | módulo apagado |
| L2 | codemap overclaim | honesty s375 |

## Call-sites

- `cortex::projection`: Ok(n) + slog ok
- `hermes::MonitorAgent`: Ok(n)
- `neural-kernel::adr0047_mvp_gates`: unsubscribe pós-smoke; recv=try_receive

## Não tocado

- ticket-lock repr(C); Legacy(0) deny; try_receive non-blocking
- ring SPSC dedicado p/ AUDIO (residual IDEA #562 — cap 8 mitiga)

## Testes

- `cargo test -p event-bus --lib`: **12/12 pass**
- `cargo check -p hermes -p cortex -p jarbas -p k_ai`: 0 erros
