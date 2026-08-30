# SESSION_296 — HW splash freeze + 1º frame compositor

**Sprint:** v1.9.99-s296 TEST  
**Data:** 2026-08-29/30  
**Objetivo:** Pendrive 21:13 congelava em `"Neural OS Core - Inicializando..."`; ler evidência em `E:\`.

## Evidência E: (volume NEURAL-OS)

| Arquivo | Resultado |
|---|---|
| `BOOT.LOG` | Placeholder mkfat32 (3 linhas) — kernel **não gravou** |
| `NSGDB.BIN` | 8 MB, **100% zeros** — TickvLite nunca inicializado |
| `CONFIG.TXT` | `LOG_TO_FAT32=1` OK na imagem |

Conclusão: boot chegou ao **Runtime** (splash = `claim_graphics` tick 1), mas **sem MSC** no stick → zero persistência no FAT de dados.

## Causa do freeze visual

**SESSION_168:** splash no tick 1; `desktop.render()+swap()` só no tick 2. Scheduler/Hermes pode atrasar minutos → tela parada no splash.

## Fix

`crates/jarbas/src/display/agent.rs`: após `claim_graphics()`, `invalidate_all()` + `render()` imediato no tick 1.

Imagem regerada: `target/usb_hw.img` 6271 MB @ 22:22 (`PACK_LLM=all`).

## Residual

- BOOT.LOG/NSGDB no metal exigem MSC enumerar o próprio stick.
- Aceite: orb+HUD visíveis logo após splash.
