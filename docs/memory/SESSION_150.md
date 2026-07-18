# SESSION_150 — Internet L4/L5 destravada (DNS raw + HTTP)

## Objetivo
Completar Onda 7 além do L3.5: DNS + HTTP smoke no path e1000/slirp.

## Evidência
Log: `logs/boot_o7_dnsok_20260718_181440.txt` (WHPX)

```
L3.5 OK: RX alive
NET-HW VERDICT=PASS reason=rx_alive
DNS OK raw 142.250.78.206
L4 OK: 142.250.78.206
L5 OK (792 bytes): HTTP/1.1 301 Moved Permanently
```

## Causas / fixes

| Problema | Fix |
|----------|-----|
| smoltcp perde 1º UDP no ARP (dequeue + NeighborPending) | DNS **raw** Ethernet+IP+UDP no NIC |
| Parser DNS seguia pointer `0xC0xx` e corrompia offset | `skip_dns_name` — skip no wire sem follow |
| Checksum RX dropava pacotes slirp | `Checksum::Tx` only no DeviceCapabilities |
| TCP/HTTP sem neighbor | `prime_neighbor_for_http()` (gw 10.0.2.2) |

## Status
| Degrau | Status |
|--------|--------|
| L3.5 ARP/RX | ✅ |
| L4 DNS | ✅ raw e1000 |
| L5 HTTP | ✅ smoltcp TCP |
| WiFi | ▶️ `depends_on: wifi` |

## Arquivos
- `crates/neural-kernel/src/netstack.rs` — dns raw, skip_dns_name, checksum, prime
- `crates/neural-kernel/src/network_agent.rs` — loops HTTP longos, prime antes de L5
