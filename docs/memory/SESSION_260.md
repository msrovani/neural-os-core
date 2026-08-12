# SESSION_260 — Crash HW real no K33: campo `address` fantasma do StackSizeResponse + pendrive corrompido (2026-08-12)

**Escopo:** Diagnóstico do reboot em HW real (i5-7300HQ / GTX 1050 / 16GB, boot UEFI do
pendrive) parado no checkpoint K33 ("apps+audio+wasm") + pendrive FAT32 corrompido.
**Status:** ✅ Fechada — 1 commit (`57ad20a`) — 0 erros — validado QEMU/OVMF.

---

## 1. Sintoma

- Boot HW real com `target/usb_hw.img` (6271MB, GPT ESP@2048 + DATA FAT32@262144, MBR híbrido
  0xEE+dados): o firmware **agora lista e boota o stick** (fix MBR do 2dd6ffc funcionou),
  mas o kernel parava no checkpoint `K33: apps+audio+wasm` (main.rs:1878) e o notebook
  **rebootava**.
- Depois do reboot, o pendrive parecia corrompido — sem BOOT.LOG legível.

## 2. Investigação (ora-1 + verificação independente)

- **K33 é impresso antes do bloco** main.rs:1878–2001 que roda SEM checkpoints
  intermediários: wasmi self-tests, `exec_arena::self_test` (executa código nativo W^X),
  TransformerTrainer backprop, SGDB/TickvLite smokes, AEAD, mesh chunk… Qualquer crash no
  bloco mostra "K33" como último checkpoint.
- **Panic handler faz `hlt` loop** (main.rs:809–838) — não rebota. Reboot observado ⇒
  **triple fault** (reset da CPU), não panic do kernel.
- Notebook i5-7300HQ = Kaby Lake = **tem AVX2** → candidato #UD/AVX2 eliminado (gate
  `allow_avx2` correto).
- **Prova no log de boot QEMU:** `reserva stack Limine 0x0 len=2MB` — o `resp.address` lia 0.

## 3. Causa raiz (2 bugs encadeados)

### 3.1 Crash K33 + reboot: campo `address` fantasma do StackSizeResponse

O fix da SESSÃO_254 (`8901d97`) assumiu que `limine_stack_size_response` tem campo
`address` e fez `reserve_range(resp.address, 2MB)`. **O protocolo Limine define apenas:**

```c
struct limine_stack_size_response {
    uint64_t revision;
};
```

(fonte oficial: limine-protocol/PROTOCOL.md, seção Stack Size — verificado em 12/08).

O campo `address` foi **inventado no struct Rust** (k_nano/src/limine.rs). O bootloader
escreve só `revision`; o campo fantasma lia .bss zerado = **0** → `reserve_range(0, 2MB)`
= **no-op silencioso**. A stack de 2MB do Limine **nunca foi reservada**.

No HW real (16GB, watermark das alocações mais alto que QEMU 6G), as alocações do bloco
K33 (arena tensores 512MB + wasmi + exec_arena) cruzam a região da stack → frames da
PRÓPRIA stack são entregues ao heap → `alloc_zeroed` zera a stack → return address
corrompido → `#PF` → `#DF` → **triple fault → reboot silencioso**.

QEMU/OVMF passava porque o watermark nunca cruzava a região da stack (memória total
menor). Bug latente do tipo "funciona no emulador, explode em HW real".

### 3.2 Pendrive corrompido: reboot no meio do `overwrite_boot_log`

`k_nano/src/boot_logger.rs:182–209`: cada persist grava os clusters de dados e depois
**reescreve o dir cluster inteiro** setor-a-setor via USB (um WRITE10 por setor). O boot
reescreve BOOT.LOG 10+ vezes antes do K33 (`persist_now` a cada 8 linhas + `init_after_usb`
flush). O crash determinístico no K33 resetava a máquina no meio de uma dessas escritas →
**dir cluster rasgado** (BOOT.LOG/CONFIG.TXT/WIFI.CFG meio antigos/novos) → Windows não
monta / não lê. Sem journal, sem recovery — cada boot agravava.

## 4. Fix (commit `57ad20a`)

Derivar a stack do **RSP atual** em vez do campo fantasma:

```rust
let rsp: u64;
unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags)); }
let rsp_phys = rsp.wrapping_sub(pm_offset);
// Stack pode não ser 2MB-alinhada → margem: reserva 4MB a partir de
// (rsp alinhado p/ baixo em 2MB) − 2MB.
let stack_base = (rsp_phys & !(2 * 1024 * 1024 - 1)) - 2 * 1024 * 1024;
frame_allocator.reserve_range(stack_base, 4 * 1024 * 1024);
```

- `StackSizeResponse` (k_nano/src/limine.rs) corrigido para o ABI real: `{ revision }`.
- O kernel EXECUTA na stack do Limine → RSP atual está dentro dela; RSP virtual = phys +
  pm_offset (HHDM). Reserva a janela de 4MB que a contém (margem p/ stack não-alinhada).

## 5. Validação

| Run | Resultado |
|---|---|
| Log QEMU pré-fix | `reserva stack Limine 0x0 len=2MB` (no-op) |
| Log QEMU pós-fix | `reserva stack via RSP 0x98000000 len=4MB (rsp_phys=0x982d0b80)` |
| `cargo check --release` | 0 erros |
| `usb_hw.img` regenerado | 6271MB (ESP FAT + dados FAT32, BITNET2B v6 773MB + LLAMA8B 2GB + firmware) |

O `kernel_region fallback` (0x9839f000 len=0x20a0f000) continua funcionando em paralelo —
imagem e stack ambas reservadas.

## 6. Lições

1. **Nunca confiar em campo de struct de bootloader sem conferir o protocolo oficial:**
   o campo `address` "obviamente útil" que lia 0 era bug latente — só explodiu em HW com
   mais RAM (16GB vs QEMU 6G). Verificar PROTOCOL.md antes de usar.
2. **Classe "funciona no emulador":** QEMU/OVMF mascara bugs de watermark/reserva; HW real
   com mais RAM expõe. Sempre validar o caminho de memória com o cenário de pior caso.
3. **Reboot = triple fault** (panic faz hlt). Crash no meio de escrita de dir FAT corrompe
   o volume; escrita não-atômica + crash determinístico = corrupção agravada a cada boot.
4. **Derivação por estado real > struct de resposta:** RSP atual é a prova viva de onde a
   stack está — não depende de ABI de resposta.

## 7. Pendências

- MED (ora-1, não fixado): USB-MSC CSW tag nunca validado + DMA em páginas WB (sem
  `map_page_uc`) — usb_msc.rs:148–160,232–238. Classe do e1000 (SESSION E1000): risco em
  chipset sem snooping. Segue como débito.
- MED (ora-1, não fixado): `overwrite_boot_log` reescreve dir cluster não-atomicamente —
  tornar a escrita do dir atômica (journal/single WRITE10) evitaria corrupção em crash
  futuro.
- BUG VGA notebook antigo (registrado no AGENTS.md): transição p/ 3ª resolução continua
  aberto (não relacionado a esta sessão).
