//! AIOS API — re-export do crate hermes + bin-only CapGate wrappers.
//!
//! ADR-0075: o conteúdo canônico vive em `hermes_crate::aios_api`.
//! O bin mantém override de `aios_net_http_get` (stack real via `crate::net`)
//! e as funções `aios_send_tcp`/`aios_write_ring` que dependem de `CapabilityGate`.

use alloc::string::String;

pub use hermes_crate::aios_api::{
    AIOS_NET_DOCS, AIOS_FS_DOCS, build_system_prompt, rag_inject, aios_fs_read, aios_fs_write,
};

use crate::capability_gate;
use crate::syscall::Cap;

/// HTTP GET real via bin stack (crate retorna stub).
pub fn aios_net_http_get(url: &str) -> Result<String, &'static str> {
    let body = crate::net::resolve_and_http_get_safe(url)?;
    core::str::from_utf8(&body)
        .map(String::from)
        .map_err(|_| "aios_net.http_get: not utf8")
}

/// Host sensível: TCP send exige Cap::RING_OP (Hermes WASM / skills).
pub fn aios_send_tcp(held: Cap, host: &str, port: u16) -> Result<u64, &'static str> {
    capability_gate::host_send_tcp(held, host, port)
}

/// Host sensível: write ring IPC exige Cap::RING_OP (ADR-0076 §4.3).
pub fn aios_write_ring(held: Cap) -> Result<u64, &'static str> {
    capability_gate::host_write_ring(held)
}
