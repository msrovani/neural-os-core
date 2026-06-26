use crate::e1000::E1000Driver;
use crate::rtl8139::Rtl8139Driver;
use crate::{println, serial_println};
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

pub const TOPIC_HW_NET_E1000: &str = "HW_NET_E1000";
pub const TOPIC_HW_NET_RTL8139: &str = "HW_NET_RTL8139";
pub const TOPIC_NETWORK_CONFIGURED: &str = "NETWORK_CONFIGURED";
pub const TOPIC_NETWORK_DEGRADED: &str = "NETWORK_DEGRADED";
pub const TOPIC_NETWORK_HEALTH: &str = "NETWORK_HEALTH";

pub static E1000: spin::Mutex<Option<E1000Driver>> = spin::Mutex::new(None);
pub static RTL8139: spin::Mutex<Option<Rtl8139Driver>> = spin::Mutex::new(None);

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

pub fn wait_ticks(ticks: usize) {
    let start = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let mut guard: usize = 0;
    loop {
        let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        if now.wrapping_sub(start) >= ticks { break; }
        // Safety fallback: se ticks nao avancam, timeout apos ~1B iteracoes
        if guard >= 100_000_000 { break; }
        guard += 1;
        x86_64::instructions::hlt();
    }
}

pub unsafe fn init_driver_network() -> bool {
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
            println!("[NET] Nenhum dispositivo de rede encontrado.");
            return false;
        }
    };

    let mac = driver.mac();
    NET_CONFIG.lock().mac = mac;
    *E1000.lock() = Some(driver);

    serial_println!("[NET] e1000 iniciado. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    println!("[NET] e1000 iniciado.");

    let hw_event = crate::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_HW_NET_E1000),
        payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
        token: crate::CapabilityToken(1),
    };
    let _ = crate::EVENT_BUS.publish(hw_event);
    true
}

pub unsafe fn init_driver_rtl8139() -> bool {
    let pci_devices = crate::pci::scan_pci();
    let mut dev_opt = None;
    for dev in &pci_devices {
        if dev.vendor_id == 0x10EC && dev.device_id == 0x8139 {
            serial_println!("[NET] RTL8139 detectado: {:02x}:{:02x}.{:02x}", dev.bus, dev.device, dev.function);
            println!("[NET] RTL8139 detectado.");
            let mut driver = Rtl8139Driver::new(dev).unwrap();
            if driver.init() {
                dev_opt = Some(driver);
            }
            break;
        }
    }

    let driver = match dev_opt {
        Some(d) => d,
        None => {
            serial_println!("[NET] RTL8139 nao encontrado.");
            return false;
        }
    };

    let mac = driver.mac();
    NET_CONFIG.lock().mac = mac;
    *RTL8139.lock() = Some(driver);

    serial_println!("[NET] RTL8139 iniciado. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    println!("[NET] RTL8139 iniciado.");

    let hw_event = crate::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_HW_NET_RTL8139),
        payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
        token: crate::CapabilityToken(1),
    };
    let _ = crate::EVENT_BUS.publish(hw_event);
    true
}

pub unsafe fn http_get(host: [u8; 4], port: u16, path: &str) -> Option<Vec<u8>> {
    let cfg = NET_CONFIG.lock();
    let our_ip = cfg.ip;
    let our_mac = cfg.mac;
    let gw_mac = cfg.gateway_mac;
    drop(cfg);
    if gw_mac == [0; 6] { return None; }

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();
    crate::proto::http_get_request(driver, our_mac, gw_mac, our_ip, host, port, path);
    drop(guard);

    for _ in 0..100000000 { core::hint::spin_loop(); }

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();
    if let Some(pkt) = driver.recv() {
        return crate::proto::parse_http_response(&pkt, our_mac);
    }
    None
}

pub unsafe fn ping(target_ip: [u8; 4]) -> Option<u64> {
    let cfg = NET_CONFIG.lock();
    let our_ip = cfg.ip;
    let our_mac = cfg.mac;
    let gw_mac = cfg.gateway_mac;
    drop(cfg);
    if gw_mac == [0; 6] { return None; }

    let ident: u16 = 0x1234;
    let seq: u16 = 1;
    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();
    crate::proto::icmp_echo_request(driver, our_mac, gw_mac, our_ip, target_ip, ident, seq);
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
    let online = cfg.online;
    drop(cfg);

    let mut report = crate::String::new();
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
