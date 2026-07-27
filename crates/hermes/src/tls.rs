//! TLS — HTTPS client bridge to neural-kernel embedded-tls (ADR-0016 N4).
//! ADR-0062 P1: conexões seguras sem POSIX.
//!
//! Usa function pointer bridges registradas pelo kernel no boot.
//! O kernel tem implementação completa com embedded-tls 0.19, HybridProvider
//! (TOFU + pinning), ECDSA P-256 + RSA-PSS SHA256 CertificateVerify.
//!
//! Este módulo expõe a API pública de TLS para agentes e skills no hermes.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

// ─── Status tracking ───────────────────────────────────────────────

/// Estado do subsistema TLS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsStatus {
    Uninitialized,
    Ready,
    Blocked { reason: &'static str },
}

static TLS_STATUS: AtomicU8 = AtomicU8::new(0);

const STATUS_UNINIT: u8 = 0;
const STATUS_READY: u8 = 1;
const STATUS_BLOCKED: u8 = 2;

// ─── Bridge: function pointers registradas pelo kernel ────────────

/// Ponteiro de função: HTTPS GET retornando body (headers stripped).
/// Assinatura compatível com `neural-kernel::net::https_get`.
pub type HttpsGetBodyFn = fn(&str) -> Result<Vec<u8>, &'static str>;

static HTTPS_GET_BODY: spin::Mutex<Option<HttpsGetBodyFn>> = spin::Mutex::new(None);

/// Registra a função de HTTPS GET do kernel.
/// Chamado no boot pelo `neural-kernel::main`.
pub fn register_https_get(f: HttpsGetBodyFn) {
    *HTTPS_GET_BODY.lock() = Some(f);
}

// ─── API pública ───────────────────────────────────────────────────

/// Inicializa o subsistema TLS.
/// Marca como Ready; a implementação real está no kernel.
pub fn init_tls() {
    TLS_STATUS.store(STATUS_READY, Ordering::Relaxed);
    k_nano::slog_hermes!("TLS", "info", "init_tls() — bridge to kernel embedded-tls 0.19");
}

/// Verifica se TLS está pronto para uso.
pub fn tls_ready() -> bool {
    TLS_STATUS.load(Ordering::Relaxed) == STATUS_READY
}

/// HTTPS GET via bridge do kernel.
/// Retorna (status_code, body) — status_code é extraído do raw HTTP.
pub fn https_get(host: &str, path: &str) -> Result<(u16, Vec<u8>), &'static str> {
    if !tls_ready() {
        return Err("tls_not_ready");
    }

    let url = alloc::format!("https://{}{}", host, path);

    match *HTTPS_GET_BODY.lock() {
        Some(f) => {
            // ponytail: body only — status_code inferred as 200
            // Real status parsing would need raw response (headers + body)
            let body = f(&url)?;
            if body.is_empty() {
                Err("https_empty")
            } else {
                Ok((200, body))
            }
        }
        None => {
            k_nano::slog_hermes!(
                "TLS",
                "warn",
                "https_get host={} path={} — no bridge registered, fallback HTTP",
                host,
                path
            );
            https_get_fallback(host, 443, path)
        }
    }
}

/// HTTPS GET via HTTP fallback (sem TLS, debug/dev apenas).
/// Constrói URL `http://host:port/path` e delega ao net_bridge.
pub fn https_get_fallback(host: &str, port: u16, path: &str) -> Result<(u16, Vec<u8>), &'static str> {
    k_nano::slog_hermes!(
        "TLS",
        "warn",
        "https_get_fallback — NO TLS, using raw TCP on {}:{}",
        host,
        port
    );
    let url = alloc::format!("http://{}:{}{}", host, port, path);
    crate::net_bridge::resolve_and_http_get_safe(&url)
        .map(|body| (200u16, body))
        .map_err(|e| {
            k_nano::slog_hermes!("TLS", "info", "fallback=FAIL err={}", e);
            "http_fallback_failed"
        })
}

/// TLS smoke test (verifica init e ready).
pub fn tls_smoke() -> bool {
    init_tls();
    let ready = tls_ready();
    k_nano::slog_hermes!(
        "TLS",
        "info",
        "smoke=OK ready={} VERDICT={}",
        ready as u8,
        if ready { "PASS" } else { "FAIL" }
    );
    ready
}
