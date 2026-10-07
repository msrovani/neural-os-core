# SESSION 459 — Reteste HW: fix hub-first rodou, reset frio TIMEOUT → fix reset via protocolo — 2026-10-06

## Evidência do reteste (fotos do FB, imagem s457 com hub-first)
- `tentativa hub-first` RODOU (fix s457 no HW). Resultado: `port 1..4 reset
  TIMEOUT PORTSC=0x2a0/0x2b0 warm=0` + FAIL em todas; `BOOT: MSC maxports=16 ccs=3`.
- `PORTSC pre-PP: P1-P4 0x2a0` = **PP=1 já ligado** (power OK; fix PP irrelevante aqui).
- Settle: `[CCS=0 PED=0 PP=1 PLS=5 SP=0]` — link preso em RxDetect, speed 0.
- HC correto bound (16 portas = 00:14.0); hipótese "controladora errada" MORTA.
  `ccs=3` com MSC=None: portas com CCS sem enumeração completa.
- Boot seguiu vivo (USB live, VFS ok) sem MSC — persist continua RAM-only.

## Causa raiz (código, confirmada por leitura)
- `reset_port` (k_nano/src/xhci/bringup.rs:944-952): com `speed_hint=0` e CCS=0,
  o speed deriva do PORTSC (lê 0 sem device) → `warm=false` sempre → PR frio
  100ms → TIMEOUT em SS presa em RxDetect. Path WPR existia mas inalcançável.
- `classify_root_port` não tentava warm após FAIL frio; retries repetiam o
  mesmo frio para sempre. Deferred retry usa HC bound (que é o correto: 16p).

## Fix (direto, @fixer sobrecarregado — sessão descartada)
- `reset_port`: se `speed_hint==0` e speed do PORTSC==0, SS-vs-USB2 decide-se
  pelo PROTOCOLO da porta (`protocol_is_ss` novo: Major Revision da Supported
  Protocol Capability que cobre a porta; 3=warm, resto=frio como antes).
  USB2-protocolo: comportamento idêntico ao anterior. Todos os callers herdam.
- `cargo check --release` 0 erros; fix provado no ELF (`protocol_is_ss` presente).
- Imagem `usb_hw_1b3b.img` regenerada (ESP sha=`433cc607` match; DATA: 3B+1B,
  sem 7B/10B, demais slots intactos).

## Reteste pedido no HW
Regravar (Rufus DD, fechar Rufus velho que trava `usb_hw.img`) e observar:
`warm=1` nos resets, `MSC OK ...`, `bootlog` saindo de `fail sem-backend`,
`E:\BOOT.LOG` com conteúdo. Se WPR também der TIMEOUT, o elo é retrain/link
físico da porta (testar outra porta USB-A do notebook).
