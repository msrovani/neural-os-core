# SESSION_404 — Imagem HW 8GB + R3/R6 do audit R2-run

**Motivo:** montar `target/usb_hw.img` p/ HW real com todos os artefatos (kernel atual c/ fixes s402+panic-FB) + fechar triagem do audit do run R2 (R3 sev, R6 SECURITY.BIN).

## Imagem HW (PACK_LLM=all, --size 8192)
- `target/usb_hw.img` 8319MB (GPT: ESP 126MB + dados FAT32 8191MB), build 21:31–21:36.
- Conteúdo verificado por parse host: BGE/E5/ROUTER/HWEXPRT4/HWEXPRT.v6/PIPER/STT/BPE/FALCON3.V6+BIN (1GB)/FALCON1B/PRO+FALCON7B (2GB)/FALCON10.V6 (2.8GB)/FW (GPCCS/SW/ACR...)/CONFIG/UPDATE/BOOT.LOG/NSGDB/TLSPINS + LEGOs.
- Kernel ESP = build atual (marcadores `[NESTED]`/`drain_pre_fat_buf`/panic-FB presentes no `kernel.elf` do target).
- Próximo passo operador: Rufus modo DD → pendrive ≥8GB → boot HW (i7: tela do panic agora mostra arquivo:linha).

## R3 — sev mapping (slog.rs)
- 40 tokens desconhecidos no run R2; mapeados 11 subsistemas → Ok (pci/hid/http/FS/TALC/ARENA/hub/stt/AGENTS/ANALYST/boot). Verbos de evento ficam TRACE (contrato ADR-0092 preservado). `cargo check` 0 erros.

## R6 — SECURITY.BIN
- Loader (`agents.rs:load_knowledge`) usa bytes só p/ `is_empty`/`len` (treino real = traces R3) → stub seguro (precedente MICROPY.WASM).
- `mkfat32.py`: entrada stub 66B p/ builds futuros; `usb_hw.img` patchado in-place (cluster 15768527 = FSI hint, EOC, read-back OK).

## Verificação
Parse FAT host (dirent+cadeia+EOC+read-back) · `cargo check --release` 0 erros.
