# SESSION 456 — Imagem HW `usb_hw.img` PACK_LLM=all + rebuild do zero (kernel + nsgdb) — 2026-10-06

## Pedido
Gerar a imagem para HW real com todos os artefatos → rebuild from scratch
(inclusive o nsgdb) → ritual pós-tarefa completo → regenerar a imagem.

## 1. Imagem HW (1ª passada)
- `cargo build --release -p boot` (incremental, 1m36s, 0 erros) +
  `$env:PACK_LLM="all"; python tools\build_image.py --hw --unified --build-boot --size 12288`
  → `target\usb_hw.img` 12,4 GB (ESP FAT32 @ LBA 2048 + DATA FAT32 0x0C @ LBA 262144).
- Verificação: `verify_usb_esp.py` (kernel.elf sha = árvore) + `check_usb_image_models.py`
  (71 entradas) + walk manual da FAT.

## 2. Bug achado pela verificação — LEARNER.BIN sombreado (FIX APLICADO)
- `LEARNER.BIN` no FAT tinha **544,3 MB / cluster do FALCON1B**; fonte `target1\LEARNER.v6`
  = 123.666.664 bytes (117,9 MB).
- Causa raiz (`tools/mkfat32.py:427`): fallback `find_falcon3_1b()` vinha **antes** de
  `find_file("LEARNER.v6")` na cadeia `or` — primeiro truthy vence e sombreia o modelo real.
- Fix (já commitado em `9094a85d` por outra frente na mesma árvore): arquivo real primeiro,
  Falcon3-1B só como fallback quando não há arquivo LEARNER.
- Pós-fix + regen: `LEARNER.BIN cl=14235601 size=123666664` (cluster próprio, magic `BE11BE11`).
- Pares `.V6`/`.BIN` do mesmo modelo compartilham cluster+size (alias intencional, sem duplicar bytes).
- Ausentes esperados (sem fonte em `target1/`): BITNET2B/850, nomes legados HWEXPRT
  (HWEXPRT4.BIN com o v6 presente).

## 3. Rebuild do zero
- Kernel: `cargo clean` (2778 arquivos, 43 GB) + `cargo build --release -p boot` = **1m58s,
  0 erros**; `cargo check --release` 0 erros. `libneural_sgdb-*.rlib` recompilado antes do
  `k_ai` (junction `crates/neural-sgdb → C:\DEV\neural-sgdb` confirmada via reparsepoint).
- Repo `github.com/msrovani/neural-sgdb` v1.4.3: build default + `--all-features`,
  **0 erros, 0 warnings**, `libneural_sgdb.rlib` fresh.
- Trava medida: `cargo clean` lá falha (os error 5) — `mcp_server.exe` vivo (PID 3320)
  trava o próprio exe. Workaround: apagar tudo exceto o exe + parents; `cargo build`
  (sem examples) não relinka o travado. **Não matar o server sem permissão.**
- `target/mcp-release` = leftover de outra invocação; build canônico usa `target/release`.

## 4. Evidências (imagem final, pós-ritual)
- ESP: `kernel.elf sha256=6f18e21e… match=True` + BOOTX64.EFI + limine.
- DATA: 71 entradas; LEARNER.BIN 117,9 MB; todos os degraus 1B/3B/7B/10B + slots + firmware.

## Lições (espelhadas em AGENTS.md + n-sgdb `scope=project/neural-os-core`)
- Fallback derivado de outro modelo nunca antes do arquivo real em cadeia `or`.
- Root FAT32 com spc=1 é cluster chain — ler além do 1º setor retorna lixo `0x55`.
- Verificação de imagem = ESP-sha + lista de modelos + walk de dirent em anomalia de size.
