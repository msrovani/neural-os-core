# SESSION_241 — TLS Bridge Fix: hermes→kernel wiring (2026-08-02)

**Objetivo:** Conectar o módulo TLS morto (`hermes::tls`) ao kernel (`embedded-tls 0.19`) e rotear todos os consumers hermes para HTTPS via bridge.

## Problema identificado

O kernel já tinha TLS 1.3 completo:
- `tls_client.rs`: `https_get_on_stack()` com `embedded-tls 0.19`, `Aes128GcmSha256`, `NetTcpIo` bridge, `alloc_aligned_buf` 16B
- `tls_trust.rs`: `HybridVerifier` (TOFU+pinning), ECDSA P-256 + RSA-PSS SHA256 CertificateVerify, sem `NoVerify`
- `net.rs`: `resolve_and_http_get()` já roteava `https://` → `https_get()`

**Mas o módulo `hermes::tls` era dead code:**
1. `register_https_get()` nunca era chamado no boot
2. Nenhum consumer hermes importava `hermes::tls`
3. Todos os consumers usavam `net_bridge::resolve_and_http_get_safe()` ou `net_bridge::http_get_url()` (HTTP-only)
4. `hermes::tls::https_get_fallback()` construía `http://host:443/path` — HTTP na porta TLS (bug silencioso)

## Implementação

### 1. `hermes/src/tls.rs` — Reescrito
- `fetch_url(url)` — dispatcher único: `https://` → kernel TLS via bridge, `http://` → net_bridge HTTP
- `register_https_get(f)` — bridge function pointer (assinatura `fn(&str) -> Result<Vec<u8>, &'static str>`)
- Fallback HTTP na porta 443 **removido**
- `tls_smoke()` — verifica init + bridge registration

### 2. `neural-kernel/src/main.rs` — Bridge wire
- `hermes_crate::tls::register_https_get(crate::net::https_get)` adicionado no Phase 7 (após net_bridge registrations)

### 3. Consumers roteados (11 arquivos)
- `browser_agent.rs`: `fetch_page()` → `crate::tls::fetch_url()`
- `marketplace.rs`: 3 chamadas → `crate::tls::fetch_url`
- `self_update.rs`: `fetch_update()` + `poll_channel()` → `crate::tls::fetch_url`
- `agents.rs`: `/fetch` + `/scrape` + model download → `crate::tls::fetch_url`
- `rss_agent.rs`: RSS fetch → `crate::tls::fetch_url`
- `search_agent.rs`: DuckDuckGo → `crate::tls::fetch_url` + `https://`
- `git_thin.rs`: Git refs fetch → `crate::tls::fetch_url`
- `async_io.rs`: `IoKind::HttpGet` → `crate::tls::fetch_url`

### 4. `hermes/src/lib.rs` — `pub mod tls;` adicionado

## Fluxo resultante

```
Consumer → crate::tls::fetch_url(url)
  ├── https:// → register_https_get → neural-kernel::net::https_get
  │                                    → tls_client::https_get_on_stack
  │                                    → embedded-tls 0.19 (HybridProvider)
  │                                    → ECDSA P-256 + RSA-PSS SHA256
  └── http://  → net_bridge::resolve_and_http_get_safe
                   → neural-kernel::net::resolve_and_http_get
                   → smoltcp HTTP
```

## Lição aprendida

**Módulo declarado + implementado ≠ funcional.** O padrão bridge (function pointer registrado no boot) exige:
1. Tipo da function pointer declarado na crate FE
2. Função `register_*()` na crate FE
3. Chamada de `register_*()` no boot (Phase 7) com cast explícito
4. Consumers chamando a API da crate FE (não o bridge diretamente)

O kernel já tinha o TLS funcionando perfeitamente — o gap era exclusivamente o wiring hermes↔kernel.

## Validação

- `cargo check --release` — 0 erros, warnings expected (dead-code)
- Nenhum warning TLS
