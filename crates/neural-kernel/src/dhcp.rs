//! edge-dhcp integration — cliente DHCP no_std + no-alloc para B-01.
//! #356: fallback DHCP alternativo ao smoltcp.
//!
//! Referência: https://github.com/sysgrok/edge-net (edge-dhcp crate)
//! Uso: adicionar ao Cargo.toml:
//!   edge-dhcp = { version = "0.1", optional = true }
//!   [features]
//!   edge-dhcp = ["edge-dhcp"]
//!
//! Pipeline:
//!   1. Adicionar dependência edge-dhcp (no_std + no-alloc DHCP client/server)
//!   2. Integrar com smoltcp Device trait como fallback
//!   3. Usar para DHCP early boot (antes do heap estar pronto)

#![allow(dead_code)]

use crate::serial_println;

pub fn status() -> &'static str {
    #[cfg(feature = "edge-dhcp")]
    { "edge-dhcp: ATIVO (via crate edge-dhcp)" }
    #[cfg(not(feature = "edge-dhcp"))]
    { "edge-dhcp: DISPONIVEL (adicione feature 'edge-dhcp')" }
}

pub fn init() {
    serial_println!("[DHCP] {} — usando smoltcp como fallback", status());
}
