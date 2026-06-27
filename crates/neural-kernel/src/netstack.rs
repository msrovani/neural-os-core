use alloc::vec;
use alloc::vec::Vec;
use smoltcp::iface::{Config, Interface, SocketSet, SocketHandle};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, Ipv4Address, IpCidr};
use smoltcp::socket::tcp::{self, State as TcpState, Socket as TcpSocket};
use smoltcp::socket::udp as udp_socket;
use smoltcp::socket::dhcpv4::{self, Event as DhcpEvent, Socket as DhcpSocket};

use crate::net::{RTL8139, VIRTIO_DEV};

pub struct PhyToken(pub Vec<u8>);

impl RxToken for PhyToken {
    fn consume<R, F>(self, f: F) -> R
    where F: FnOnce(&[u8]) -> R {
        f(&self.0)
    }
}

impl TxToken for PhyToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where F: FnOnce(&mut [u8]) -> R {
        let mut buf = vec![0u8; len];
        let r = f(&mut buf);
        unsafe { nic_send(buf) };
        r
    }
}

unsafe fn nic_send(data: Vec<u8>) {
    if let Some(ref mut nic) = *RTL8139.lock() {
        nic.send(&data);
    } else if let Some(ref mut nic) = *VIRTIO_DEV.lock() {
        nic.send(&data);
    }
}

unsafe fn nic_recv() -> Option<Vec<u8>> {
    if let Some(ref mut nic) = *RTL8139.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *VIRTIO_DEV.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
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
        let data = unsafe { nic_recv() }?;
        Some((PhyToken(data), PhyToken(vec![])))
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

        NetStack { iface, sockets, phy, dhcp_handle, dhcp_done: false }
    }

    pub fn poll(&mut self, tick: i64) {
        let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
        iface.poll(Instant::from_millis(tick), phy, sockets);
    }

    /// Poll DHCP — must be called each tick until dhcp_done = true
    /// Returns (gateway_ip, dns_ip) when configured
    pub fn dhcp_poll(&mut self, tick: i64) -> (bool, [u8; 4], [u8; 4]) {
        let Self { ref mut iface, ref mut phy, ref mut sockets, ref mut dhcp_done, .. } = self;
        iface.poll(Instant::from_millis(tick), phy, sockets);

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
            let _ = udp.send_slice(&query, dns_server_addr);
        }

        for _ in 0..100 {
            let Self { ref mut iface, ref mut phy, ref mut sockets, .. } = self;
            iface.poll(Instant::from_millis(0), phy, sockets);

            let payload = {
                let udp = sockets.get_mut::<udp_socket::Socket>(handle);
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
                        return Some(ip);
                    }
                    pos = name_end.max(pos + rdlen);
                }
                break;
            }
        }
        self.sockets.remove(handle);
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
