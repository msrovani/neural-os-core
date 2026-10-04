# SESSION_445 — Forum CURAIX: recover_count no wipe + reload pos-Tickv

**Data:** 2026-10-04
**Versao:** v1.9.99-s445 TEST
**Branch:** `main`
**Crates:** `k_nano` (`boot_ramlog`), `logwriter-efi`, `hermes` (`skill_loader`, `skill_lab`), bin `neural-kernel` (`main.rs`)
**Foro:** `LOG AGENTES .txt` (OPCODE/1). Identidade desta sessao: CURAIX. Ultima linha escrita antes deste registro: CURAIX-0049. Proxima livre: CURAIX-0050.

## 1. O que esta sessao fez

O foro pediu sobrevivencia do BSP (`#PF` storm / OOM nao pode parkar em `hlt`) e um harness F1.5 de skill que persiste entre dois boots. CURAIX implementou a camada de recover e mediu dois suites de host. O harness F1.5 entrou no tree por outros ids do foro; CURAIX conferiu o fio e nao fechou o run.

## 2. Recover

- Header 24 bytes. `recover_count` u32 no offset 20. `logwriter-efi` usa o mesmo `HDR_SIZE`. `mark_done` escreve magic e crc; nao toca o contador.
- `append`, quando o magic ja e `NEURDONE`, fazia `write_bytes` dos 256 KiB e apagava o contador. Agora le o u32, zera, grava de volta.
- `reboot_ordered` sela e so reseta se a decisao permitir e o contador for < 3. `is_bsp` esta hardcoded `true`. Quem exclui o AP sao os callers em `interrupts_ext` e `allocator`.
- `try_bsp_recover_reset` foi apagado: chamava `warm_reset` sem incrementar o contador e nao tinha caller.
- Init com magic desconhecido ainda zera o header inteiro. Magic 0 cai nesse ramo; se o header ja era zero, o efeito no contador e nulo. Nao reclassifiquei magic 0 como "desconhecido" de proposito (pin do foro).

## 3. Reload de skills

- Se `list_vfs("/skills")` falha, a funcao nao retorna 0: segue o passe `skill/wasm/` no Tickv.
- O fim da funcao sempre faz `RELOAD_DONE=true` e `RELOAD_SKIPPED=false`.
- `with_tickv` devolve `None` sem mount (`tickv.rs`). `unwrap_or_default` transforma isso em `durable=0`. Desconhecido reportado como zero.
- Ordem no bin: primeira chamada ~linha 2495, `tickv_smoke` ~2744, segunda chamada ~2760 (fora do `if`, roda mesmo se o smoke falhar). A funcao nao sai cedo em `RELOAD_DONE`, entao a segunda chamada revarre. Esse e o caminho do harness.
- `retry_reload_if_skipped` sai imediatamente se `DONE` ou se `SKIPPED` e falso. O wrapper seta `DONE` antes de chamar. Em producao o retry e no-op. O teste que passa seta `DONE=false` e `SKIPPED=true` a mao — estado que o bin nao produz.
- Patch v2 (`RELOAD_RETRIED` gravado no wrapper antes do reload, e so rearmar `SKIPPED` se ainda nao houve retry) foi aceito na logica e **nao aplicado**.

## 4. F1.5

- `hermes::skill_lab`, magic `LSK1`, phys `0x0211_0000`, `BLOB_LEN = 512`. `poll_and_run` em `agents.rs`.
- `tools/gen_lsk1.py` grava `target/lab_skill_boot1.bin` e `boot2.bin` com **256** bytes. Prefixos medidos: boot1 `LSK1Goracle_rt_expr_v1.(a*a + 3*b) - 5.6.7`; boot2 `LSK1Eoracle_rt_expr_v1..9.2`.
- Aritmetica a mao: a=6 b=7 → 52; a=9 b=2 → 82. Nao rodei wasmi.
- O kernel le 512. Os quatro campos sao NUL-terminated dentro dos 256, entao o parse nao depende do pad. O pad de 512 **nao foi escrito**.
- Se os 4 primeiros bytes nao sao `LSK1`, `poll_and_run` retorna `None` sem log e sem `CONSUMED`. Warn unico so para nao-zero diferente de `LSK1` ficou combinado e **nao escrito**. Pagina zero continua muda.
- `tools/run-f15.ps1` termina em `& $Qemu`. Nao ha `Select-String`, nem PASS, nem parser. Cada invocacao ve um log. Exigir `act=gen` e `act=reuse` no mesmo ficheiro falsifica os dois boots. O hash no log e FNV-1a64 dos bytes lidos de `skill/wasm/NAME`, nao do texto-fonte. Comparar hashes exige os dois logs. A linha de reload do boot 2 e a ultima (`Select-Object -Last 1`), porque a primeira chamada ainda imprime `durable=0`.
- QEMU com serial de 0 bytes foi afirmado por outro id e **nao reproduzido** aqui.

## 5. Medido

| Suite | Resultado |
|---|---|
| `cargo test -p k-nano --lib boot_ramlog -- --test-threads=1` | 3 passed, 0 failed, exit 0 |
| `cargo test -p hermes --lib skill_loader -- --test-threads=1` | 4 passed, 0 failed, exit 0 |
| `cargo check -p k-nano --target x86_64-unknown-none --offline` | exit 0, perfil dev, antes do hook F1.5 |

Os 3 testes de ramlog nao entram no ramo do wipe.

## 6. UNKNOWN

- `cargo check --release` depois de `skill_lab`.
- Testes de `skill_lab`.
- Serial QEMU, `TICKV backend=file`, `[RECOVER]` no BOOT.LOG.
- Conteudo de `uefi.img` (frescura se mede na imagem, nao no ELF). `cargo nk` nao regenera a imagem.
- 1 h sem `#PF`. Watchpoint no writer (`tools/watch_corruption.ps1`).
- Durabilidade em power-loss. QEMU com `cache=writethrough` nao e corte de energia.

## 7. Governanca

Decisao com `accepted` no foro e sem linha `from=HUMAN` permanece **proposta**. Sem tag. Sem declarar v2.0.0. Sem commit do tree inteiro (s441–s444 e `target/` ja estavam sujos).

## 8. Proximo (nao iniciado neste registro)

1. Aplicar `RELOAD_RETRIED` e manter o slog `durable=`, acrescentando `durable_unknown=`.
2. Pad 512 no gerador. Warn unico de magic. Parser de dois logs.
3. `cargo check --release` + testes `skill_lab`.
4. Rebuild de `uefi.img` e um boot com serial, so depois do parser existir.
