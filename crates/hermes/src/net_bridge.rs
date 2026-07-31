//! Bridge Hermes FE → neural-kernel NETSTACK (registrado no boot).
//! Browser/Search/Market/SelfUpdate NÃO usam hermes::net espelho (NETSTACK vazio).

use alloc::vec::Vec;
use spin::Mutex;

pub type HttpGetUrlFn = fn(&str) -> Result<Vec<u8>, &'static str>;
pub type ResolveAndHttpGetSafeFn = fn(&str) -> Result<Vec<u8>, &'static str>;
pub type TcpXferFn = fn([u8; 4], u16, &[u8]) -> Option<Vec<u8>>;
pub type UdpXferFn = fn([u8; 4], u16, &[u8]) -> Option<Vec<u8>>;
pub type DnsResolveFn = fn(&str) -> Option<[u8; 4]>;
/// P2P Mesh broadcast UDP (ADR-0081) — usa o NIC real do kernel.
pub type UdpBroadcastSendFn = fn(&[u8], u16) -> bool;
pub type UdpBroadcastRecvFn = fn(u16) -> Option<Vec<u8>>;

static HTTP_GET_URL: Mutex<Option<HttpGetUrlFn>> = Mutex::new(None);
static RESOLVE_AND_HTTP_GET_SAFE: Mutex<Option<ResolveAndHttpGetSafeFn>> = Mutex::new(None);
static TCP_XFER: Mutex<Option<TcpXferFn>> = Mutex::new(None);
static UDP_XFER: Mutex<Option<UdpXferFn>> = Mutex::new(None);
static DNS_RESOLVE: Mutex<Option<DnsResolveFn>> = Mutex::new(None);
static UDP_BCAST_SEND: Mutex<Option<UdpBroadcastSendFn>> = Mutex::new(None);
static UDP_BCAST_RECV: Mutex<Option<UdpBroadcastRecvFn>> = Mutex::new(None);

pub fn register_http_get_url(f: HttpGetUrlFn) {
    *HTTP_GET_URL.lock() = Some(f);
}

pub fn register_resolve_and_http_get_safe(f: ResolveAndHttpGetSafeFn) {
    *RESOLVE_AND_HTTP_GET_SAFE.lock() = Some(f);
}

pub fn register_tcp_xfer(f: TcpXferFn) {
    *TCP_XFER.lock() = Some(f);
}

pub fn register_udp_xfer(f: UdpXferFn) {
    *UDP_XFER.lock() = Some(f);
}

pub fn register_dns_resolve(f: DnsResolveFn) {
    *DNS_RESOLVE.lock() = Some(f);
}

pub fn register_udp_broadcast_send(f: UdpBroadcastSendFn) {
    *UDP_BCAST_SEND.lock() = Some(f);
}

pub fn register_udp_broadcast_recv(f: UdpBroadcastRecvFn) {
    *UDP_BCAST_RECV.lock() = Some(f);
}

pub fn udp_broadcast_send(payload: &[u8], port: u16) -> bool {
    match *UDP_BCAST_SEND.lock() {
        Some(f) => f(payload, port),
        None => false,
    }
}

pub fn udp_broadcast_recv(port: u16) -> Option<Vec<u8>> {
    match *UDP_BCAST_RECV.lock() {
        Some(f) => f(port),
        None => None,
    }
}

pub fn http_get_url(url: &str) -> Result<Vec<u8>, &'static str> {
    match *HTTP_GET_URL.lock() {
        Some(f) => f(url),
        None => Err("net_bridge: kernel HTTP not registered"),
    }
}

pub fn resolve_and_http_get_safe(url: &str) -> Result<Vec<u8>, &'static str> {
    match *RESOLVE_AND_HTTP_GET_SAFE.lock() {
        Some(f) => f(url),
        None => Err("net_bridge: kernel HTTPS not registered"),
    }
}

pub fn tcp_xfer(host: [u8; 4], port: u16, payload: &[u8]) -> Option<Vec<u8>> {
    match *TCP_XFER.lock() {
        Some(f) => f(host, port, payload),
        None => None,
    }
}

pub fn udp_xfer(host: [u8; 4], port: u16, payload: &[u8]) -> Option<Vec<u8>> {
    match *UDP_XFER.lock() {
        Some(f) => f(host, port, payload),
        None => None,
    }
}

pub fn dns_resolve(hostname: &str) -> Option<[u8; 4]> {
    match *DNS_RESOLVE.lock() {
        Some(f) => f(hostname),
        None => None,
    }
}






