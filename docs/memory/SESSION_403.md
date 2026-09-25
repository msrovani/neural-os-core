# SESSION_403 — Auditoria externa: triagem cruzada + R2 BOOT.LOG pré-alocado

**Motivo:** relatório externo listou 6 anomalias + 5 recomendações (R1–R5). Cruzamento com logs mesh6 (6 nós) + n-sgdb (S238/S252/S258/S260/S264/S269/S275): só R2 exigia fix novo; resto já-coberto, by-design ou carece de design.

## Triagem (veredito por item)
- **I4/slow-ticks/diag-loop:** confirmado (41–153 viol/nó; self-learning ~1s; loop ~115×) — honesty funcionando, drivers conhecidos (S252/S258 já implementados). R1 (time-slice 5ms) = design, fora de escopo.
- **BOOT.LOG FAIL (15×, "ausente no root"):** confirmado — arquivo ausente na imagem (17 entradas, parse FAT host). Backoff 50 já existia (S269). **R2 implementado (único fix).**
- **xHCI FAIL (10–22× ports 5/6):** ports são HID virtual (tablet/kbd), não MSC; timeouts com budget TSC + retry em camada já existem. R4 sem valor em QEMU.
- **Ring3 #PF/#UD, badsig pré-TOFU:** by-design (demos P6 contidos; fail-closed até TOFU). R5 (persistir chaves) = feature nova, sem decisão prévia.
- **Modelo:** small experts já wirados (HWEXPRT4 LOADED); Falcon3 via FAT inviável em 2GB. R3 parcial-feito.

## R2 — implementação
- `tools/mkfat32.py`: entrada `("BOOT.LOG", b"\x00"*(256*1024))` (writer já aceita bytes; dirent existe → kernel escreve data-only, sem rasgar dir — S260).
- `target/disk_qemu.raw` patchado in-place: dirent + cadeia 512 clusters contíguos + zeros, nas 2 FATs (parse host: start=483587, 512 links, EOC ✓).
- Prova viva: instância WHPX 4c + disco → `flush BOOT.LOG ok=true` repetido (linhas 327/576/1237) até PostRuntime, zero FALHOU.

## Verificação
Parse FAT host OK · boot com disco OK (`flush ok=true`) · `cargo check --release` 0 erros (sem mudança guest — só tools).
