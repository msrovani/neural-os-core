use alloc::vec;
use alloc::vec::Vec;

/// Injeta pacote RX vindo do MSI-X/WiFi diretamente na interface smoltcp.
pub fn inject_rx_packet(_pkt: &[u8]) {
}
use core::sync::atomic::{AtomicU64, Ordering};
use crate::slip;

fn ip_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in data.chunks(2) {
        let word = u16::from_be_bytes([chunk[0], *chunk.get(1).unwrap_or(&0)]);
        sum = sum.wrapping_add(word as u32);
    }
    while sum >> 16 != 0 { sum = (sum & 0xFFFF) + (sum >> 16); }
    !(sum as u16)
}

/// DNS resolve MANUAL — bypassa smoltcp, envia raw UDP/IP via serial tunnel.
/// Pipelines de ataque: constroi frame IPv4+UDP+DNS e envia via slip::send().
pub fn dns_resolve_manual(hostname: &str, dns_server: [u8; 4]) -> Option<[u8; 4]> {
    let hostname = hostname.trim_end_matches('.');
    if hostname.is_empty() { return None; }

    // 1. DNS name encoding with validation
    let mut qname = Vec::new();
    for part in hostname.split('.') {
        if part.is_empty() || part.len() > 63 { return None; }
        qname.push(part.len() as u8);
        qname.extend_from_slice(part.as_bytes());
    }
    qname.push(0);

    // 2. DNS query payload (header 12B + question)
    let txid: u16 = 0x1234;
    let mut dns = Vec::with_capacity(12 + qname.len() + 4);
    dns.extend_from_slice(&txid.to_be_bytes());     // TXID
    dns.extend_from_slice(&[0x01, 0x00]);             // flags: standard query
    dns.extend_from_slice(&[0x00, 0x01]);             // QDCOUNT: 1 question
    dns.extend_from_slice(&[0x00, 0x00]);             // ANCOUNT
    dns.extend_from_slice(&[0x00, 0x00]);             // NSCOUNT
    dns.extend_from_slice(&[0x00, 0x00]);             // ARCOUNT
    dns.extend_from_slice(&qname);                     // question name
    dns.extend_from_slice(&[0x00, 0x01]);             // QTYPE: A
    dns.extend_from_slice(&[0x00, 0x01]);             // QCLASS: IN

    // 3. UDP header (8B) + DNS payload
    let src_port: u16 = 54321;
    let dst_port: u16 = 53;
    let udp_len = 8usize.checked_add(dns.len())?;
    let udp_len_u16 = u16::try_from(udp_len).ok()?;
    let mut udp_data = Vec::with_capacity(udp_len);
    udp_data.extend_from_slice(&src_port.to_be_bytes());
    udp_data.extend_from_slice(&dst_port.to_be_bytes());
    udp_data.extend_from_slice(&udp_len_u16.to_be_bytes());
    udp_data.extend_from_slice(&[0x00, 0x00]); // checksum = 0
    udp_data.extend_from_slice(&dns);

    // 4. IP header (20B)
    let total_len = 20usize.checked_add(udp_data.len())?;
    let total_len_u16 = u16::try_from(total_len).ok()?;
    let mut ip = [0u8; 20];
    ip[0] = 0x45;
    ip[1] = 0;
    ip[2..4].copy_from_slice(&total_len_u16.to_be_bytes());
    ip[4..8].copy_from_slice(&[0, 0, 0x40, 0x00]); // ID + flags/frag
    ip[8] = 64;                                     // TTL
    ip[9] = 17;                                     // UDP
    ip[10..12].copy_from_slice(&[0, 0]);            // checksum placeholder
    ip[12..16].copy_from_slice(&[10, 0, 2, 15]); // src IP
    ip[16..20].copy_from_slice(&dns_server);       // dst IP
    let cs = ip_checksum(&ip);
    ip[10..12].copy_from_slice(&cs.to_be_bytes());

    // 5. Serial SLIP tunnel carries raw IP (no Ethernet header)
    let mut frame = Vec::with_capacity(20 + udp_data.len());
    frame.extend_from_slice(&ip);
    frame.extend_from_slice(&udp_data);

    k_nano::slog_bin!("DNS", "MANUAL", "Resolvendo {} -> {}.{}.{}.{} ({} bytes)",
        hostname, dns_server[0], dns_server[1], dns_server[2], dns_server[3], frame.len());

    unsafe { slip::send(&frame); }

    // 6. Poll for response (multi-answer parser)
    for i in 0..200 {
        if let Some(resp) = unsafe { slip::recv() } {
            if resp.len() < 42 { continue; }
            let dns_offset = 20 + 8; // skip IP + UDP
            let resp_txid = u16::from_be_bytes([resp[dns_offset], resp[dns_offset + 1]]);
            if resp_txid != txid { continue; }
            let flags = u16::from_be_bytes([resp[dns_offset + 2], resp[dns_offset + 3]]);
            if flags & 0x8000 == 0 { continue; }
            let ancount = u16::from_be_bytes([resp[dns_offset + 6], resp[dns_offset + 7]]);
            if ancount == 0 { continue; }

            // Skip question section
            let mut pos = dns_offset + 12;
            while pos < resp.len() && resp[pos] != 0 {
                if resp[pos] & 0xC0 == 0xC0 { pos += 2; break; }
                let step = 1usize.saturating_add(resp[pos] as usize);
                pos = pos.saturating_add(step);
            }
            if pos >= resp.len() { continue; }
            pos += 5; // null term + QTYPE + QCLASS

            // Parse answers
            for _ in 0..ancount {
                if pos >= resp.len() { break; }
                // Name field (can be pointer or sequence)
                if resp[pos] & 0xC0 == 0xC0 {
                    pos = pos.saturating_add(2);
                } else {
                    while pos < resp.len() && resp[pos] != 0 {
                        let step = 1usize.saturating_add(resp[pos] as usize);
                        let Some(next) = pos.checked_add(step) else { break; };
                        pos = next;
                    }
                    pos = pos.saturating_add(1);
                }
                if pos + 10 > resp.len() { break; }
                let rr_type = u16::from_be_bytes([resp[pos], resp[pos + 1]]);
                let _rr_class = u16::from_be_bytes([resp[pos + 2], resp[pos + 3]]);
                let rdlen = u16::from_be_bytes([resp[pos + 8], resp[pos + 9]]) as usize;
                pos += 10;
                let Some(rdata_end) = pos.checked_add(rdlen) else { break; };
                if rdata_end > resp.len() { break; }
                if rr_type == 1 && rdlen == 4 {
                    let ip = [resp[pos], resp[pos + 1], resp[pos + 2], resp[pos + 3]];
                    k_nano::slog_bin!("DNS", "MANUAL", "OK: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
                    return Some(ip);
                }
                pos = rdata_end;
            }
        }
        if i % 50 == 0 {
            unsafe { slip::send(&frame); }
        }
    }
    k_nano::slog_bin!("DNS", "MANUAL", "Timeout");
    None
}
use smoltcp::iface::{Config, Interface, SocketSet, SocketHandle};
use smoltcp::phy::{Checksum, ChecksumCapabilities, Device, DeviceCapabilities, Medium, RxToken, TxToken};

static NET_TX_COUNT: AtomicU64 = AtomicU64::new(0);
static NET_RX_COUNT: AtomicU64 = AtomicU64::new(0);
pub fn net_tx_count() -> u64 { NET_TX_COUNT.load(Ordering::Relaxed) }
pub fn net_rx_count() -> u64 { NET_RX_COUNT.load(Ordering::Relaxed) }

/// Wall-clock pause (pré-sti / sem TIMER). Conservador @2GHz: 1µs ≈ 2000 ciclos.
/// Necessário para QEMU slirp injetar ARP/DNS no e1000 — Instant fake sem delay = RX=0.
pub(crate) fn wall_pause_us(us: u64) {
    let cycles = us.saturating_mul(2_000);
    let start = unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem, preserves_flags));
        ((hi as u64) << 32) | (lo as u64)
    };
    loop {
        let now = unsafe {
            let lo: u32;
            let hi: u32;
            core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem, preserves_flags));
            ((hi as u64) << 32) | (lo as u64)
        };
        if now.wrapping_sub(start) >= cycles {
            break;
        }
        core::hint::spin_loop();
    }
}
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, Ipv4Address, IpCidr};
use smoltcp::socket::tcp::{self, State as TcpState, Socket as TcpSocket};
use smoltcp::socket::udp as udp_socket;
use smoltcp::socket::dhcpv4::{Event as DhcpEvent, Socket as DhcpSocket};
use crate::net::VIRTIO_DEV;

pub struct PhyToken(pub Vec<u8>);

impl RxToken for PhyToken {
    fn consume<R, F>(self, f: F) -> R
    where F: FnOnce(&[u8]) -> R {
        NET_RX_COUNT.fetch_add(1, Ordering::Relaxed);
        f(&self.0)
    }
}

impl TxToken for PhyToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where F: FnOnce(&mut [u8]) -> R {
        NET_TX_COUNT.fetch_add(1, Ordering::Relaxed);
        let mut buf = vec![0u8; len];
        let r = f(&mut buf);
        unsafe { nic_send(buf) };
        r
    }
}

unsafe fn nic_send(data: Vec<u8>) {
    // Labor 29: SoftMAC wifi path (Note) — antes dos NICs wired QEMU
    if crate::wifi_softmac::is_enabled() {
        if crate::wifi_softmac::push_tx_eth(&data) {
            return;
        }
    }
    // VirtIO-Net (mais rapido em QEMU)
    if let Some(ref mut nic) = *VIRTIO_DEV.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::net::E1000.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::net::I225.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::net::RTL8139.lock() {
        nic.send(&data); return;
    }
    crate::generic_wifi::ACTIVE_DRIVER.lock(|driver| {
        if let Some(wifi) = driver { let _ = wifi.send_packet(&data); }
    });
    crate::slip::send(&data);
}

unsafe fn nic_recv() -> Option<Vec<u8>> {
    // Labor 29: SoftMAC RX first when armed
    if crate::wifi_softmac::is_enabled() {
        if let Some(pkt) = crate::wifi_softmac::pop_rx_eth() {
            return Some(pkt);
        }
    }
    // VirtIO-Net first (mais rapido em QEMU)
    if let Some(ref mut nic) = *VIRTIO_DEV.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::net::E1000.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::net::I225.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::net::RTL8139.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    // Generic WiFi driver — bridge formal smoltcp::phy::Device via WifiChipset trait
    let mut wifi_pkt: Option<Vec<u8>> = None;
    crate::generic_wifi::ACTIVE_DRIVER.lock(|driver| {
        if let Some(wifi) = driver {
            let mut rx_buf = [0u8; 1518];
            if let Ok(n) = wifi.receive_packet(&mut rx_buf) {
                if n > 0 {
                    wifi_pkt = Some(rx_buf[..n].to_vec());
                }
            }
        }
    });
    if let Some(pkt) = wifi_pkt {
        return Some(pkt);
    }
    // Serial tunnel (SLIP) — tenta sempre como fallback universal
    if let Some(pkt) = crate::slip::recv() {
        return Some(pkt);
    }
    None
}

pub struct NetPhy;

impl Device for NetPhy {
    type RxToken<'x> = PhyToken;
    type TxToken<'x> = PhyToken;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        // Ethernet MTU = eth header (14) + IP MTU — ver docs smoltcp DeviceCapabilities.
        caps.max_transmission_unit = 1514;
        caps.medium = Medium::Ethernet;
        // QEMU/slirp às vezes entrega UDP/TCP com checksum 0 ou offload; verificar RX
        // descarta pacotes já contados em NET_RX_COUNT → DNS/HTTP “timeout fantasma”.
        let mut csum = ChecksumCapabilities::ignored();
        csum.ipv4 = Checksum::Both;
        csum.udp = Checksum::Tx;
        // TCP valida RX também (SESSION_252): ANTES era só Tx — o smoltcp aceitava
        // payload corrompido (checksum ignorado no RX) e o TCP não retransmitia →
        // download grande (KERNEL.BIN 17MB) vinha com bytes errados e mesmo
        // tamanho → hash_mismatch no OTA. Com Both, segmento ruim é descartado
        // e retransmitido — download íntegro.
        csum.tcp = Checksum::Both;
        csum.icmpv4 = Checksum::Tx;
        caps.checksum = csum;
        caps
    }

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let data = unsafe { nic_recv() };
        data.map(|d| (PhyToken(d), PhyToken(vec![])))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(PhyToken(vec![]))
    }
}

pub enum HttpState {
    Connecting,
    Sending,
    Waiting,
    Receiving,
    Done(Vec<u8>),
    Failed,
}

pub struct HttpConn {
    handle: SocketHandle,
    pub state: HttpState,
    request: alloc::string::String,
    started: bool,
    pub buf: Vec<u8>,
    timeout: u32,
    /// Content-Length do header HTTP (0 = não informado). Corpo truncado
    /// (< expect_len) NUNCA é sucesso — RST/FIN precoce do slirp no OTA
    /// (SESSION_252) produzia "download completo" com ~1748 bytes a menos.
    pub expect_len: usize,
    /// Offset onde o corpo começa (fim do header HTTP, após \r\n\r\n).
    pub header_len: usize,
}

pub struct NetStack {
    iface: Interface,
    sockets: SocketSet<'static>,
    phy: NetPhy,
    dhcp_handle: SocketHandle,
    pub dhcp_done: bool,
    pub has_static_ip: bool,
    pub tx_count: u64,
    pub rx_count: u64,
}

fn ip_to_u32(ip: [u8; 4]) -> u32 {
    (ip[0] as u32) << 24 | (ip[1] as u32) << 16 | (ip[2] as u32) << 8 | ip[3] as u32
}

/// Busca manual de substring (no_std, sem memmem). Retorna offset da 1ª ocorrência.
fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    for i in 0..=haystack.len() - needle.len() {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

impl NetStack {
    pub fn new(mac: [u8; 6]) -> Self {
        let eth = EthernetAddress::from_bytes(&mac);
        let config = Config::new(HardwareAddress::Ethernet(eth));
        let now = Instant::from_millis(0);
        let mut phy = NetPhy;
        let iface = Interface::new(config, &mut phy, now);
        let mut sockets = SocketSet::new(vec![]);

        // DHCP socket — auto-discovery via broadcast
        let dhcp = DhcpSocket::new();
        let dhcp_handle = sockets.add(dhcp);

        NetStack { iface, sockets, phy, dhcp_handle, dhcp_done: false, has_static_ip: false, tx_count: 0, rx_count: 0 }
    }

    /// Configura IP estatico para QEMU user-mode (10.0.2.15/24 padrao) ou custom (mesh P2P)
    pub fn set_static_ip(&mut self, custom_ip: Option<[u8; 4]>) {
        let (ip_bytes, gw_byte3): ([u8; 4], u8) = match custom_ip {
            Some(ip) => (ip, ip[2]),
            None => ([10, 0, 2, 15], 2),
        };
        let ip = Ipv4Address::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);
        let cidr = IpCidr::new(IpAddress::Ipv4(ip), 24);
        self.iface.update_ip_addrs(|addrs| { addrs.push(cidr).ok(); });
        let gw = Ipv4Address::new(ip_bytes[0], ip_bytes[1], gw_byte3, 1);
        self.iface.routes_mut().add_default_ipv4_route(gw.into()).ok();
        self.dhcp_done = true;
        self.has_static_ip = true;
        let dns_ip = [ip_bytes[0], ip_bytes[1], gw_byte3, 3];
        {
            let mut cfg = crate::net::NET_CONFIG.lock();
            cfg.ip = ip_bytes;
            cfg.gateway_ip = [ip_bytes[0], ip_bytes[1], gw_byte3, 1];
            cfg.subnet_mask = [255, 255, 255, 0];
            cfg.dns_ip = dns_ip;
            cfg.configured = true;
            cfg.online = true;
        }
        // SESSION_234: sincroniza MAC/IP para o transporte P2P do k_nano (R0).
        k_nano::net::set_nic_config(crate::net::NET_CONFIG.lock().mac, ip_bytes);
        k_nano::slog_hermes!("Net", "info",
            "Static IP: {}.{}.{}.{}/24 gw={}.{}.{}.1 dns={}.{}.{}.3",
            ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3],
            ip_bytes[0], ip_bytes[1], gw_byte3,
            ip_bytes[0], ip_bytes[1], gw_byte3);
    }

    pub fn poll(&mut self, now_ms: i64) {
        let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
        let now = Instant::from_millis(now_ms);
        // Poll in tight loop until no more work — required for DHCP multi-step
        loop {
            let _ = iface.poll(now, phy, sockets);
            match iface.poll_delay(now, sockets) {
                Some(smoltcp::time::Duration::ZERO) => continue,
                _ => break,
            }
        }
        // Fast-path para serial tunnel: se COM2 tem dados, poll novamente imediatamente
        // Isso evita o delay de 1 tick (~55ms) entre a chegada do dado e o processamento
        if crate::env::is_sandbox() && unsafe { crate::slip::has_data() } {
            let _ = iface.poll(now, phy, sockets);
        }
    }

    /// Poll DHCP — multi-step DISCOVER→OFFER→REQUEST→ACK
    pub fn dhcp_poll(&mut self, now_ms: i64) -> (bool, [u8; 4], [u8; 4]) {
        let Self { ref mut iface, ref mut phy, ref mut sockets, ref mut dhcp_done, .. } = self;
        let now = Instant::from_millis(now_ms);
        // Tight poll loop
        loop {
            let _ = iface.poll(now, phy, sockets);
            match iface.poll_delay(now, sockets) {
                Some(smoltcp::time::Duration::ZERO) => continue,
                _ => break,
            }
        }

        let dhcp = sockets.get_mut::<DhcpSocket>(self.dhcp_handle);
        if let Some(event) = dhcp.poll() {
            match event {
                DhcpEvent::Configured(config) => {
                    // Apply IP address from DHCP
                    let cidr = smoltcp::wire::IpCidr::Ipv4(config.address);
                    iface.update_ip_addrs(|addrs| { addrs.push(cidr).ok(); });
                    // Apply default route via DHCP router
                    if let Some(router) = config.router {
                        iface.routes_mut().add_default_ipv4_route(router.into()).ok();
                    }
                    let gw = config.router.map(|r| r.octets()).unwrap_or([0; 4]);
                    let dns = config.dns_servers.first().map(|s| s.octets()).unwrap_or([10, 0, 2, 3]);
                    *dhcp_done = true;
                    return (true, gw, dns);
                }
                DhcpEvent::Deconfigured => {
                    *dhcp_done = false;
                }
            }
        }
        (false, [0; 4], [0; 4])
    }

    pub fn http_new(&mut self, host: [u8; 4], port: u16, path: &str) -> HttpConn {
        self.http_new_host(host, port, path, None)
    }

    /// HTTP GET with optional Host header (hostname for vhosts / CDN).
    pub fn http_new_host(
        &mut self,
        host: [u8; 4],
        port: u16,
        path: &str,
        host_header: Option<&str>,
    ) -> HttpConn {
        self.http_new_host_ex(host, port, path, host_header, None)
    }

    /// HTTP GET + `Range: bytes=start-end` (AirLLM stream-to-disk).
    pub fn http_new_host_ranged(
        &mut self,
        host: [u8; 4],
        port: u16,
        path: &str,
        host_header: Option<&str>,
        start: usize,
        end: usize,
    ) -> HttpConn {
        self.http_new_host_ex(host, port, path, host_header, Some((start, end)))
    }

    fn http_new_host_ex(
        &mut self,
        host: [u8; 4],
        port: u16,
        path: &str,
        host_header: Option<&str>,
        range: Option<(usize, usize)>,
    ) -> HttpConn {
        let tcp_rx = tcp::SocketBuffer::new(vec![0u8; 8192]);
        let tcp_tx = tcp::SocketBuffer::new(vec![0u8; 4096]);
        let tcp = TcpSocket::new(tcp_rx, tcp_tx);
        let handle = self.sockets.add(tcp);

        let remote = (IpAddress::v4(host[0], host[1], host[2], host[3]), port);
        let tcp = self.sockets.get_mut::<TcpSocket>(handle);
        let context = self.iface.context();
        let local_port: u16 = 49152u16.wrapping_add((net_tx_count() as u16) & 0x3fff);
        let _ = tcp.connect(context, remote, local_port);

        let host_line = match host_header {
            Some(h) if !h.is_empty() => alloc::string::String::from(h),
            _ => alloc::format!("{}.{}.{}.{}", host[0], host[1], host[2], host[3]),
        };
        let request = if let Some((start, end)) = range {
            alloc::format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nRange: bytes={}-{}\r\nConnection: close\r\n\r\n",
                path, host_line, start, end
            )
        } else {
            alloc::format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                path, host_line
            )
        };

        HttpConn {
            handle,
            state: HttpState::Connecting,
            request,
            started: false,
            buf: Vec::new(),
            timeout: 0,
            expect_len: 0,
            header_len: 0,
        }
    }

    /// TCP connect → send payload → recv until close/timeout (NetFs / SMTP framing).
    pub fn tcp_exchange(&mut self, host: [u8; 4], port: u16, payload: &[u8], now: u64) -> Option<Vec<u8>> {
        let tcp_rx = tcp::SocketBuffer::new(vec![0u8; 8192]);
        let tcp_tx = tcp::SocketBuffer::new(vec![0u8; 8192]);
        let tcp = TcpSocket::new(tcp_rx, tcp_tx);
        let handle = self.sockets.add(tcp);
        let remote = (IpAddress::v4(host[0], host[1], host[2], host[3]), port);
        {
            let tcp = self.sockets.get_mut::<TcpSocket>(handle);
            let context = self.iface.context();
            let local_port: u16 = 50000u16.wrapping_add((net_tx_count() as u16) & 0x3fff);
            let _ = tcp.connect(context, remote, local_port);
        }
        let mut sent = false;
        let mut buf = Vec::new();
        for _ in 0..8_000 {
            let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
            iface.poll(Instant::from_millis(now as i64), phy, sockets);
            let tcp = sockets.get_mut::<TcpSocket>(handle);
            match tcp.state() {
                TcpState::Established => {
                    if !sent {
                        let _ = tcp.send_slice(payload);
                        sent = true;
                    }
                    if tcp.can_recv() {
                        if let Ok(chunk) = tcp.recv(|data| {
                            let v = Vec::from(&*data);
                            (data.len(), v)
                        }) {
                            buf.extend_from_slice(&chunk);
                        }
                    }
                }
                TcpState::CloseWait => {
                    if tcp.can_recv() {
                        if let Ok(chunk) = tcp.recv(|data| {
                            let v = Vec::from(&*data);
                            (data.len(), v)
                        }) {
                            buf.extend_from_slice(&chunk);
                        }
                    }
                    tcp.close();
                    break;
                }
                TcpState::Closed | TcpState::Closing => break,
                TcpState::SynSent | TcpState::SynReceived => {}
                _ => {
                    if sent && !buf.is_empty() {
                        break;
                    }
                }
            }
            core::hint::spin_loop();
        }
        {
            let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
            let tcp = sockets.get_mut::<TcpSocket>(handle);
            tcp.close();
            iface.poll(Instant::from_millis(now as i64), phy, sockets);
            sockets.remove(handle);
        }
        if buf.is_empty() { None } else { Some(buf) }
    }

    pub fn http_poll(&mut self, conn: &mut HttpConn, now: u64) {
        let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
        // CLOCK FIX (SESSION_252 / ora-1): `now` = TIMER_TICKS incrementado pelo
        // PIT a ~18.2Hz (~55ms/tick). ANTES era passado como ms — o relógio do
        // smoltcp rodava ~55× mais devagar que o real: delayed-ACK (~40ms no
        // smoltcp) virava ~2.2s reais → slirp (RTO 1s) retransmitia, backoff
        // estourava e ABORTAVA a conexão com RST — downloads grandes (KERNEL.BIN
        // 17MB) truncavam ~1748 bytes no fim → hash_mismatch no OTA.
        iface.poll(Instant::from_millis(now.saturating_mul(55) as i64), phy, sockets);

        conn.timeout = conn.timeout.wrapping_add(1);
        // Limite alto: downloads grandes (KERNEL.BIN ~17MB) sob TCG/slirp drenam
        // ~2KB por poll — timeout 8000 cortava em ~17.4MB (hash_mismatch no OTA,
        // SESSION_252). O server manda Connection: close → Done no fim do corpo.
        if conn.timeout > 200_000 {
            conn.state = HttpState::Failed;
            return;
        }

        let tcp = sockets.get_mut::<TcpSocket>(conn.handle);

        match tcp.state() {
            TcpState::SynSent | TcpState::SynReceived => {
                conn.state = HttpState::Connecting;
            }
            TcpState::Established => {
                if !conn.started {
                    let _ = tcp.send_slice(conn.request.as_bytes());
                    conn.started = true;
                    conn.state = HttpState::Sending;
                } else if tcp.can_recv() {
                    let result = tcp.recv(|data| {
                        let v = Vec::from(&*data);
                        (data.len(), v)
                    });
                    match result {
                        Ok(data) => {
                            conn.buf.extend_from_slice(&data);
                            conn.state = HttpState::Receiving;
                            // Parseia Content-Length na primeira recepção (header
                            // completo). SESSION_252/ora-1: corpo truncado nunca
                            // é sucesso — RST/FIN precoce do slirp cortava ~1748B.
                            if conn.expect_len == 0 && conn.header_len == 0 {
                                Self::parse_http_meta(conn);
                            }
                        }
                        Err(_) => conn.state = HttpState::Failed,
                    }
                } else {
                    conn.state = HttpState::Waiting;
                }
            }
            TcpState::CloseWait => {
                // Drena TUDO o que ainda resta (RST/FIN precoce deixa dados no
                // buffer — drenar uma vez só não basta para downloads grandes).
                while tcp.can_recv() {
                    let result = tcp.recv(|data| {
                        let v = Vec::from(&*data);
                        (data.len(), v)
                    });
                    match result {
                        Ok(data) => conn.buf.extend_from_slice(&data),
                        Err(_) => break,
                    }
                }
                tcp.close();
                if Self::http_complete(conn) {
                    let data = core::mem::take(&mut conn.buf);
                    conn.state = HttpState::Done(data);
                } else {
                    conn.state = HttpState::Failed;
                }
            }
            TcpState::Closed | TcpState::Closing => {
                // RST do slirp: pode ter dado ainda no buffer — drena e valida.
                while tcp.can_recv() {
                    let result = tcp.recv(|data| {
                        let v = Vec::from(&*data);
                        (data.len(), v)
                    });
                    match result {
                        Ok(data) => conn.buf.extend_from_slice(&data),
                        Err(_) => break,
                    }
                }
                if Self::http_complete(conn) {
                    let data = core::mem::take(&mut conn.buf);
                    conn.state = HttpState::Done(data);
                } else {
                    conn.state = HttpState::Failed;
                }
            }
            _ => {
                conn.state = HttpState::Failed;
            }
        }
    }

    /// Extrai `Content-Length` do header HTTP (após \r\n\r\n) e o offset do corpo.
    fn parse_http_meta(conn: &mut HttpConn) {
        let n = conn.buf.len();
        let mut header_end = 0usize;
        for i in 0..n.saturating_sub(3) {
            if conn.buf[i] == b'\r'
                && conn.buf[i + 1] == b'\n'
                && conn.buf[i + 2] == b'\r'
                && conn.buf[i + 3] == b'\n'
            {
                header_end = i + 4;
                break;
            }
        }
        if header_end == 0 {
            return; // header ainda incompleto
        }
        conn.header_len = header_end;
        // Content-Length: "content-length:" case-insensitive no header.
        let head = &conn.buf[..header_end];
        let lower = head.to_ascii_lowercase();
        let needle = b"content-length:";
        if let Some(pos) = find_sub(&lower, needle) {
            let start = pos + needle.len();
            let mut end = start;
            while end < lower.len() && (lower[end] as char).is_ascii_digit() {
                end += 1;
            }
            if let Ok(cl) = core::str::from_utf8(&lower[start..end]).unwrap_or("").trim().parse::<usize>() {
                conn.expect_len = cl;
            }
        }
    }

    /// True se o corpo recebido tem o Content-Length esperado (ou não informado).
    fn http_complete(conn: &HttpConn) -> bool {
        if conn.expect_len == 0 {
            // Sem Content-Length: aceita se não vazio (legado).
            return !conn.buf.is_empty();
        }
        // buf = header + body; body = buf.len() - header_len.
        let body = conn.buf.len().saturating_sub(conn.header_len);
        body >= conn.expect_len
    }

    /// Envia dados brutos via TCP (nao HTTP) — usado por SMTP
    pub fn http_send_raw(&mut self, conn: &mut HttpConn, data: &[u8]) {
        conn.request = alloc::string::String::from(core::str::from_utf8(data).unwrap_or(""));
    }
    pub fn http_close(&mut self, conn: &mut HttpConn) {
        let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
        let tcp = sockets.get_mut::<TcpSocket>(conn.handle);
        tcp.close();
        iface.poll(Instant::from_millis(0), phy, sockets);
        sockets.remove(conn.handle);
    }

    /// DNS via Ethernet+IP+UDP raw no NIC (bypass demux smoltcp).
    /// Necessário: smoltcp perde o 1º UDP no ARP e, mesmo com resend, o demux
    /// sob QEMU/slirp não entrega a resposta ao socket (evidência SESSION_149).
    pub fn dns_resolve(&mut self, hostname: &str, dns_server: [u8; 4]) -> Option<[u8; 4]> {
        let tx0 = net_tx_count();
        let rx0 = net_rx_count();
        let (sip, smac) = {
            let cfg = crate::net::NET_CONFIG.lock();
            let sip = if cfg.ip != [0; 4] { cfg.ip } else { [10, 0, 2, 15] };
            (sip, cfg.mac)
        };
        if smac == [0; 6] {
            k_nano::slog_bin!("DNS", "info", "raw SKIP: MAC zero");
            return None;
        }

        let dmac = match Self::arp_resolve_raw(sip, dns_server, smac) {
            Some(m) => m,
            None => {
                k_nano::slog_bin!("DNS", "info", "raw ARP timeout for {}.{}.{}.{}",
                    dns_server[0], dns_server[1], dns_server[2], dns_server[3]);
                return None;
            }
        };

        let txid: u16 = 0x1234;
        let qname = Self::encode_dns_name(hostname);
        let mut dns = Vec::with_capacity(12 + qname.len() + 4);
        dns.extend_from_slice(&txid.to_be_bytes());
        dns.extend_from_slice(&[0x01, 0x00]);
        dns.extend_from_slice(&[0x00, 0x01]);
        dns.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        dns.extend_from_slice(&qname);
        dns.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);

        let src_port: u16 = 54321;
        let udp_len = (8 + dns.len()) as u16;
        let mut udp = Vec::with_capacity(udp_len as usize);
        udp.extend_from_slice(&src_port.to_be_bytes());
        udp.extend_from_slice(&53u16.to_be_bytes());
        udp.extend_from_slice(&udp_len.to_be_bytes());
        udp.extend_from_slice(&[0x00, 0x00]); // UDP checksum opcional = 0
        udp.extend_from_slice(&dns);

        let total_len = (20 + udp.len()) as u16;
        let mut ip = [0u8; 20];
        ip[0] = 0x45;
        ip[2..4].copy_from_slice(&total_len.to_be_bytes());
        ip[8] = 64;
        ip[9] = 17;
        ip[12..16].copy_from_slice(&sip);
        ip[16..20].copy_from_slice(&dns_server);
        let cs = ip_checksum(&ip);
        ip[10..12].copy_from_slice(&cs.to_be_bytes());

        let mut frame = Vec::with_capacity(14 + 20 + udp.len());
        frame.extend_from_slice(&dmac);
        frame.extend_from_slice(&smac);
        frame.extend_from_slice(&[0x08, 0x00]);
        frame.extend_from_slice(&ip);
        frame.extend_from_slice(&udp);

        for attempt in 0..8u32 {
            unsafe { nic_send(frame.clone()) };
            NET_TX_COUNT.fetch_add(1, Ordering::Relaxed);
            for _ in 0..600u32 {
                wall_pause_us(500);
                let pkt = unsafe { nic_recv() };
                let Some(pkt) = pkt else { continue };
                NET_RX_COUNT.fetch_add(1, Ordering::Relaxed);
                if let Some(ip) = Self::parse_dns_udp_reply(&pkt, txid, src_port) {
                    let _ = self.prime_neighbor_for_http();
                    k_nano::slog_bin!(
                        "DNS",
                        "info",
                        "OK raw {}.{}.{}.{} (dtx={} drx={} attempt={})",
                        ip[0], ip[1], ip[2], ip[3],
                        net_tx_count().saturating_sub(tx0),
                        net_rx_count().saturating_sub(rx0),
                        attempt + 1
                    );
                    return Some(ip);
                }
            }
        }
        k_nano::slog_bin!(
            "DNS",
            "info",
            "timeout raw dtx={} drx={} tx={} rx={}",
            net_tx_count().saturating_sub(tx0),
            net_rx_count().saturating_sub(rx0),
            net_tx_count(),
            net_rx_count()
        );
        None
    }

    /// ARP who-has via NIC raw; devolve MAC do alvo.
    fn arp_resolve_raw(sip: [u8; 4], tip: [u8; 4], smac: [u8; 6]) -> Option<[u8; 6]> {
        let mut req = [0u8; 42];
        req[0..6].copy_from_slice(&[0xff; 6]);
        req[6..12].copy_from_slice(&smac);
        req[12] = 0x08;
        req[13] = 0x06;
        req[14] = 0x00;
        req[15] = 0x01;
        req[16] = 0x08;
        req[17] = 0x00;
        req[18] = 6;
        req[19] = 4;
        req[20] = 0x00;
        req[21] = 0x01;
        req[22..28].copy_from_slice(&smac);
        req[28..32].copy_from_slice(&sip);
        req[38..42].copy_from_slice(&tip);

        for _ in 0..5u32 {
            unsafe { nic_send(req.to_vec()) };
            NET_TX_COUNT.fetch_add(1, Ordering::Relaxed);
            for _ in 0..400u32 {
                wall_pause_us(500);
                let Some(pkt) = (unsafe { nic_recv() }) else { continue };
                NET_RX_COUNT.fetch_add(1, Ordering::Relaxed);
                if pkt.len() < 42 {
                    continue;
                }
                if pkt[12] != 0x08 || pkt[13] != 0x06 {
                    continue;
                }
                let oper = u16::from_be_bytes([pkt[20], pkt[21]]);
                if oper != 2 {
                    continue;
                }
                let spa = [pkt[28], pkt[29], pkt[30], pkt[31]];
                if spa != tip {
                    continue;
                }
                let mut mac = [0u8; 6];
                mac.copy_from_slice(&pkt[22..28]);
                return Some(mac);
            }
        }
        None
    }

    /// UDP request/response raw no NIC (mesmo caminho DNS SESSION_149).
    /// Retorna payload UDP (sem headers Ethernet/IP/UDP) se `sport==dst_port` e `dport==src_port`.
    pub fn udp_exchange_raw(
        &mut self,
        dst: [u8; 4],
        dst_port: u16,
        payload: &[u8],
    ) -> Option<Vec<u8>> {
        let (sip, smac) = {
            let cfg = crate::net::NET_CONFIG.lock();
            let sip = if cfg.ip != [0; 4] { cfg.ip } else { [10, 0, 2, 15] };
            (sip, cfg.mac)
        };
        if smac == [0; 6] || payload.is_empty() {
            return None;
        }
        let dmac = Self::arp_resolve_raw(sip, dst, smac)?;
        let src_port: u16 = 54323;
        let udp_len = (8 + payload.len()) as u16;
        let mut udp = Vec::with_capacity(udp_len as usize);
        udp.extend_from_slice(&src_port.to_be_bytes());
        udp.extend_from_slice(&dst_port.to_be_bytes());
        udp.extend_from_slice(&udp_len.to_be_bytes());
        udp.extend_from_slice(&[0x00, 0x00]);
        udp.extend_from_slice(payload);

        let total_len = (20 + udp.len()) as u16;
        let mut ip = [0u8; 20];
        ip[0] = 0x45;
        ip[2..4].copy_from_slice(&total_len.to_be_bytes());
        ip[8] = 64;
        ip[9] = 17;
        ip[12..16].copy_from_slice(&sip);
        ip[16..20].copy_from_slice(&dst);
        let cs = ip_checksum(&ip);
        ip[10..12].copy_from_slice(&cs.to_be_bytes());

        let mut frame = Vec::with_capacity(14 + 20 + udp.len());
        frame.extend_from_slice(&dmac);
        frame.extend_from_slice(&smac);
        frame.extend_from_slice(&[0x08, 0x00]);
        frame.extend_from_slice(&ip);
        frame.extend_from_slice(&udp);

        for _attempt in 0..6u32 {
            unsafe { nic_send(frame.clone()) };
            NET_TX_COUNT.fetch_add(1, Ordering::Relaxed);
            for _ in 0..800u32 {
                wall_pause_us(500);
                let pkt = unsafe { nic_recv() };
                let Some(pkt) = pkt else { continue };
                NET_RX_COUNT.fetch_add(1, Ordering::Relaxed);
                if let Some(pl) = Self::parse_udp_payload(&pkt, src_port, dst_port) {
                    return Some(pl);
                }
            }
        }
        None
    }

    fn parse_udp_payload(pkt: &[u8], local_port: u16, expect_sport: u16) -> Option<Vec<u8>> {
        if pkt.len() < 14 + 20 + 8 {
            return None;
        }
        if pkt[12] != 0x08 || pkt[13] != 0x00 {
            return None;
        }
        let ihl = (pkt[14] & 0x0f) as usize * 4;
        if ihl < 20 || pkt.len() < 14 + ihl + 8 {
            return None;
        }
        if pkt[14 + 9] != 17 {
            return None;
        }
        let udp = 14 + ihl;
        let sport = u16::from_be_bytes([pkt[udp], pkt[udp + 1]]);
        let dport = u16::from_be_bytes([pkt[udp + 2], pkt[udp + 3]]);
        if sport != expect_sport || dport != local_port {
            return None;
        }
        let ulen = u16::from_be_bytes([pkt[udp + 4], pkt[udp + 5]]) as usize;
        if ulen < 8 || pkt.len() < udp + ulen {
            return None;
        }
        Some(pkt[udp + 8..udp + ulen].to_vec())
    }

    fn parse_dns_udp_reply(pkt: &[u8], txid: u16, local_port: u16) -> Option<[u8; 4]> {
        if pkt.len() < 14 + 20 + 8 + 12 {
            return None;
        }
        if pkt[12] != 0x08 || pkt[13] != 0x00 {
            return None;
        }
        let ihl = (pkt[14] & 0x0f) as usize * 4;
        if ihl < 20 || pkt.len() < 14 + ihl + 8 + 12 {
            return None;
        }
        if pkt[14 + 9] != 17 {
            return None;
        }
        let udp = 14 + ihl;
        let sport = u16::from_be_bytes([pkt[udp], pkt[udp + 1]]);
        let dport = u16::from_be_bytes([pkt[udp + 2], pkt[udp + 3]]);
        if sport != 53 || dport != local_port {
            return None;
        }
        let dns_off = udp + 8;
        let data = &pkt[dns_off..];
        if data.len() < 12 {
            return None;
        }
        let resp_txid = u16::from_be_bytes([data[0], data[1]]);
        if resp_txid != txid {
            return None;
        }
        let flags = u16::from_be_bytes([data[2], data[3]]);
        if flags & 0x8000 == 0 {
            return None;
        }
        let ancount = u16::from_be_bytes([data[6], data[7]]);
        if ancount == 0 {
            return None;
        }
        // Pula question name + QTYPE/QCLASS (skip no wire; pointer 0xC0xx = 2 bytes).
        let mut pos = Self::skip_dns_name(data, 12);
        pos += 4;
        for _ in 0..ancount {
            if pos + 10 > data.len() {
                break;
            }
            pos = Self::skip_dns_name(data, pos);
            if pos + 10 > data.len() {
                break;
            }
            let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let rclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
            let rdlen = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
            pos += 10;
            if rtype == 1 && rclass == 1 && rdlen == 4 && pos + 4 <= data.len() {
                return Some([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            }
            // CNAME/AAAA/etc — avança RDATA
            pos = pos.saturating_add(rdlen);
        }
        None
    }

    /// Priming público do gw QEMU (10.0.2.2) antes de TCP/HTTP.
    pub fn prime_neighbor_for_http(&mut self) -> bool {
        self.prime_neighbor_smoltcp([10, 0, 2, 2])
    }

    /// Dispara UDP dummy com resends para popular NeighborCache do smoltcp (TCP/HTTP).
    fn prime_neighbor_smoltcp(&mut self, target: [u8; 4]) -> bool {
        let addr = (IpAddress::v4(target[0], target[1], target[2], target[3]), 9u16);
        let meta = vec![smoltcp::storage::PacketMetadata::<smoltcp::socket::udp::UdpMetadata>::EMPTY; 2];
        let payload = vec![0u8; 64];
        let buf_rx = udp_socket::PacketBuffer::new(meta, payload);
        let meta2 = vec![smoltcp::storage::PacketMetadata::<smoltcp::socket::udp::UdpMetadata>::EMPTY; 2];
        let payload2 = vec![0u8; 64];
        let buf_tx = udp_socket::PacketBuffer::new(meta2, payload2);
        let socket = udp_socket::Socket::new(buf_rx, buf_tx);
        let handle = self.sockets.add(socket);
        {
            let udp = self.sockets.get_mut::<udp_socket::Socket>(handle);
            let _ = udp.bind(54322);
            let _ = udp.send_slice(&[0u8], addr);
        }
        let tx_before = net_tx_count();
        for i in 0..400u64 {
            self.poll((i * 5) as i64);
            wall_pause_us(500);
            if i > 0 && i % 40 == 0 {
                let udp = self.sockets.get_mut::<udp_socket::Socket>(handle);
                if udp.can_send() {
                    let _ = udp.send_slice(&[0u8], addr);
                }
            }
            // ARP reply + possível ICMP → phy RX; se TX avançou além do ARP, OK
            if net_tx_count().saturating_sub(tx_before) >= 2 {
                self.sockets.remove(handle);
                return true;
            }
        }
        self.sockets.remove(handle);
        net_tx_count().saturating_sub(tx_before) >= 1
    }

    fn encode_dns_name(name: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        for part in name.split('.') {
            buf.push(part.len() as u8);
            buf.extend_from_slice(part.as_bytes());
        }
        buf.push(0);
        buf
    }

    /// Avança offset no wire após um DNS name (labels ou pointer 0xC0xx).
    /// Não segue o pointer — só precisa saber quantos bytes o nome ocupa na mensagem.
    fn skip_dns_name(pkt: &[u8], offset: usize) -> usize {
        let mut pos = offset;
        while pos < pkt.len() {
            let b = pkt[pos];
            if b & 0xC0 == 0xC0 {
                return pos.saturating_add(2);
            } else if b == 0 {
                return pos.saturating_add(1);
            } else {
                pos = pos.saturating_add(1 + b as usize);
            }
        }
        pos
    }

    // ── TCP session (TLS N4 / embedded-io) ────────────────────────────
    // Tempo virtual avança a cada poll (igual L5) — smoltcp precisa para SYN/retransmit.

    /// Connect TCP; spin until Established or timeout. Caller must `tcp_session_close`.
    pub fn tcp_session_connect(
        &mut self,
        host: [u8; 4],
        port: u16,
        now: u64,
    ) -> Option<SocketHandle> {
        let tcp_rx = tcp::SocketBuffer::new(vec![0u8; 16_384]);
        let tcp_tx = tcp::SocketBuffer::new(vec![0u8; 16_384]);
        let tcp = TcpSocket::new(tcp_rx, tcp_tx);
        let handle = self.sockets.add(tcp);
        let remote = (IpAddress::v4(host[0], host[1], host[2], host[3]), port);
        {
            let tcp = self.sockets.get_mut::<TcpSocket>(handle);
            let context = self.iface.context();
            let local_port: u16 = 51000u16.wrapping_add((net_tx_count() as u16) & 0x3fff);
            let _ = tcp.connect(context, remote, local_port);
        }
        let mut t = now;
        // ~6k × 200µs ≈ 1.2s wall + poll; TCG soft-crypto TLS precisa fail rápido se SYN morrer.
        for _ in 0..6_000 {
            t = t.wrapping_add(5);
            self.tcp_session_poll(t);
            wall_pause_us(200);
            let st = self.sockets.get_mut::<TcpSocket>(handle).state();
            match st {
                TcpState::Established => return Some(handle),
                TcpState::Closed | TcpState::Closing | TcpState::TimeWait => break,
                _ => {}
            }
        }
        k_nano::slog_bin!("TLS", "info", "tcp_connect timeout state");
        self.tcp_session_close(handle, t);
        None
    }

    pub fn tcp_session_poll(&mut self, now: u64) {
        let Self {
            ref mut iface,
            ref mut phy,
            ref mut sockets,
            ..
        } = self;
        iface.poll(Instant::from_millis(now as i64), phy, sockets);
    }

    pub fn tcp_session_send(
        &mut self,
        handle: SocketHandle,
        data: &[u8],
        now: u64,
    ) -> Result<usize, ()> {
        let mut offset = 0usize;
        let mut t = now;
        for _ in 0..8_000 {
            t = t.wrapping_add(5);
            self.tcp_session_poll(t);
            wall_pause_us(100);
            let tcp = self.sockets.get_mut::<TcpSocket>(handle);
            match tcp.state() {
                TcpState::Established => {
                    if offset >= data.len() {
                        return Ok(offset);
                    }
                    if tcp.can_send() {
                        match tcp.send_slice(&data[offset..]) {
                            Ok(n) => {
                                offset = offset.saturating_add(n);
                                if offset >= data.len() {
                                    return Ok(offset);
                                }
                            }
                            Err(_) => return Err(()),
                        }
                    }
                }
                TcpState::Closed | TcpState::Closing | TcpState::TimeWait => {
                    return if offset > 0 { Ok(offset) } else { Err(()) };
                }
                _ => {}
            }
        }
        if offset > 0 {
            Ok(offset)
        } else {
            Err(())
        }
    }

    /// Blocking recv: waits for data or peer close. Returns 0 on clean EOF.
    pub fn tcp_session_recv(
        &mut self,
        handle: SocketHandle,
        buf: &mut [u8],
        now: u64,
    ) -> Result<usize, ()> {
        let mut t = now;
        for _ in 0..8_000 {
            t = t.wrapping_add(5);
            self.tcp_session_poll(t);
            wall_pause_us(100);
            let tcp = self.sockets.get_mut::<TcpSocket>(handle);
            if tcp.can_recv() {
                return tcp
                    .recv(|data| {
                        let n = core::cmp::min(buf.len(), data.len());
                        buf[..n].copy_from_slice(&data[..n]);
                        (n, n)
                    })
                    .map_err(|_| ());
            }
            match tcp.state() {
                TcpState::CloseWait => {
                    if tcp.can_recv() {
                        continue;
                    }
                    return Ok(0);
                }
                TcpState::Closed | TcpState::Closing | TcpState::TimeWait => return Ok(0),
                _ => {}
            }
        }
        Err(())
    }

    pub fn tcp_session_close(&mut self, handle: SocketHandle, now: u64) {
        {
            let tcp = self.sockets.get_mut::<TcpSocket>(handle);
            tcp.close();
        }
        let mut t = now;
        for _ in 0..32 {
            t = t.wrapping_add(5);
            self.tcp_session_poll(t);
        }
        self.sockets.remove(handle);
    }
}
