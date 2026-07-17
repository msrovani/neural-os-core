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

    crate::serial_println!("[DNS-MANUAL] Resolvendo {} -> {}.{}.{}.{} ({} bytes)",
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
                    crate::serial_println!("[DNS-MANUAL] OK: {}.{}.{}.{}",
                        ip[0], ip[1], ip[2], ip[3]);
                    return Some(ip);
                }
                pos = rdata_end;
            }
        }
        if i % 50 == 0 {
            unsafe { slip::send(&frame); }
        }
    }
    crate::serial_println!("[DNS-MANUAL] Timeout");
    None
}
use smoltcp::iface::{Config, Interface, SocketSet, SocketHandle};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};

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
    // VirtIO-Net (mais rapido em QEMU)
    if let Some(ref mut nic) = *VIRTIO_DEV.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::net::E1000.lock() {
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
    // VirtIO-Net first (mais rapido em QEMU)
    if let Some(ref mut nic) = *VIRTIO_DEV.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::net::E1000.lock() {
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
        caps.max_transmission_unit = 1500;
        caps.medium = Medium::Ethernet;
        caps
    }

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let data = unsafe { nic_recv() };
        if let Some(ref d) = data {
            unsafe { crate::serial_println!("[NET-RX] {} bytes", d.len()); }
        }
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

    /// Configura IP estatico para QEMU user-mode (10.0.2.15/24)
    pub fn set_static_ip(&mut self) {
        let ip = Ipv4Address::new(10, 0, 2, 15);
        let cidr = IpCidr::new(IpAddress::Ipv4(ip), 24);
        self.iface.update_ip_addrs(|addrs| { addrs.push(cidr).ok(); });
        let gw = Ipv4Address::new(10, 0, 2, 2);
        self.iface.routes_mut().add_default_ipv4_route(gw.into()).ok();
        self.dhcp_done = true;
        self.has_static_ip = true;
        // Espelha em NET_CONFIG (diag / agentes / is_online)
        {
            let mut cfg = crate::net::NET_CONFIG.lock();
            cfg.ip = [10, 0, 2, 15];
            cfg.gateway_ip = [10, 0, 2, 2];
            cfg.subnet_mask = [255, 255, 255, 0];
            cfg.dns_ip = [10, 0, 2, 3];
            cfg.configured = true;
            cfg.online = true;
        }
        crate::serial_println!("[NET] Static IP: 10.0.2.15/24 gw=10.0.2.2 dns=10.0.2.3");
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
        let tcp_rx = tcp::SocketBuffer::new(vec![0u8; 4096]);
        let tcp_tx = tcp::SocketBuffer::new(vec![0u8; 4096]);
        let tcp = TcpSocket::new(tcp_rx, tcp_tx);
        let handle = self.sockets.add(tcp);

        let remote = (IpAddress::v4(host[0], host[1], host[2], host[3]), port);
        let tcp = self.sockets.get_mut::<TcpSocket>(handle);
        let context = self.iface.context();
        let _ = tcp.connect(context, remote, 54321);

        let request = alloc::format!(
            "GET {} HTTP/1.1\r\nHost: {}.{}.{}.{}\r\nConnection: close\r\n\r\n",
            path, host[0], host[1], host[2], host[3]
        );

        HttpConn {
            handle,
            state: HttpState::Connecting,
            request,
            started: false,
            buf: Vec::new(),
            timeout: 0,
        }
    }

    pub fn http_poll(&mut self, conn: &mut HttpConn, now: u64) {
        let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
        iface.poll(Instant::from_millis(now as i64), phy, sockets);

        conn.timeout = conn.timeout.wrapping_add(1);
        if conn.timeout > 200 {
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
                        }
                        Err(_) => conn.state = HttpState::Failed,
                    }
                } else {
                    conn.state = HttpState::Waiting;
                }
            }
            TcpState::CloseWait => {
                if tcp.can_recv() {
                    let result = tcp.recv(|data| {
                        let v = Vec::from(&*data);
                        (data.len(), v)
                    });
                    if let Ok(data) = result {
                        conn.buf.extend_from_slice(&data);
                    }
                }
                tcp.close();
                let data = core::mem::take(&mut conn.buf);
                conn.state = HttpState::Done(data);
            }
            TcpState::Closed | TcpState::Closing => {
                let data = core::mem::take(&mut conn.buf);
                if !data.is_empty() {
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

    pub fn dns_resolve(&mut self, hostname: &str, dns_server: [u8; 4]) -> Option<[u8; 4]> {
        let txid: u16 = 0x1234;
        let qname = Self::encode_dns_name(hostname);
        let tx0 = net_tx_count();
        let rx0 = net_rx_count();

        let mut query = Vec::with_capacity(12 + qname.len() + 4);
        query.extend_from_slice(&txid.to_be_bytes());
        query.extend_from_slice(&[0x01, 0x00]);
        query.extend_from_slice(&[0x00, 0x01]);
        query.extend_from_slice(&[0x00, 0x00]);
        query.extend_from_slice(&[0x00, 0x00]);
        query.extend_from_slice(&[0x00, 0x00]);
        query.extend_from_slice(&qname);
        query.extend_from_slice(&[0x00, 0x01]);
        query.extend_from_slice(&[0x00, 0x01]);

        let dns_server_addr = (IpAddress::v4(dns_server[0], dns_server[1], dns_server[2], dns_server[3]), 53u16);

        let meta = vec![smoltcp::storage::PacketMetadata::<smoltcp::socket::udp::UdpMetadata>::EMPTY; 1];
        let payload = vec![0u8; 512];
        let buf_rx = udp_socket::PacketBuffer::new(meta, payload);
        let meta2 = vec![smoltcp::storage::PacketMetadata::<smoltcp::socket::udp::UdpMetadata>::EMPTY; 1];
        let payload2 = vec![0u8; 512];
        let buf_tx = udp_socket::PacketBuffer::new(meta2, payload2);
        let socket = udp_socket::Socket::new(buf_rx, buf_tx);
        let handle = self.sockets.add(socket);

        {
            let udp = self.sockets.get_mut::<udp_socket::Socket>(handle);
            let _ = udp.bind(54321);
            if udp.send_slice(&query, dns_server_addr).is_err() {
                crate::serial_println!("[DNS] send_slice falhou");
            }
        }

        // ~800 × 500µs ≈ 400ms wall — tempo para ARP gw 10.0.2.2 + DNS 10.0.2.3 no slirp.
        // Instant avança 5ms/iter (lógico); wall_pause dá RX real no e1000.
        for i in 0..800u64 {
            self.poll((i * 5) as i64);
            wall_pause_us(500);

            let payload = {
                let udp = self.sockets.get_mut::<udp_socket::Socket>(handle);
                udp.recv().ok().map(|(data, _)| Vec::from(data))
            };

            if let Some(ref data) = payload {
                if data.len() < 12 { break; }
                let resp_txid = u16::from_be_bytes([data[0], data[1]]);
                if resp_txid != txid { continue; }
                let flags = u16::from_be_bytes([data[2], data[3]]);
                if flags & 0x8000 == 0 { continue; }
                let ancount = u16::from_be_bytes([data[6], data[7]]);
                if ancount == 0 { break; }

                let (mut pos, _) = Self::parse_dns_name(data, 12);
                pos += 4;

                for _ in 0..ancount {
                    let (new_pos, name_end) = Self::parse_dns_name(data, pos);
                    pos = new_pos;
                    let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
                    let rclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
                    let _ttl = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
                    let rdlen = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
                    pos += 10;

                    if rtype == 1 && rclass == 1 && rdlen == 4 {
                        let ip = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
                        self.sockets.remove(handle);
                        crate::serial_println!(
                            "[DNS] OK {}.{}.{}.{} (dtx={} drx={})",
                            ip[0], ip[1], ip[2], ip[3],
                            net_tx_count().saturating_sub(tx0),
                            net_rx_count().saturating_sub(rx0)
                        );
                        return Some(ip);
                    }
                    pos = name_end.max(pos + rdlen);
                }
                break;
            }
        }
        self.sockets.remove(handle);
        crate::serial_println!(
            "[DNS] timeout dtx={} drx={} tx={} rx={}",
            net_tx_count().saturating_sub(tx0),
            net_rx_count().saturating_sub(rx0),
            net_tx_count(),
            net_rx_count()
        );
        None
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

    fn parse_dns_name(pkt: &[u8], offset: usize) -> (usize, usize) {
        let mut pos = offset;
        let mut jumped = false;
        let mut end = 0;
        while pos < pkt.len() {
            let b = pkt[pos];
            if b & 0xC0 == 0xC0 {
                if !jumped { end = pos + 2; }
                pos = ((b as usize & 0x3F) << 8) | (pkt[pos + 1] as usize);
                jumped = true;
            } else if b == 0 {
                pos += 1;
                return (pos, if jumped { end } else { pos });
            } else {
                pos += 1 + b as usize;
            }
        }
        (pos, pos)
    }
}
