//! edge-dhcp integration — cliente DHCP no_std + no-alloc (#356).
//! Fallback DHCP alternativo ao smoltcp.
//! Usado pelo shell (/dhcp) e AutoLearnAgent (download de conhecimento).

use crate::serial_println;
use crate::EVENT_BUS;
use crate::Event;
use crate::CapabilityToken;

pub const TOPIC_DHCP_REQUEST: &str = "DHCP_REQUEST";
pub const TOPIC_DHCP_RESPONSE: &str = "DHCP_RESPONSE";

pub fn status() -> &'static str {
    #[cfg(feature = "edge-dhcp")]
    { "edge-dhcp: ATIVO (via crate edge-dhcp)" }
    #[cfg(not(feature = "edge-dhcp"))]
    { "edge-dhcp: DISPONIVEL (adicione feature 'edge-dhcp')" }
}

pub fn init() {
    serial_println!("[DHCP] {} — usando smoltcp como fallback", status());
}

/// Dispara requisicao DHCP via EventBus
pub fn request() {
    let _ = EVENT_BUS.publish(Event {
        id: 0, topic: alloc::string::String::from(TOPIC_DHCP_REQUEST),
        payload: alloc::vec![], token: CapabilityToken::Legacy(1),
    });
    serial_println!("[DHCP] Requisicao enviada via EventBus");
}
