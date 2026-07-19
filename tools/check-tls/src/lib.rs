//! Probe compile `embedded-tls` no_std + soft-float (`x86_64-unknown-none`).
//! Não é wire N4 — só prova se a crate entra no target do kernel.
#![no_std]

pub use embedded_tls::blocking::TlsConnection;
pub use embedded_tls::{Aes128GcmSha256, TlsConfig};

/// Smoke de tipo: config + cipher suite (sem I/O real).
pub fn probe_types() -> usize {
    let _cfg = TlsConfig::new();
    core::mem::size_of::<Aes128GcmSha256>() + core::mem::size_of::<TlsConfig<'static>>()
}
