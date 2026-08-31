# SESSION_297 — virtio_blk driver: causa raiz "missing headers" + NSGDB persistente em QEMU

**Sprint:** v1.9.99-s297 TEST
**Data:** 2026-08-30/31
**Objetivo:** Persistir o NSGDB entre boots no QEMU dev/test (via virtio_blk, escolha do maintainer) em vez de `backend=RAM (VOLATIL)`.

## Sintoma

`logs/qd.err` = `virtio-blk missing headers`; `[VBLK] timeout` 5× (avail idx 1→5, used idx=0); `[TICKV] backend=RAM (VOLATIL)`.

## Investigação

1. **Descritores corretos do lado do driver** (dump: desc[0] hdr flags=0x0, desc[1] dados flags=0x2, desc[2] status flags=0x2).
2. **Hipótese de corrupção de page tables REFUTADA** via `page_leaf_phys()` (walk P4→PT): `queue_va->phys=0x111ff000 (esperado 0x111ff000)`, `scratch_va->phys=0x11202000 (esperado 0x11202000)`. HHDM aponta pro frame certo.
3. **Causa raiz (via @librarian + source QEMU):** `VRING_DESC_F_NEXT` (0x1) ausente nos flags. QEMU só segue `next` se esse bit estiver setado (`virtqueue_split_read_next_desc`). Sem ele lê só o header → `out_num=1, in_num=0` → `virtio_error` marca o device BROKEN. `virtio_net.rs` nunca acertou porque **não encadeia** (1 pacote = 1 desc, next=0).

## Fix

`crates/k_nano/src/virtio_blk.rs`:
```rust
const DESC_F_NEXT: u16 = 1;   // VRING_DESC_F_NEXT
set_desc(0, scratch_pa, 16, DESC_F_NEXT, 1);
set_desc(1, scratch_pa+4096, len, if is_write { DESC_F_NEXT } else { DESC_F_NEXT | DESC_F_WRITE }, 2);
set_desc(2, scratch_pa+16, 1, DESC_F_WRITE, 0); // fim
```

`crates/k_nano/src/storage/flash.rs`: `FlashDev::VirtioBlk` (enum + name + `with_flash_dev` + `ORDER[5]`).
`crates/k_nano/src/memory.rs`: `page_leaf_phys()` (diagnóstico P4→PT).

## Verificação (QEMU TCG 4c 6G)

```
[VBLK] ok - io=0xc000 cap=3072MB qsize=256 blk=512 ro=false
[VBLK] ok - self-test: MBR 0x55AA OK
[TICKV] ok - backend=file lba=5514479 dev=virtio cap=8192KB   ← persistente
```

`err=0` (antes `err=1`). NSGDB (`SELF.STATE`/episódica/HANR/audit) agora persiste em disco no QEMU.

## Commits

- `6027ee4` — BAR0 raw re-read (read_bar_value mascara bit 0 de I/O BAR) + diagnóstico virtqueue
- `9d0cf04` — VRING_DESC_F_NEXT + FileFlash::VirtioBlk + page_leaf_phys

## Lição (→ AGENTS.md)

Descritor encadeado exige `VRING_DESC_F_NEXT` no **flags** — o campo `next` é ignorado pelo QEMU sem esse bit. Driver que não encadeia (virtio_net) nunca expõe o bug.
