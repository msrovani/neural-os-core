# SESSION_156 — TLS N4 wire (item a item)

**Data:** 2026-07-18  
**Tipo:** wire ADR-0016 N4  
**Check:** `cargo check --release -p neural-kernel --target x86_64-unknown-none` → 0 erros

---

## Itens

| # | Item | Estado |
|---|------|--------|
| N4.1 | Dep `embedded-tls` 0.19 + `rand_core` + `embedded-io` no bin | ✅ |
| N4.2 | `KernelRng` → `HardwareRandom` (RDRAND/ChaCha fallback) | ✅ |
| N4.3 | `NetTcpIo` + `tcp_session_*` no NetStack | ✅ |
| N4.4 | `https_get` / `resolve_and_http_get` rota HTTPS :443 | ✅ |
| N4.5 | Docs + boot `VERDICT=WIRED trust=unsecure` | ✅ |

## Arquivos

- `crates/neural-kernel/Cargo.toml` — deps TLS
- `crates/neural-kernel/src/tls_client.rs` — novo
- `crates/neural-kernel/src/netstack.rs` — `tcp_session_connect/send/recv/close`
- `crates/neural-kernel/src/net.rs` — `https_get` real; boot WIRED
- `crates/neural-kernel/src/main.rs` — `mod tls_client`

## Trust (honesto)

`UnsecureProvider` — **não** valida certificado. Log: `trust=unsecure`.  
PKI/TOFU = residual pós-smoke.

## Serial esperado

```
[TLS] VERDICT=WIRED trust=unsecure crate=embedded-tls-0.19
# após GET https://...
[TLS] VERDICT=PASS bytes=N trust=unsecure
# ou
[TLS] VERDICT=FAIL reason=tls_handshake|tls_tcp_connect|...
```

## Smoke QEMU (próximo)

Com LAN up: `https_get("https://example.com/")` ou Hermes `/fetch` HTTPS.  
Não auto-smoke no bootstrap (evitar deadlock NETSTACK).

## Aceite compile

```
Finished `release` profile [optimized] — 0 erros (target/check-n4)
```
