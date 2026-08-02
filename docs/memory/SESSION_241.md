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

---

# SESSION_241 (cont.) — Mesh: AEAD Tier F + anti-replay dados + calibração ed25519 (2026-08-02)

**Objetivo (follow-ups ADR-0081 Fase B, decisão maintainer):** implementar AEAD para o Tier F (externo/não-isolado) e estender o anti-replay aos dados (Tier L), fechando o "clock=0 nos senders" e a falta de confidencialidade no unicast MR/EDR.

## 1. Anti-replay de dados (Tier L)

- **Raiz do falso drop cross-type:** heartbeat usava `TIMER_TICKS` (~10000) vs dados com `clock=0` → um dado com clock=0 depois de um heartbeat com clock alto era descartado como replay.
- **Fix:** `next_data_clock()` em `k_nano::net::mesh` — estrito-monotônico via `GLOBAL_LOGICAL_CLOCK.tick()` (única fonte). Todos os 12 sites de `AiosTaskPacket::new` auditados e stamped: MW/MR, ED/EDR, FD/FM, CRDT, SKILL, PROMOTE, MEM/CHK, ROLE.
- **RX:** gate de replay estendido para todo pacote autenticado de peer conhecido (`clock <= last` → DROP + `SEC_DROPPED_REPLAY++`). TOFU intacto.

## 2. AEAD Tier F (MR\0/EDR\0 encriptados)

- **Dep:** `chacha20poly1305 0.11` (`default-features = false, features = ["alloc"]`) — primeira dep cripto simétrica. X25519 via feature `x25519` do `ed25519-compact` (sem `x25519-dalek`, sem handshake novo no wire — identidade já vinculada no TOFU PK\0).
- **Wire:** `header NoProto 36B ‖ ciphertext ‖ tag16` (tag pós-fixada pelo `Aead::encrypt`).
- **Nonce:** 12B = `source_id` u32 BE ‖ `clock` u64 BE — derivado do header, NÃO vai no wire. Anti-replay garante não-repetição (NIST SP 800-38D: nonce contador sem limite 2³²).
- **KDF:** `sha256(DH(X25519_local_sk, peer_pk))` via `from_ed25519`; `LOCAL_XSK` cacheado (Mutex), slots `AEAD_KEYS[16]` indexados por peer pk.
- **RX order:** len-check → TOFU → anti-replay CHECK → decrypt → clock UPDATE. **Update só após auth** — previne forged-high-clock DoS.
- **Escopo:** unicast MR\0/EDR\0 encriptados; broadcasts (MW/ED/FD/FM/CRDT/SKILL/PROMOTE/offer/sync) permanecem assinados (sem chave única de recipiente — documentado). Fail-closed: sem chave/peer desconhecido = Full Ed25519, zero regressão.
- **Build:** `.cargo/config.toml` ganhou `--cfg chacha20_backend="soft"` + `--cfg poly1305_backend="soft"` — LLVM crash `STATUS_ILLEGAL_INSTRUCTION` com backend SIMD sob soft-float (mesmo padrão `polyval_force_soft`/`aes_force_soft`). Cargo.lock +153 linhas.

## 3. Calibração ed25519-compact 2.3.1 (host, default-features=false espelhando kernel)

- **Source confirmado: SEM SIMD** — crate portable/scalar (features `blind-keys/opt_size/pem/random/self-verify/std/traits/x25519`; zero simd/avx/sse/target_feature). A pergunta "tem SIMD?" do ADR respondida: **não**.
- **Bench:** verify 68.9µs/69.8µs/114.0µs; sign 65.5µs/68.3µs/162.3µs @ 300B/1200B/17.5KB (~14.3k ops/s). Faixa eBACS 26-46µs do ADR **corrigida** (era otimista demais p/ nosso crate).
- Bench em `C:\Users\msrov\AppData\Local\Temp\opencode\bench-ed25519` (nota: `Noise::default()` é feature-gated por `random`; usar `Noise::new([0u8; Noise::BYTES])`).

## Validação

- `cargo check --release` — 0 erros, warnings expected
- `cargo build --release` (boot image, pipeline canônico) — OK (48.78s, limine-esp.img)
- **Nota build:** `cargo nk` direto (O3 + `-Z threads=16`) crasha LLVM no codegen dos kernels AVX512 **pré-existentes** do k_ai (`arch/x86_64.rs` bitwise_add_avx512 etc.) — reproduzido em árvore sem nossas mudanças no arquivo; pipeline canônico (boot, opt-level 2 no artifact) não afetado. Known issue toolchain, não blocker do release.

## Commit

`[mesmo commit/tag da SESSION_241]` — ADR-0081 Fase B atualizada (calibração + anti-replay + AEAD Tier F), INDEX.md (ADR-0081 adicionada ao inventário), STATE.md, CHANGELOG.
