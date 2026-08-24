# SESSION_289 — ADR-0092 observabilidade de boot (O0–O5)

**Data:** 2026-08-24  
**Sprint:** v1.9.99-s289 TEST  
**ADR:** 0092 (`docs/architecture/0092-boot-observability.md`) · IDEA #539  
**Objetivo:** dmesg Neural — três canais, severidade, banners de fase, placar parseável, HUD de produto.

---

## Aprendido

O boot já era funcional; o relato era instrumentação de sessão. `slog` tinha anel/crate mas o 4º campo era lixo (`info`, `e1000`, `ckpt`). `boot_ckpt(Knn)` no FB e INIT1 no ecrã competiam com o compositor. `BootReport` existia sem placar. Linux/Theseus: um printk + nível + capitão — copiámos a disciplina, não o código.

## Feito (O0–O5)

| Onda | Entrega |
|------|---------|
| O0 | `Sev` ok/warn/fail/trace; desconhecido = TRACE; consola default esconde TRACE; `boot-trace` para ficheiro |
| O1 | `=== PHASE n= name= status= ===` uma vez por 0–8; DriverInit extra = TRACE; `PostRuntime` |
| O2 | BPB uma vez; INIT1 só timeout; SMP `Brought up N APs`; e1000 MMIO TRACE; llama scan ×1; PnP sem `HERMES_RESPONSE` |
| O3 | `BOOT SCORE` em `finalize_and_publish`; `tools/parse_boot_score.py` |
| O4 | `boot_ckpt` não pinta FB; HUD `JARBAS`+RAM+net; sem dump BOOT.LOG no ecrã |
| O5 | `qemu=` no placar; LLM/áudio ABSENT no sandbox = `degraded expected` |

## Evidência

- `cargo test -p k-nano --lib slog::tests` 3/3 PASS  
- `cargo check --release -p neural-kernel` 0 erros (`target/check-s92`)  
- Parser exit 0 em `docs/evidence/boot-score-adr0092-sample.txt`  
- **Residual:** capturar serial QEMU real e correr o parser no log (lifecycle ADR = `fazendo` até essa evidência)

## Limites

TRACE some da consola — debug fino exige `--features boot-trace`. Sem COM1 o placar vai ao journal, não ao GOP (HUD compacto só).

## Próximo

Boot QEMU → `python tools/parse_boot_score.py logs/<serial>.txt` → se exit 0, lifecycle 0092 → `completa`.
