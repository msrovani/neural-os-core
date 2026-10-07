# SESSION 462 — Fotos M1 no HW + pacote M1-FIX/M2/M3 — 2026-10-07

## 1. Leitura das 5 fotos (build M1 rodando)
- `usb_ok=1` + `to0` (zero timeouts) provam o build M1 no HW: sem a sequência
  `port 1..4 reset TIMEOUT` — skips no lugar. Boot mais rápido, mesmo quadro
  (M1 é higiene, não cura — esperado).
- `PRE HC=00:0d.0 RS=0` (TBT): HC parado na chegada → HCRST legítimo ali;
  takeover não salva esse HC. Todos `CCS=0 PED=0 PLS=5 SP=0` (vazio real).
- `ccs=3` estável no 16p com MSC=None: 3 links vivos (provável câmera/BT) —
  retrain funciona nesse HC; o stick é o ausente específico.
- Bloco PRE do 00:14.0 NÃO veio nas fotos (só TBT). `port=3 3×` = dumps
  por-HC (cada HC tem sua porta 3), não bug — confirmado por leitura do loop.
- Risco achado: DONE persistente do M1 podia esconder o stick para sempre se
  marcado numa falha transiente (reset TIMEOUT etc.).

## 2. Pacote implementado (@fixer, check 0 erros, testes 7/7 novos + 13/13 xhci)
- M1-FIX: DONE só em classificação DEFINITIVA (non-MSC com desc lido, hub
  exaurido, SCSI com resposta sem BOT); transientes (reset/slot/addr/ep0/cfg/
  scsi-transport) viram `retry P<n> <motivo>`, nunca done. Logs `done`/`retry`
  em serial + FB.
- M2 companion: mapa via Supported Protocol Caps (primeiro range por major);
  twin ordinal tentada 1× SÓ em reset-FAIL com história na gêmea (`twin
  P<a>/<b>`); sem história/sem budget = sem try. Twin done nunca re-tentada.
- M3 CAS: `CCS==0 && PLS∈{7,10}` → 1 WPR 100ms (`CAS WR P<n>`) + re-ler CCS;
  fora disso, `proto_ss` do F2 intacto.
- Imagem `usb_hw_1b3b.img` regenerada (ESP sha=`61698f65` match; DATA 1B+3B,
  demais slots intactos).

## Reteste pedido no HW
Regravar (Rufus DD) e fotografar: (1) bloco `PRE HC=00:14.0` (RS? PED? —
  decide takeover de vez); (2) linhas `twin`/`CAS WR`/`done`/`retry`/`MSC OK`;
  (3) HUD final. Se `MSC OK`, resolvido; se twin sem história em tudo, o elo
  é hub (M5) — próxima.
