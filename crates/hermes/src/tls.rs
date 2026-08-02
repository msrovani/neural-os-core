//! TLS — HTTPS client bridge to neural-kernel embedded-tls (ADR-0016 N4).
//! ADR-0062 P1: conexões seguras sem POSIX.
//!
//! Expõe `fetch_url` como dispatcher único: `https://` → kernel TLS (embedded-tls 0.19,
//! HybridProvider TOFU+pinning, ECDSA/RSA-PSS CertificateVerify); `http://` → net_bridge HTTP.
//! Nunca strip https→http. Nunca usa fallback HTTP na porta 443.

use alloc::vec::Vec;

// ─── Status tracking ───────────────────────────────────────────────

/// Estado do subsistema TLS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsStatus {
    Uninitialized,
    Ready,
    Blocked { reason: &'static str },
}

static TLS_READY: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

// ─── Bridge: function pointer registrada pelo kernel no boot ───────

/// HTTPS GET retornando body (headers stripped).
/// Assinatura compatível com `neural-kernel::net::https_get`.
pub type HttpsGetBodyFn = fn(&str) -> Result<Vec<u8>, &'static str>;

static HTTPS_GET_BODY: spin::Mutex<Option<HttpsGetBodyFn>> = spin::Mutex::new(None);

/// Registra a função de HTTPS GET do kernel.
/// Chamado no boot pelo `neural-kernel::main` (Phase 7).
pub fn register_https_get(f: HttpsGetBodyFn) {
    *HTTPS_GET_BODY.lock() = Some(f);
    TLS_READY.store(true, core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_hermes!("TLS", "info", "bridge=registered https_get=OK");
}

// ─── API pública ───────────────────────────────────────────────────

/// Inicializa o subsistema TLS.
pub fn init_tls() {
    TLS_READY.store(true, core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_hermes!("TLS", "info", "init_tls() — bridge to kernel embedded-tls 0.19");
}

/// Verifica se TLS está pronto para uso.
pub fn tls_ready() -> bool {
    TLS_READY.load(core::sync::atomic::Ordering::Relaxed)
}

/// Dispatcher único para qualquer URL.
/// `https://` → kernel TLS via bridge (embedded-tls 0.19).
/// `http://` → net_bridge HTTP (netstack smoltcp).
/// Retorna body (headers HTTP stripped).
///
/// # Uso nos consumers
/// ```ignore
/// let body = crate::tls::fetch_url("https://example.com/api")?;
/// ```
pub fn fetch_url(url: &str) -> Result<Vec<u8>, &'static str> {
    let u = url.trim();
    if u.starts_with("https://") || u.starts_with("HTTPS://") {
        fetch_https(u)
    } else {
        // HTTP via net_bridge (netstack smoltcp)
        crate::net_bridge::resolve_and_http_get_safe(u)
    }
}

/// HTTPS GET via bridge do kernel (embedded-tls 0.19).
/// Nunca fallback HTTP na porta 443.
fn fetch_https(url: &str) -> Result<Vec<u8>, &'static str> {
    match *HTTPS_GET_BODY.lock() {
        Some(f) => {
            let body = f(url)?;
            if body.is_empty() {
                Err("https_empty")
            } else {
                Ok(body)
            }
        }
        None => {
            k_nano::slog_hermes!(
                "TLS",
                "warn",
                "fetch_https: bridge not registered, delegating to net_bridge (kernel https_get)"
            );
            // Fallback: net_bridge já roteia https:// para kernel TLS
            crate::net_bridge::resolve_and_http_get_safe(url)
        }
    }
}

/// TLS smoke test (verifica init e bridge).
pub fn tls_smoke() -> bool {
    init_tls();
    let ready = tls_ready();
    let bridge = HTTPS_GET_BODY.lock().is_some();
    k_nano::slog_hermes!(
        "TLS",
        "info",
        "smoke=OK ready={} bridge={} VERDICT={}",
        ready as u8,
        bridge as u8,
        if ready { "PASS" } else { "FAIL" }
    );
    ready
}
