# TLS Integration Blueprint — Neural-OS-Core

**Status:** Active  
**Date:** 2026-07-23  
**Related:** ADR-0016 (Network), ADR-0062 (ClaudioOS Analysis), #123 (TLS Real)

---

## 1. Current State Analysis

### 1.1 Existing Implementation
- **embedded-tls 0.19** already in Cargo.toml (default-features = false)
- **tls_client.rs** — `https_get_on_stack()` with NetTcpIo bridge
- **tls_trust.rs** — HybridProvider (pins + TOFU) with HybridVerifier
- **KernelRng** — HardwareRandom-based RNG for embedded-tls

### 1.2 Blocking Issues (from STATE.md:261)
- `[TLS] VERDICT=BLOCKED softfloat_or_crate`
- Current implementation is stub (`tls_not_ready`, deny https)
- Soft-float crypto forced via `.cargo/config` (polyval_force_soft, aes_force_soft, sha2/force-soft)

### 1.3 ClaudioOS Reference (ADR-0062)
- **File:** `crates/net/src/tls.rs`
- **Library:** embedded-tls 0.17 with Aes128GcmSha256
- **Alignment:** 16-byte via `alloc_aligned_buf()` for AES-NI
- **Bridge:** SmoltcpSocket → embedded_io::Read+Write
- **Helper:** `https_request()` — DNS → TCP → TLS → HTTP → close
- **Security Gap:** `NoVerify` in dev (must NOT copy)

---

## 2. Integration Architecture

### 2.1 Component Map

```
┌─────────────────────────────────────────────────────────────┐
│                    APPLICATION LAYER                        │
│  SelfUpdate HTTPS  │  BrowserAgent HTTPS  │  Market HTTPS  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      TLS CLIENT LAYER                       │
│  https_get_on_stack()  │  parse_https_url()  │  NetTcpIo   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      TLS PROTOCOL LAYER                     │
│  embedded-tls 0.19  │  TlsConnection  │  TlsConfig         │
│  Aes128GcmSha256    │  TlsContext     │  HybridProvider    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      TRUST LAYER                            │
│  HybridVerifier  │  PinTable (16 entries)  │  TrustClass   │
│  RootPin/RootLearn/Tofu/TofuLearn/Deny                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      TRANSPORT LAYER                        │
│  NetTcpIo (embedded_io bridge)  │  NetStack (smoltcp)      │
│  tcp_session_connect/send/recv  │  SocketHandle            │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Data Flow

```
Application Request
       │
       ▼
parse_https_url() → (host, port, path)
       │
       ▼
https_get_on_stack(stack, ip, port, host, path, now)
       │
       ▼
tcp_session_connect() → SocketHandle
       │
       ▼
NetTcpIo::new(stack, handle, now)  ← embedded_io bridge
       │
       ▼
TlsConnection::new(io, read_buf, write_buf)
       │
       ▼
TlsConfig::new().with_server_name(host).enable_rsa_signatures()
       │
       ▼
HybridProvider::new() → HybridVerifier + KernelRng
       │
       ▼
tls.open(TlsContext::new(&config, provider))  ← HANDSHAKE
       │
       ▼
tls.write_all(HTTP_REQUEST) → tls.flush()
       │
       ▼
tls.read() loop → body bytes
       │
       ▼
tcp_session_close(handle)
       │
       ▼
Return body to application
```

---

## 3. Required Changes

### 3.1 Core Implementation (Phase 2)

| Task | File | Description |
|------|------|-------------|
| 2.1 TLS Bridge | `tls_client.rs` | Verify NetTcpIo implements embedded_io correctly |
| 2.2 16-Byte Alignment | `tls_client.rs` | Add `alloc_aligned_buf()` for read/write buffers |
| 2.3 Certificate Verification | `tls_trust.rs` | Ensure HybridVerifier is production-ready (no NoVerify) |
| 2.4 NetStack Integration | `tls_client.rs` | Test https_get_on_stack end-to-end |

### 3.2 Application Integration (Phase 2)

| Task | File | Description |
|------|------|-------------|
| 2.5 SelfUpdate HTTPS | `net.rs` / `main.rs` | Replace HTTP with HTTPS for SelfUpdate |
| 2.6 BrowserAgent HTTPS | `hermes/src/browser_agent.rs` | Enable HTTPS requests |
| 2.7 Market HTTPS | `hermes/src/marketplace.rs` | Enable HTTPS product catalog |

### 3.3 Testing & Verification (Phase 3)

| Task | Description |
|------|-------------|
| 3.1 Unit Tests | Test HybridVerifier, HybridProvider, KernelRng |
| 3.2 Integration Tests | Test https_get_on_stack with real endpoints |
| 3.3 Security Tests | Verify certificate pinning, TOFU, denial cases |
| 3.4 Smoke Tests | SelfUpdate HTTPS, BrowserAgent HTTPS, Market HTTPS |
| 3.5 Performance Tests | Handshake latency, throughput |
| 3.6 Compatibility Tests | soft-float, CI pipeline |

---

## 4. Security Requirements

### 4.1 Must Implement
- [ ] **Certificate Verification** — No `NoVerify` in production
- [ ] **Root Pinning** — Pre-loaded pins for known hosts (google.com, etc.)
- [ ] **TOFU** — Trust On First Use for unknown hosts
- [ ] **CSPRNG** — KernelRng uses HardwareRandom (not xorshift64*)
- [ ] **Constant-Time** — Verify embedded-tls uses constant-time ops

### 4.2 Must NOT Copy from ClaudioOS
- [ ] `NoVerify` mode (dev only, security gap)
- [ ] `DevRng` xorshift64* (not CSPRNG)
- [ ] `static mut VECTOR_STORE` without Mutex

---

## 5. Configuration

### 5.1 Cargo Features
```toml
# Already present
embedded-tls = { version = "0.19", default-features = false }
rand_core = { version = "0.6", default-features = false }
embedded-io = { version = "0.7", default-features = false }
sha2 = { version = "0.10", default-features = false, features = ["force-soft"] }
p256 = { version = "0.13", default-features = false, features = ["ecdsa"] }
```

### 5.2 .cargo/config.toml (Soft-Float)
```toml
[target.x86_64-unknown-none.rustflags]
# Force soft crypto for LLVM compatibility
-C target-feature=-aes,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2
```

---

## 6. Milestones

| Milestone | Target | Criteria |
|-----------|--------|----------|
| M1: Core TLS Working | Week 2 | `cargo check --release` passes, https_get_on_stack compiles |
| M2: SelfUpdate HTTPS | Week 3 | SelfUpdate fetches via HTTPS |
| M3: BrowserAgent HTTPS | Week 4 | BrowserAgent makes HTTPS requests |
| M4: Market HTTPS | Week 4 | Market fetches catalog via HTTPS |
| M5: All Tests Pass | Week 5 | Unit + Integration + Security tests pass |
| M6: Documentation | Week 6 | Complete docs, migration guide |

---

## 7. Risk Mitigation

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| embedded-tls 0.19 API changes | Medium | High | Pin version, test early |
| soft-float performance | High | Medium | Benchmark, optimize buffers |
| Certificate verification bugs | Medium | Critical | Extensive security tests |
| NetStack integration issues | Low | High | Test with smoltcp TCP first |

---

## 8. Success Criteria

- [ ] `cargo check --release` — 0 errors
- [ ] `https_get_on_stack("https://www.google.com/")` returns body
- [ ] SelfUpdate via HTTPS works
- [ ] BrowserAgent HTTPS requests work
- [ ] Market HTTPS catalog fetch works
- [ ] Certificate pinning verified (RootPin for known hosts)
- [ ] TOFU works for unknown hosts
- [ ] No `NoVerify` in production code
- [ ] All tests pass in CI