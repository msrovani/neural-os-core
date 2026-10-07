# SESSION 461 — Fórmula de ataque do oráculo + M1 higiene — 2026-10-07

## 1. Pesquisa @librarian reconciliada (verificada contra nosso código)
- `0x2a0` = porta vazia normal (OSDev t=30372, NetBSD) — não PHY morto.
- Nosso init JÁ é textbook (`xhci/mod.rs:624-764`: halt→HCH, HCRST→
  `USBCMD.HCRST==0`+`CNR==0`, CONFIG escrito, DCBAA/CRCR/ERST, Run→HCH=0,
  PP RMW, LEGSUP + SMI mask). T1/T6/T7/T13/T14 NÃO se aplicam.
- Aplicáveis: takeover sem HCRST (T10), companion via Protocol Caps,
  hub route+TT, preferir 00:14.0, Chitti como modelo Rust.

## 2. Fórmula de ataque do oráculo (ora-1, reconciliada)
- Tese: "parem de resetar, comecem a preservar" (stage-0 prova UEFI treinado).
- Ordem: M0 foto PRE (zero risco, maior info) → M1 higiene (vai com tudo) →
  M2 companion → M3 Missing-CAS → M4 takeover A/B (ponto de não-retorno,
  blindado) → M5 hub-desc → M6 settle → M7 rivais. Sem foto: assume RS=1,
  pula p/ M1+M2.
- Red flags confirmadas nos nossos fixes (budget em vazio, `return ped ||
  reset_change` mentiroso).

## 3. M1 implementado (@fixer, check 0 erros, provado no ELF)
- `hub_msc.rs`: gate hub-first — sem CCS e sem CSC sticky → `skip P<n> sem
  história` (serial + FB), sem reset. Path CCS>0 intacto.
- `bringup.rs`: `port_csc_sticky` novo (leitura pura); `clear_msc_port_skips`
  virou CSC-gated (DONE persiste entre HCs/retries; só re-plug reabre);
  poll: `return ped && !reset_active` (WRC/PRC sem PED = FAIL honesto).
- Desvio validado: CSC = bit 17 (não bit 1 = PED) — nosso `mod.rs:779/782`
  confirma; bit 1 quebraria o path CCS>0.
- `mod.rs`: export + failover nunca limpa (static nasce 0).
- Imagem `usb_hw_1b3b.img` regenerada (ESP sha=`ce471ea5` match; DATA 1B+3B).

## Reteste pedido no HW
Regravar (Rufus DD) e fotografar: (1) linhas `PRE HC=...` (M0 — decide
takeover vs companion); (2) sequência USB — esperado: `skip P<n> sem história`
em vez de `port 1..4 reset TIMEOUT`, boot mais rápido, mesmo quadro clínico
(M1 é higiene, não cura). Se `MSC OK`, resolvido já no M1.
