use crate::e1000::E1000Driver;
use crate::{println, serial_println};
use alloc::vec::Vec;

pub static E1000: spin::Mutex<Option<E1000Driver>> = spin::Mutex::new(None);

pub struct NetConfig {
    pub mac: [u8; 6],
    pub ip: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub dns_ip: [u8; 4],
    pub gateway_mac: [u8; 6],
    pub configured: bool,
    pub online: bool,
}

pub static NET_CONFIG: spin::Mutex<NetConfig> = spin::Mutex::new(NetConfig {
    mac: [0; 6],
    ip: [0; 4],
    gateway_ip: [10, 0, 2, 1],
    subnet_mask: [255, 255, 255, 0],
    dns_ip: [10, 0, 2, 3],
    gateway_mac: [0; 6],
    configured: false,
    online: false,
});

pub unsafe fn init_network() -> bool {
    let pci_devices = crate::pci::scan_pci();
    let mut dev_opt = None;
    for dev in &pci_devices {
        if dev.vendor_id == 0x8086 && dev.device_id == 0x100E {
            serial_println!("[NET] e1000 detectado: {:02x}:{:02x}.{:02x}", dev.bus, dev.device, dev.function);
            println!("[NET] e1000 detectado.");
            let mut driver = E1000Driver::new(dev).unwrap();
            if driver.init() {
                dev_opt = Some(driver);
                break;
            }
        }
    }

    let driver = match dev_opt {
        Some(d) => d,
        None => {
            serial_println!("[NET] Nenhum dispositivo de rede encontrado.");
            return false;
        }
    };

    let mac = driver.mac();
    NET_CONFIG.lock().mac = mac;
    *E1000.lock() = Some(driver);

    serial_println!("[NET] Driver e1000 inicializado. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    println!("[NET] Driver de rede pronto.");

    // Pulando DHCP (lento no QEMU TCG). Usando IP estatico.
    serial_println!("[NET] Usando configuracao estatica (10.0.2.15).");
    println!("[NET] Usando configuracao estatica.");
    let mut cfg = NET_CONFIG.lock();
    cfg.ip = [10, 0, 2, 15];
    cfg.configured = true;
    drop(cfg);

    // Gateway MAC para QEMU user-mode: 52:54:00:12:34:56 (padrao slirp)
    NET_CONFIG.lock().gateway_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    serial_println!("[NET] Gateway MAC configurado: 52:54:00:12:34:56");
    true
}

#[allow(dead_code)]
unsafe fn try_dhcp() -> bool {
    for attempt in 0..5 {
        serial_println!("[DHCP] Tentativa {}...", attempt + 1);
        if crate::proto::dhcp_discover(attempt) {
            return true;
        }
        for _ in 0..50000000 { core::hint::spin_loop(); }
    }
    false
}

pub unsafe fn http_get(host: [u8; 4], port: u16, path: &str) -> Option<Vec<u8>> {
    let cfg = NET_CONFIG.lock();
    let our_ip = cfg.ip;
    let our_mac = cfg.mac;
    let gw_mac = cfg.gateway_mac;
    drop(cfg);

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();

    if gw_mac == [0; 6] {
        let our_mac = driver.mac();
        drop(guard);
        let cfg = NET_CONFIG.lock();
        let gw = cfg.gateway_ip;
        drop(cfg);
        crate::proto::arp_request(our_mac, gw);
        for _ in 0..10000000 { core::hint::spin_loop(); }
        let mut guard = E1000.lock();
        let driver = guard.as_mut().unwrap();
        if let Some(pkt) = driver.recv() {
            if let Some(gw_mac) = crate::proto::parse_arp_reply(&pkt) {
                NET_CONFIG.lock().gateway_mac = gw_mac;
            }
        }
        drop(guard);
        let mut guard = E1000.lock();
        let driver = guard.as_mut().unwrap();
        let cfg = NET_CONFIG.lock();
        let gw_mac = cfg.gateway_mac;
        drop(cfg);
        if gw_mac == [0; 6] {
            return None;
        }
        crate::proto::http_get_request(driver, our_mac, gw_mac, our_ip, host, port, path);
        for _ in 0..100000000 { core::hint::spin_loop(); }
        if let Some(pkt) = driver.recv() {
            return crate::proto::parse_http_response(&pkt, our_mac);
        }
    } else {
        crate::proto::http_get_request(driver, our_mac, gw_mac, our_ip, host, port, path);
        for _ in 0..100000000 { core::hint::spin_loop(); }
        if let Some(pkt) = driver.recv() {
            return crate::proto::parse_http_response(&pkt, our_mac);
        }
    }
    None
}

pub unsafe fn ping(target_ip: [u8; 4]) -> Option<u64> {
    let cfg = NET_CONFIG.lock();
    let our_ip = cfg.ip;
    let our_mac = cfg.mac;
    let gw_mac = cfg.gateway_mac;
    drop(cfg);

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();

    if gw_mac == [0; 6] {
        let our_mac2 = driver.mac();
        drop(guard);
        let cfg2 = NET_CONFIG.lock();
        let gw = cfg2.gateway_ip;
        drop(cfg2);
        crate::proto::arp_request(our_mac2, gw);
        for _ in 0..10000000 { core::hint::spin_loop(); }
        let mut guard2 = E1000.lock();
        let driver2 = guard2.as_mut().unwrap();
        if let Some(pkt) = driver2.recv() {
            if let Some(gwm) = crate::proto::parse_arp_reply(&pkt) {
                NET_CONFIG.lock().gateway_mac = gwm;
            }
        }
        drop(guard2);
    }

    let cfg = NET_CONFIG.lock();
    let gw_mac_final = cfg.gateway_mac;
    drop(cfg);
    if gw_mac_final == [0; 6] { return None; }

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();
    let ident: u16 = 0x1234;
    let seq: u16 = 1;
    crate::proto::icmp_echo_request(driver, our_mac, gw_mac_final, our_ip, target_ip, ident, seq);
    drop(guard);

    for _ in 0..30000000 { core::hint::spin_loop(); }

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();
    if let Some(pkt) = driver.recv() {
        if crate::proto::parse_icmp_reply(&pkt, our_mac, target_ip, ident, seq).is_some() {
            return Some(1);
        }
    }
    None
}

pub fn run_network_diagnostics() -> crate::String {
    let cfg = NET_CONFIG.lock();
    let mac = cfg.mac;
    let ip = cfg.ip;
    let gw = cfg.gateway_ip;
    let dns = cfg.dns_ip;
    let configured = cfg.configured;
    drop(cfg);

    let mut report = crate::String::new();
    report.push_str("=== Diagnostico de Rede [IA] ===\n");

    if !configured {
        report.push_str("Rede nao configurada.\n");
        report.push_str("Verifique o adaptador de rede (e1000) no QEMU.\n");
        return report;
    }

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

    report.push_str("\nSistema com capacidade de rede.\n");
    report
}

pub struct NetDiagnosticSkill;

impl crate::Skill for NetDiagnosticSkill {
    fn manifest(&self) -> crate::McpManifest {
        crate::McpManifest {
            name: alloc::string::String::from("net_diag"),
            description: alloc::string::String::from("Network diagnostics and AI analysis of connectivity"),
            required_tokens: alloc::vec![1],
        }
    }
    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let report = run_network_diagnostics();
        Ok(report.into_bytes())
    }
}
