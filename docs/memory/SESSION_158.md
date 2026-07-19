# SESSION_158 — TLS PKI híbrido (pins + TOFU)

**Data:** 2026-07-18  
**Log:** `logs/boot_tls_pki_20260718_195513.txt`

---

## Goal

Opção **3** (híbrido): hosts conhecidos → pin sticky (“root-class”); demais → TOFU.  
Fingerprint = SHA-256(leaf X.509 DER). Smoke: `#1 root_learn` → `#2 root_pin`.

## Evidência serial

```
[TLS] VERDICT=WIRED trust=hybrid crate=embedded-tls-0.19
[TLS] smoke=START url=https://www.google.com/
[TLS] trust=root_learn host=www.google.com fp=a633 leaf=1114
[TLS] VERDICT=PASS bytes=81416 trust=root_learn
[TLS] smoke=PASS bytes=81416 trust=root_learn (google#1)
[TLS] trust=root_pin host=www.google.com fp=a633
[TLS] VERDICT=PASS bytes=75498 trust=root_pin
[TLS] smoke=PASS bytes=75498 trust=root_pin (google#2)
```

QEMU: WHPX `-cpu qemu64` (~16s).

## Implementação

| Peça | Onde |
|------|------|
| `HybridVerifier` / `HybridProvider` | `crates/neural-kernel/src/tls_trust.rs` |
| Wire | `tls_client.rs` (substitui `UnsecureProvider`) |
| Logs | `net.rs` `trust=` + smoke duplo google |
| Dep | `p256` 0.13 (tipo `Signature`) |

Known hosts: `google.com`, `www.google.com`, `example.com`, `www.example.com`.  
Tabela de pins: 16 entradas RAM.

## Residuais (honestos)

- Não é CA store / chain verify (`rustpki`/`webpki` não wired)
- “Roots” = leaf pins sticky após 1ª observação em host conhecido
- `verify_signature` (CertificateVerify ECDSA/RSA) = residual N+1 (`Ok(())`)
- Persistência FAT dos pins = residual
- TOFU path não exercitado neste smoke (só google known-host)

## Aceite

✅ Híbrido wired + smoke `root_learn` → `root_pin`.
