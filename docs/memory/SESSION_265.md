# SESSION_265 — Fix hang HW real no K13 (smokes prematuros pós fat-boot-log)

**Data:** 2026-08-15  
**Foco:** Boot travava no marcador K13 em HW real após wire `fat-boot-log`.

## Sintoma

Pendrive live USB: último checkpoint no FB = `K13: SafeHarbor+MemoryCore`.
Não chegava em K14 (SIMD).

## Causa

Entre K13 e K14 rodava um **gauntlet de ~40 labor smokes** sem micro-checkpoint:

- `async_io::boot_smoke` spawnava HttpGet/TcpXfer (rede live quando bridge existe)
- `git_thin::boot_smoke` fazia `fetch_refs("https://github.com/...")`
- theme apply, fw_cfg port I/O, dezenas de slog

Em HW sem COM o FB só mostra `boot_ckpt` — slog não pinta (k_nano `fb_print`
stub). Resultado: tela congelada em K13 enquanto o gauntlet corre/trava.

Agravante SESSION_262/PR#7: `fat-boot-log` passou a funcionar de verdade →
`append_raw` tentava `persist_now` a cada 16 linhas **antes** de haver MSC/ATA.

## Fix

1. K13 → BootSmokeOk → platform_probe → SIMD → SYSCALL → K14 (caminho mínimo)
2. Micro-ckpts K130/K131/K132 para bisect
3. Gauntlet → `labor_smokes::run_deferred` pós-DriverInit (K71/K72)
4. `async_io` / `git_thin` smokes = só parse/API local (sem rede live)
5. `append_raw`/`log_quiet`: só `persist_now` se `storage_available()`
6. Early USB: K181–K184 em volta de xHCI/MSC

## Verificação

- `cargo check -p neural-kernel --features fat-boot-log` → 0 erros
- `cargo check -p hermes` → 0 erros

## Validação HW

Rebuild `usb_hw.img` → boot → esperar sequência K13 → K130 → K131 → K132 → K14.
Se travar, o último K18x identifica xHCI.
