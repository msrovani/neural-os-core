# SESSION 457 — Boot HW real: HUD aponta `ccs:0` → fix hub-first — 2026-10-06

## Evidência do HW (fotos da tela + pendrive D:/E:)
- Desktop/orb vivo (Runtime OK). `D:\NEURAL\BOOT.LOG` = só stage-0 EFI;
  `D:\BOOT.LOG` + `E:\BOOT.LOG` 256KB zeros; `E:\NSGDB.BIN` 8MB zeros.
- HUD: `xhci c2 st10 00:14.0` · `usb 16p ccs:0 no msc` · `storage no dev` ·
  `bootlog ram only` + `fail sem-backend msc=0 ata=1` · `kv ram volatile` ·
  `model n/a`. HW: 22 PCI (nic ausente, wifi 14c3:7925, gpu 10de:28a0, storage
  8086:a77f — todos sem driver; usb xhci 16p OK).
- `T=00000000` nas fotos (thumb; orb animado = vivo). `decide a1 x5 e17 L1`.

## Causa raiz (mapeada via @explorer, confirmada no código)
- `ccs=[]` → `hub_msc.rs:86-96` retornava `None` ANTES do Pass 2 (hub) — stick
  atrás de hub interno ou sem retrain nunca era tentado. Ordem causal:
  `ccs:0` → `USB_MSC=None` → `storage no dev` + `persist any_tried=false` →
  `fail sem-backend`. Persist/storage = vítimas.
- `ata=1` era misnomer (flag "USB não-skipado", não `ATA_DRIVER`).

## Fix (@fixer, `cargo check --release` 0 erros)
- `hub_msc.rs`: tentativa hub-first (helpers do próprio arquivo, speed hint 0
  honesto) com `msc_budget_ok()` em 3 pontos; deadline só zera em sucesso ou
  budget esgotado; falha não marca porta failed (retry deferred vivo).
  Path CCS>0 intocado.
- `boot_logger.rs:165`: label `ata=` → `usb_ok=` (valor inalterado).
- Diff: +59/-4 em 2 arquivos. Kernel com fix provado por strings no ELF
  (`hub-first`, `usb_ok=` presentes). Imagem `usb_hw.img` regenerada
  (ESP sha=`b870cdfd` match, DATA 71 entradas, LEARNER 117,9 MB).

## Reteste pedido no HW
Regravar o pendrive (Rufus DD) e observar no HUD: linha `usb` deve mostrar
`tentativa hub-first` / `MSC OK ...`; `bootlog` deve sair de `fail sem-backend`;
`E:\BOOT.LOG` deve ganhar conteúdo. Se ainda `ccs:0` puro, o elo é PP pós-HCRST
ou retrain — ver `PORTSC pre-PP` (serial) ou foto da linha `usb` completa.
