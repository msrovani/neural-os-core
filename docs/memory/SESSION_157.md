# SESSION_157 — TLS N4 smoke PASS (QEMU)

**Data:** 2026-07-18  
**Log:** `logs/boot_tls_20260718_195001.txt`

---

## Goal

`https_get` e2e → `[TLS] VERDICT=PASS` + `smoke=PASS`.

## Evidência serial

```
[TLS] VERDICT=WIRED trust=unsecure crate=embedded-tls-0.19
[TLS] smoke=START url=https://www.google.com/
[TLS] GET 142.251.150.119:443/ Host=www.google.com trust=unsecure
[TLS] step=tcp_connect
[TLS] step=tcp_ok
[TLS] step=handshake
[TLS] step=handshake_ok
[TLS] step=http_sent
[TLS] VERDICT=PASS bytes=80952 trust=unsecure
[TLS] smoke=PASS bytes=80952 (google)
```

Pré-condição: L5 HTTP OK neste boot.

## Correções aplicadas

| Problema | Fix |
|----------|-----|
| `cargo build` LLVM fail `polyval`/`sha2` soft-float | `--cfg polyval_force_soft` + `aes_force_soft` + `sha2` feature `force-soft` |
| WHPX `-cpu host` → #GP OVMF (APX/MPX) | Smoke com `-accel whpx -cpu qemu64` (não `host`) |
| Hang handshake sob TCG | Soft AES lento; WHPX+qemu64 completa em ~15s |
| TCP session clock fixo | Tempo virtual + `wall_pause` (já SESSION_156) |
| Smoke example.com hang | `prime_neighbor` + google.com (L4/L5 aquecido) + step logs |

## Avisos QEMU (ignoráveis)

```
this feature conflicts with APX/MPX ...
Ignoring request for interrupt vector 0
```

Não impedem boot com `-cpu qemu64`.

## Residual

- Trust ainda `unsecure` (sem PKI)
- PreFlight `tls-fetch` → promover com marker deste log
- TCG: handshake pode demorar minutos (soft crypto)

## Aceite

✅ Goal N4 smoke atingido neste log.
