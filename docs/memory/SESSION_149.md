# SESSION_149 — Onda 7: LAN RX destravado (e1000 TX offsets)

## Objetivo
Destravar internet / gate `depends_on: lan` (RX>0 / L3.5) na Onda 7.

## Causa raiz
Registros TX do e1000 usavam **aliases** Intel `0x0420..0x0438` (`TDBAL_A`/`TDT_A`).
No QEMU `hw/net/e1000.c`, esses aliases **não estão em** `macreg_writeops` — write = no-op.
Sintoma: `send()=true` mas `TDH=TDT=0`, `tx_dd=false`, ARP nunca sai → RX forever 0.

Offsets canônicos (funcionam no QEMU e no HW 8254x):
`TDBAL=0x3800` `TDBAH=0x3804` `TDLEN=0x3808` `TDH=0x3810` `TDT=0x3818`

## Fixes
| Área | Mudança |
|------|---------|
| `neural-kernel` + `k_nano` `e1000.rs` | TX regs → 0x3800 family |
| `kick_rx` | Não escrever RDH; evitar RDT==RDH (false-full QEMU) |
| `prove_rx` | Wall-clock pause, wait TX DD, 3× ARP |
| `net.rs` | prove iters 6000 |
| `netstack` | DNS wait ~1.5s (L4 ainda falha — residual) |

## Evidência QEMU (WHPX)
Log: `logs/boot_o7_whpx_20260718_180023.txt` (e boots `boot_o7_dns_*`)

```
prove_rx ARP#1 sent=true tx_dd=true TDH=1 TDT=1
prove_rx: ok=true rdh=3 dd=3
L3.5 OK: RX alive dtx=2 drx=4
NET-HW VERDICT=PASS reason=rx_alive
```

## Status honesto
| Degrau | Status |
|--------|--------|
| L3 e1000 link | ✅ |
| L3.5 ARP/RX | ✅ **gate lan** |
| L4 DNS | ❌ timeout (drx sobe; UDP parse/entrega residual) |
| L5 HTTP | ❌ (segue L4) |
| WiFi | ▶️ AWAITING / `depends_on: wifi` |

## PreFlight wave 7
`lan_rx_ok=True` após logs PASS. Itens `depends_on: lan` (#418, tls-fetch) deixam de ser `force_blocked` quando RX OK → **DO**.

## Próximo
1. Debug L4 DNS/smoltcp UDP no e1000 (RX chega, socket não consome)
2. Fila `depends_on: lan` (#418, #119–123, #308, AirLLM Net)
3. WiFi permanece separado
