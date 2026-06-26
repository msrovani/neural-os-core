use alloc::vec;
use alloc::vec::Vec;
use smoltcp::iface::{Config, Interface, SocketSet, SocketHandle};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr};
use smoltcp::socket::tcp::{self, State as TcpState, Socket as TcpSocket};

use crate::net::RTL8139;

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
        unsafe { send_raw(buf) };
        r
    }
}

unsafe fn send_raw(data: Vec<u8>) {
    if let Some(ref mut driver) = *RTL8139.lock() {
        driver.send(&data);
    }
}

unsafe fn recv_raw() -> Option<Vec<u8>> {
    if let Some(ref mut driver) = *RTL8139.lock() {
        driver.recv()
    } else {
        None
    }
}

pub struct Rtl8139Phy;

impl Device for Rtl8139Phy {
    type RxToken<'x> = PhyToken;
    type TxToken<'x> = PhyToken;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1500;
        caps.medium = Medium::Ethernet;
        caps
    }

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let data = unsafe { recv_raw() }?;
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
    phy: Rtl8139Phy,
}

impl NetStack {
    pub fn new(mac: [u8; 6]) -> Self {
        let eth = EthernetAddress::from_bytes(&mac);
        let config = Config::new(HardwareAddress::Ethernet(eth));
        let now = Instant::from_millis(0);
        let mut phy = Rtl8139Phy;
        let iface = Interface::new(config, &mut phy, now);
        let sockets = SocketSet::new(vec![]);
        NetStack { iface, sockets, phy }
    }

    pub fn poll(&mut self, tick: i64) {
        let Self { ref mut iface, ref mut phy, ref mut sockets } = self;
        iface.poll(Instant::from_millis(tick), phy, sockets);
    }

    pub fn set_ip(&mut self, ip: [u8; 4]) {
        let addr = IpAddress::v4(ip[0], ip[1], ip[2], ip[3]);
        let cidr = IpCidr::new(addr, 24);
        self.iface.update_ip_addrs(|addrs| {
            addrs.push(cidr).unwrap();
        });
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
        let Self { ref mut iface, ref mut phy, ref mut sockets } = self;
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
        let Self { ref mut iface, ref mut phy, ref mut sockets } = self;
        let tcp = sockets.get_mut::<TcpSocket>(conn.handle);
        tcp.close();
        iface.poll(Instant::from_millis(0), phy, sockets);
        sockets.remove(conn.handle);
    }
}
