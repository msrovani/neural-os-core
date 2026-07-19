# SESSION_155 — TLS #123 opção A: compile probe PASS

**Data:** 2026-07-18  
**Tipo:** probe isolado (sem wire no bin)  
**Crate:** `tools/check-tls` (workspace `exclude`)

---

## Comando

```powershell
cd tools\check-tls
cargo check --release --target x86_64-unknown-none --target-dir ..\..\target\check-tls
```

Soft-float herdado de `.cargo/config.toml` (`-sse…-sse4.2`).

## Resultado

| Item | Verdict |
|------|---------|
| `embedded-tls` **0.19.0** + deps (aes-gcm, p256, sha2, …) | **PASS** — 0 erros |
| `check-tls` lib (`TlsConfig` / `Aes128GcmSha256` / `TlsConnection`) | **PASS** |
| Wire `neural-kernel::https_get` | **não feito** (fora do probe) |
| PreFlight `tls-fetch` | ainda PARTIAL / stub |

```
Finished `release` profile [optimized] target(s) in 0.23s  # re-check pós-fix API
```

Primeira tentativa falhou só na API do probe (`TlsConfig` 0.19 não é genérico sobre cipher); a crate **já tinha compilado**.

## Implicação

O bloqueio documentado `reason=softfloat_or_crate` estava **desatualizado para compile**: soft-float + `x86_64-unknown-none` **aceitam** `embedded-tls` 0.19 com `default-features = false`.

Bloqueios restantes para N4 wire:

1. Adaptador TCP → `embedded-io` Read/Write sobre `NETSTACK` / `tcp_exchange`
2. RNG criptográfico bare-metal (`rand_core` + fonte OS/RDRAND)
3. Trust: TOFU / root embutido (webpki std ainda frágil)
4. `https_get` real porta 443 + consumidores Hermes
5. Smoke QEMU + `[TLS] VERDICT=PASS`

## Artefatos

- `tools/check-tls/Cargo.toml` + `src/lib.rs`
- `Cargo.toml` workspace: `exclude += "tools/check-tls"`
- **Não** adicionado a `neural-kernel` deps

## Próximo

Wire N4 mínimo: dep no bin + `https_get` sobre TCP existente + log VERDICT; smoke GET `https://` controlado.
