//! TLS — canônico hermes (C5 emagrecer). Soft-float embedded-tls 0.19.
//! Re-exporta trust + client; mantém bridge register_https_get (lesson 241).
//!
//! Bin vira `pub use hermes_crate::tls::*` + wiring via `register_https_get`.

extern crate alloc;

pub mod trust;
pub mod client;

pub use trust::{
    ca_chain_boot_smoke, issuer_pinned, last_certverify_ok, last_trust, load_pins_from_fat,
    persist_pins_to_fat, pin_ca_der, HybridProvider, HybridVerifier, TrustClass,
};
pub use client::{
    alloc_aligned_buf, https_get_on_stack, parse_https_url, KernelRng, NetTcpIo, TlsIoError,
};

use alloc::vec::Vec;

// ─── Bridge: function pointer registrada pelo kernel no boot ───────

/// HTTPS GET retornando body (headers stripped).
pub type HttpsGetBodyFn = fn(&str) -> Result<Vec<u8>, &'static str>;

static HTTPS_GET_BODY: spin::Mutex<Option<HttpsGetBodyFn>> = spin::Mutex::new(None);
static TLS_READY: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Registra a função de HTTPS GET do kernel.
/// Chamado no boot pelo `neural-kernel::main` (Phase 7).
pub fn register_https_get(f: HttpsGetBodyFn) {
    *HTTPS_GET_BODY.lock() = Some(f);
    TLS_READY.store(true, core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_hermes!("TLS", "info", "bridge=registered https_get=OK");
}

// Compat alias — lesson 241 wiring `register_tls` existe em docs.
pub fn register_tls(f: HttpsGetBodyFn) {
    register_https_get(f);
}

/// Inicializa o subsistema TLS.
pub fn init_tls() {
    TLS_READY.store(true, core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_hermes!("TLS", "info", "init_tls() — hermes embedded-tls 0.19 soft-float");
}

/// Verifica se TLS está pronto.
pub fn tls_ready() -> bool {
    TLS_READY.load(core::sync::atomic::Ordering::Relaxed)
}

/// Dispatcher único para qualquer URL.
/// `https://` → kernel TLS via bridge (embedded-tls 0.19).
/// `http://` → net_bridge HTTP (netstack smoltcp).
pub fn fetch_url(url: &str) -> Result<Vec<u8>, &'static str> {
    let u = url.trim();
    if u.starts_with("https://") || u.starts_with("HTTPS://") {
        fetch_https(u)
    } else {
        crate::net_bridge::resolve_and_http_get_safe(u)
    }
}

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
                "fetch_https: bridge not registered, delegating to net_bridge"
            );
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
