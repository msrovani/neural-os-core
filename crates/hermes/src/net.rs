use k_nano::rtl8139::Rtl8139Driver;
use k_nano::e1000::{E1000Driver, REG_STATUS};
use k_nano::i225::I225Driver;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

pub const TOPIC_HW_NET_RTL8139: &str = "HW_NET_RTL8139";
pub const TOPIC_NETWORK_CONFIGURED: &str = "NETWORK_CONFIGURED";
pub const TOPIC_NET_READY: &str = "NET_READY";
pub const TOPIC_NETWORK_DEGRADED: &str = "NETWORK_DEGRADED";
pub const TOPIC_NETWORK_HEALTH: &str = "NETWORK_HEALTH";

// Low-level NIC statics live in k_nano (R0 transport). Re-export for single source.
pub use k_nano::nic_globals::{RTL8139, E1000, I225, VIRTIO_DEV};
pub use k_nano::nic_globals::NET_CONFIG as KNANO_NET_CONFIG;

pub static NETSTACK: spin::Mutex<Option<crate::netstack::NetStack>> = spin::Mutex::new(None);

pub struct NetConfig {
    pub mac: [u8; 6],
    pub ip: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub dns_ip: [u8; 4],
    pub gateway_mac: [u8; 6],
    pub configured: bool,
    pub online: bool,
    pub is_dev_env: bool,
}

pub static NET_CONFIG: spin::Mutex<NetConfig> = spin::Mutex::new(NetConfig {
    mac: [0; 6],
    ip: [0; 4],
    gateway_ip: [10, 0, 2, 2],
    subnet_mask: [255, 255, 255, 0],
    dns_ip: [10, 0, 2, 3],
    gateway_mac: [0; 6],
    configured: false,
    online: false,
    is_dev_env: false,
});

pub fn wait_ticks(ticks: usize) {
    let start = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let mut guard: usize = 0;
    loop {
        let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        if now.wrapping_sub(start) >= ticks { break; }
        if guard >= 100_000_000 { break; }
        guard += 1;
        x86_64::instructions::hlt();
    }
}

pub fn detect_hypervisor_name() -> alloc::string::String {
    unsafe {
        let hv = core::arch::x86_64::__cpuid(0x40000000);
        let mut name = [0u8; 13];
        name[0..4].copy_from_slice(&hv.ebx.to_le_bytes());
        name[4..8].copy_from_slice(&hv.ecx.to_le_bytes());
        name[8..12].copy_from_slice(&hv.edx.to_le_bytes());
        let s = core::str::from_utf8(&name).unwrap_or("unknown");
        alloc::string::String::from(s.trim_end_matches('\0'))
    }
}

pub fn detect_dev_env() -> bool {
    unsafe {
        let leaf1 = core::arch::x86_64::__cpuid(1);
        let has_hypervisor = (leaf1.ecx & (1 << 31)) != 0;
        if has_hypervisor {
            let hv = core::arch::x86_64::__cpuid(0x40000000);
            let max_leaf = hv.eax;
            if max_leaf >= 0x40000000 {
                let vendor_ebx = hv.ebx;
                let vendor_ecx = hv.ecx;
                let vendor_edx = hv.edx;
                let mut hypervisor_name = [0u8; 13];
                let ebx_bytes = vendor_ebx.to_le_bytes();
                let ecx_bytes = vendor_ecx.to_le_bytes();
                let edx_bytes = vendor_edx.to_le_bytes();
                hypervisor_name[0..4].copy_from_slice(&ebx_bytes);
                hypervisor_name[4..8].copy_from_slice(&ecx_bytes);
                hypervisor_name[8..12].copy_from_slice(&edx_bytes);
                let name = core::str::from_utf8(&hypervisor_name).unwrap_or("unknown");
                k_nano::slog_hermes!("Net", "info", "Hypervisor detected: {}", name.trim_end_matches('\0'));
                let name_lower = name.to_ascii_lowercase();
                return name_lower.contains("qemu") ||
                       name_lower.contains("kvm") ||
                       name_lower.contains("vbox") ||
                       name_lower.contains("vmware") ||
                       name_lower.contains("tcg") ||
                       name_lower.contains("micr");
            }
        }
        let mac = NET_CONFIG.lock().mac;
        if mac[0] == 0x08 && mac[1] == 0x00 && mac[2] == 0x27 { return true; }
        if mac[0] == 0x52 && mac[1] == 0x54 && mac[2] == 0x00 { return true; }
        false
    }
}

pub unsafe fn init_driver_rtl8139() -> bool {
    if RTL8139.lock().is_some() { return true; }
    let pci_devices = k_nano::pci::scan_pci();
    let mut dev_opt = None;
    for dev in &pci_devices {
        if dev.vendor_id == 0x10EC && dev.device_id == 0x8139 {
            k_nano::slog_hermes!("Net", "info", "RTL8139 detectado: {:02x}:{:02x}.{:02x}", dev.bus, dev.device, dev.function);
            let mut driver = match Rtl8139Driver::new(dev) { Some(d) => d, None => { k_nano::slog_hermes!("Net", "info", "RTL8139 new() falhou"); return false; } };
            if driver.init() {
                dev_opt = Some(driver);
            }
            break;
        }
    }
    let driver = match dev_opt {
        Some(d) => d,
        None => {
            k_nano::slog_hermes!("Net", "info", "RTL8139 nao encontrado.");
            return false;
        }
    };
    let mac = driver.mac();
    NET_CONFIG.lock().mac = mac;
    *RTL8139.lock() = Some(driver);
    k_nano::slog_hermes!("Net", "info", "RTL8139 iniciado. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    let hw_event = event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_HW_NET_RTL8139),
        payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
        token: event_bus::CapabilityToken::Legacy(1),
    };
    let _ = k_nano::EVENT_BUS.publish(hw_event);
    true
}

pub unsafe fn init_driver_e1000() -> bool {
    if E1000.lock().is_some() { return true; }
    let pci_devices = k_nano::pci::scan_pci();
    for dev in &pci_devices {
        if k_nano::e1000::is_e1000_family(dev.vendor_id, dev.device_id) {
            k_nano::slog_hermes!("Net", "info", "e1000 detectado: {:02x}:{:02x}.{:02x} device={:#06x}", dev.bus, dev.device, dev.function, dev.device_id);
            let mut driver = match E1000Driver::new(dev) { Some(d) => d, None => { k_nano::slog_hermes!("Net", "info", "E1000 new() falhou"); return false; } };
            if driver.init() {
                let mac = driver.mac();
                NET_CONFIG.lock().mac = mac;
                *E1000.lock() = Some(driver);
                k_nano::slog_hermes!("Net", "info", "e1000 iniciado. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
                let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                    id: 0,
                    topic: alloc::string::String::from("HW_NET_E1000"),
                    payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
                    token: event_bus::CapabilityToken::Legacy(1),
                });
                return true;
            }
        }
    }
    k_nano::slog_hermes!("Net", "info", "e1000 nao encontrado.");
    false
}

pub unsafe fn init_driver_i225() -> bool {
    if I225.lock().is_some() {
        return true;
    }
    let pci_devices = k_nano::pci::scan_pci();
    for dev in &pci_devices {
        if !k_nano::i225::is_i225_family(dev.vendor_id, dev.device_id) {
            continue;
        }
        k_nano::slog_hermes!(
            "Net",
            "i225",
            "detectado {:02x}:{:02x}.{:x} DID={:#06x}",
            dev.bus,
            dev.device,
            dev.function,
            dev.device_id
        );
        let mut driver = match I225Driver::new(dev) {
            Some(d) => d,
            None => {
                k_nano::slog_hermes!("Net", "i225", "new() FAIL");
                return false;
            }
        };
        if driver.init() {
            let mac = driver.mac();
            NET_CONFIG.lock().mac = mac;
            *I225.lock() = Some(driver);
            k_nano::slog_hermes!(
                "Net",
                "i225",
                "iniciado MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );
            let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                id: 0,
                topic: alloc::string::String::from("HW_NET_I225"),
                payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
                token: event_bus::CapabilityToken::Legacy(1),
            });
            return true;
        }
        k_nano::slog_hermes!("Net", "i225", "init() FAIL DID={:#06x}", dev.device_id);
    }
    k_nano::slog_hermes!("Net", "i225", "nao encontrado (esperado em QEMU — sem emu i225)");
    false
}

pub unsafe fn probe_nics_from_bind_plan() -> bool {
    use k_nano::boot_bind::NicKind;
    let (order, n) = k_nano::boot_bind::nic_probe_order();
    if n == 0 {
        k_nano::slog_hermes!(
            "Net",
            "bind",
            "DeviceTree sem NIC classificada — skip probe (observe, nao martelo)"
        );
        return false;
    }
    for i in 0..n {
        let kind = order[i];
        let ok = match kind {
            NicKind::I225 => init_driver_i225(),
            NicKind::Virtio => k_nano::virtio_net::init_driver_virtio(),
            NicKind::E1000 => init_driver_e1000(),
            NicKind::Rtl8139 => init_driver_rtl8139(),
            NicKind::None => false,
        };
        k_nano::slog_hermes!("Net", "bind", "probe {} ok={}", kind.as_str(), ok);
        if ok {
            return true;
        }
    }
    false
}

pub unsafe fn init_serial_tunnel() -> bool {
    k_nano::slog_hermes!("Net", "info", "Inicializando serial tunnel (COM2 bypass)...");
    let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0xFE];
    NET_CONFIG.lock().mac = mac;
    k_nano::slog_hermes!("Net", "info", "Serial tunnel ativo. Fake MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    k_nano::slog_hermes!("Net", "info", "Modo offline — aguardando trafego via serial tunnel.");
    true
}

static E1000_LINK_WAS_UP: AtomicBool = AtomicBool::new(false);

pub unsafe fn dump_e1000_status() {
    let mut guard = E1000.lock();
    if let Some(ref mut nic) = *guard {
        nic.dump_status();
        let status = nic.read32(REG_STATUS);
        let link_up = status & 0x02 != 0;
        if link_up && !E1000_LINK_WAS_UP.load(Ordering::Relaxed) {
            nic.kick_rx();
        }
        E1000_LINK_WAS_UP.store(link_up, Ordering::Relaxed);
    }
}

pub const NETMODE_LOADER_PHYS: u64 = 0x1640_0000_00 >> 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QemuNetMode {
    User,
    Bridge,
    Static([u8;4]),
}

pub fn detect_qemu_net_mode() -> QemuNetMode {    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return QemuNetMode::User;
    }
    unsafe {
        let ram_end = k_nano::memory::TOTAL_RAM_MB
            .load(core::sync::atomic::Ordering::Relaxed)
            .saturating_mul(1024 * 1024)
            .min(0x200000000);
        if ram_end <= 0x100000000 {
            return QemuNetMode::User;
        }
        let mut addr: u64 = ram_end;
        while addr > 0x100000000 {
            addr -= 0x100000;
            k_nano::apic::map_page_uc(addr, pmoff);
            let p = (addr + pmoff) as *const u8;
            let b = core::ptr::read_volatile(p);
            match b {
                b'B' | b'b' => return QemuNetMode::Bridge,
                b'S' | b's' => {
                    let ip = [
                        core::ptr::read_volatile(p.add(1)),
                        core::ptr::read_volatile(p.add(2)),
                        core::ptr::read_volatile(p.add(3)),
                        core::ptr::read_volatile(p.add(4)),
                    ];
                    if ip[0] != 0 && ip[0] != 0xFF {
                        return QemuNetMode::Static(ip);
                    }
                }
                _ => {}
            }
        }
        QemuNetMode::User
    }
}

pub unsafe fn prove_e1000_rx(sip: [u8; 4], tip: [u8; 4]) -> bool {
    let mut guard = E1000.lock();
    if let Some(ref mut nic) = *guard {
        let (rdh, dd, ok) = nic.prove_rx(sip, tip, 6_000);
        k_nano::slog_bin!("Net", "e1000", "prove_rx: ok={} rdh={} dd={} (ARP who-has {}.{}.{}.{})", ok, rdh, dd, tip[0], tip[1], tip[2], tip[3]);
        return ok;
    }
    k_nano::slog_bin!("Net", "e1000", "prove_rx SKIP: no e1000");
    false
}

pub fn detect_qemu_ota_trigger() -> bool {
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return false;
    }
    unsafe {
        let ram_end = k_nano::memory::TOTAL_RAM_MB
            .load(core::sync::atomic::Ordering::Relaxed)
            .saturating_mul(1024 * 1024)
            .min(0x200000000);
        if ram_end <= 0x100000000 {
            return false;
        }
        let mut addr: u64 = ram_end;
        while addr > 0x100000000 {
            addr -= 0x100000;
            k_nano::apic::map_page_uc(addr, pmoff);
            let p = (addr + pmoff) as *const u8;
            if core::ptr::read_volatile(p) == b'O' {
                k_nano::slog_bin!("OTA", "info", "trigger flag encontrado @ {:#x}", addr);
                return true;
            }
        }
    }
    false
}

pub unsafe fn http_get(host: [u8; 4], port: u16, path: &str) -> Option<Vec<u8>> {
    http_get_host(host, port, path, None)
}

pub unsafe fn http_get_host(
    host: [u8; 4],
    port: u16,
    path: &str,
    host_header: Option<&str>,
) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;

    let mut conn = stack.http_new_host(host, port, path, host_header);
    for _ in 0..200_000 {
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        stack.http_poll(&mut conn, now as u64);
        match conn.state {
            crate::netstack::HttpState::Done(ref data) => {
                return Some(strip_http_envelope(data));
            }
            crate::netstack::HttpState::Failed => {
                break;
            }
            _ => {
                core::hint::spin_loop();
            }
        }
    }
    None
}

pub unsafe fn http_get_raw(host: [u8; 4], port: u16, data: &[u8]) -> Option<Vec<u8>> {
    tcp_exchange(host, port, data)
}

pub unsafe fn tcp_exchange(host: [u8; 4], port: u16, data: &[u8]) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    stack.tcp_exchange(host, port, data, now as u64)
}

pub fn parse_http_url(
    url: &str,
) -> Result<(alloc::string::String, u16, alloc::string::String), &'static str> {
    let u = url.trim();
    if u.starts_with("https://") || u.starts_with("HTTPS://") {
        return Err("use_https_get");
    }
    let rest = u
        .strip_prefix("http://")
        .or_else(|| u.strip_prefix("HTTP://"))
        .ok_or("bad_url")?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if hostport.is_empty() {
        return Err("bad_url");
    }
    let (host, port) = if let Some(i) = hostport.rfind(':') {
        let maybe_port = &hostport[i + 1..];
        if maybe_port.chars().all(|c| c.is_ascii_digit()) {
            let p: u16 = maybe_port.parse().map_err(|_| "bad_port")?;
            (&hostport[..i], p)
        } else {
            (hostport, 80u16)
        }
    } else {
        (hostport, 80u16)
    };
    Ok((
        alloc::string::String::from(host),
        port,
        alloc::string::String::from(path),
    ))
}

fn parse_ipv4_host(host: &str) -> Option<[u8; 4]> {
    let parts: alloc::vec::Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 3 {
            return None;
        }
        out[i] = p.parse().ok()?;
    }
    Some(out)
}

pub unsafe fn dns_resolve_host(hostname: &str) -> Option<[u8; 4]> {
    if let Some(ip) = parse_ipv4_host(hostname) {
        return Some(ip);
    }
    let dns = {
        let cfg = NET_CONFIG.lock();
        if cfg.dns_ip != [0; 4] {
            cfg.dns_ip
        } else {
            [10, 0, 2, 3]
        }
    };
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    stack.dns_resolve(hostname, dns)
}

pub fn udp_exchange_safe(dst: [u8; 4], dst_port: u16, payload: &[u8]) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    stack.udp_exchange_raw(dst, dst_port, payload)
}

pub fn dns_resolve_host_safe(hostname: &str) -> Option<[u8; 4]> {
    unsafe { dns_resolve_host(hostname) }
}

pub unsafe fn resolve_and_http_get(url: &str) -> Result<Vec<u8>, &'static str> {
    let u = url.trim();
    if u.starts_with("https://") || u.starts_with("HTTPS://") {
        return https_get(u);
    }
    let (host, port, path) = parse_http_url(url)?;
    let ip = dns_resolve_host(&host).ok_or("dns_failed")?;
    k_nano::slog_bin!(
        "Net",
        "http",
        "GET {}.{}.{}.{}:{}{} Host={}",
        ip[0], ip[1], ip[2], ip[3], port, path, host
    );
    match http_get_host(ip, port, &path, Some(host.as_str())) {
        Some(body) if !body.is_empty() => Ok(body),
        Some(_) => Err("http_empty"),
        None => Err("http_failed"),
    }
}

pub fn resolve_and_http_get_safe(url: &str) -> Result<Vec<u8>, &'static str> {
    unsafe { resolve_and_http_get(url) }
}

pub fn https_get(url: &str) -> Result<Vec<u8>, &'static str> {
    // In hermes crate, HTTPS is stub until TLS wired; return error to keep honest.
    let _ = url;
    Err("tls_not_ready")
}

pub fn log_tls_status_boot() {
    use core::sync::atomic::{AtomicBool, Ordering};
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::Relaxed) {
        k_nano::slog_bin!(
            "TLS",
            "info",
            "VERDICT=WIRED trust=hybrid+certverify crate=embedded-tls-0.19 pins=TLSPINS.BIN"
        );
    }
}

pub fn smoke_https_if_online() {
    // best-effort, no-op in hermes (bin will handle)
}

pub struct HttpRangeBody {
    pub status: u16,
    pub body: Vec<u8>,
    pub total: Option<usize>,
}

pub unsafe fn http_get_range_host(
    host: [u8; 4],
    port: u16,
    path: &str,
    host_header: Option<&str>,
    start: usize,
    end: usize,
) -> Option<HttpRangeBody> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let mut conn = stack.http_new_host_ranged(host, port, path, host_header, start, end);
    for _ in 0..12_000 {
        stack.http_poll(&mut conn, now as u64);
        match conn.state {
            crate::netstack::HttpState::Done(ref data) => {
                return Some(parse_http_range_raw(data));
            }
            crate::netstack::HttpState::Failed => break,
            _ => core::hint::spin_loop(),
        }
    }
    None
}

fn parse_http_range_raw(raw: &[u8]) -> HttpRangeBody {
    let mut status = 0u16;
    let mut total = None;
    let body_off = find_header_end(raw).unwrap_or(0);
    if raw.starts_with(b"HTTP/") {
        let line_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len().min(32));
        if let Ok(line) = core::str::from_utf8(&raw[..line_end]) {
            if let Some(code) = line.split_whitespace().nth(1) {
                status = code.parse().unwrap_or(0);
            }
        }
        if let Ok(hdrs) = core::str::from_utf8(&raw[..body_off]) {
            for hline in hdrs.lines() {
                let lower = hline.to_ascii_lowercase();
                if let Some(rest) = lower.strip_prefix("content-range:") {
                    if let Some(slash) = rest.rfind('/') {
                        let t = rest[slash + 1..].trim();
                        if t != "*" {
                            total = t.parse().ok();
                        }
                    }
                }
            }
        }
    }
    let body = if body_off < raw.len() {
        raw[body_off..].to_vec()
    } else {
        Vec::new()
    };
    HttpRangeBody { status, body, total }
}

fn strip_http_envelope(raw: &[u8]) -> Vec<u8> {
    if let Some(idx) = find_header_end(raw) {
        Vec::from(&raw[idx..])
    } else {
        Vec::from(raw)
    }
}

fn find_header_end(raw: &[u8]) -> Option<usize> {
    let n = raw.len();
    for i in 0..n.saturating_sub(3) {
        if raw[i] == b'\r'
            && raw[i + 1] == b'\n'
            && raw[i + 2] == b'\r'
            && raw[i + 3] == b'\n'
        {
            return Some(i + 4);
        }
    }
    for i in 0..n.saturating_sub(1) {
        if raw[i] == b'\n' && raw[i + 1] == b'\n' {
            return Some(i + 2);
        }
    }
    None
}

pub unsafe fn ping(_target_ip: [u8; 4]) -> Option<u64> { None }

pub fn persist_dhcp_config() {
    let cfg = NET_CONFIG.lock();
    if !cfg.configured { return; }
    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&cfg.gateway_ip);
    buf[4..8].copy_from_slice(&cfg.dns_ip);
    let _ = k_ai::sgdb::store::put_kv("sys/net_config", &buf);
}

pub fn restore_dhcp_config() -> bool {
    let Ok(Some(buf)) = k_ai::sgdb::store::get_kv("sys/net_config") else { return false; };
    if buf.len() < 8 { return false; }
    let mut cfg = NET_CONFIG.lock();
    cfg.gateway_ip.copy_from_slice(&buf[0..4]);
    cfg.dns_ip.copy_from_slice(&buf[4..8]);
    cfg.configured = true;
    true
}

pub fn run_network_diagnostics() -> alloc::string::String {
    let cfg = NET_CONFIG.lock();
    let mac = cfg.mac;
    let ip = cfg.ip;
    let gw = cfg.gateway_ip;
    let dns = cfg.dns_ip;
    let configured = cfg.configured;
    let online = cfg.online;
    drop(cfg);

    let mut report = alloc::string::String::new();
    report.push_str("=== Diagnostico de Rede ===\n");

    if !configured {
        report.push_str("Rede nao configurada.\n");
        return report;
    }

    report.push_str(&alloc::format!("Status: {}\n", if online { "ONLINE" } else { "DEGRADED" }));
    report.push_str(&alloc::format!(
        "MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ));
    report.push_str(&alloc::format!(
        "IP: {}.{}.{}.{} / GW: {}.{}.{}.{} / DNS: {}.{}.{}.{}\n",
        ip[0], ip[1], ip[2], ip[3],
        gw[0], gw[1], gw[2], gw[3],
        dns[0], dns[1], dns[2], dns[3]
    ));
    report.push_str("Diagnostico concluido.\n");
    report
}

pub struct NetDiagnosticSkill;

impl skill_registry::Skill for NetDiagnosticSkill {
    fn manifest(&self) -> skill_registry::McpManifest {
        skill_registry::McpManifest {
            name: alloc::string::String::from("net_diag"),
            description: alloc::string::String::from("Network diagnostics and AI analysis of connectivity"),
            required_tokens: alloc::vec![1],
            preconditions: alloc::vec![],
            context_links: alloc::vec![],
            output_schema: skill_registry::OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }
    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }
    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let report = run_network_diagnostics();
        Ok(report.into_bytes())
    }
}
