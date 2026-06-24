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

    // Try DHCP
    if try_dhcp() {
        serial_println!("[NET] DHCP bem-sucedido.");
        println!("[NET] Configuracao de rede via DHCP.");
        let cfg = NET_CONFIG.lock();
        serial_println!("[NET] IP: {}.{}.{}.{} / GW: {}.{}.{}.{} / DNS: {}.{}.{}.{}",
            cfg.ip[0], cfg.ip[1], cfg.ip[2], cfg.ip[3],
            cfg.gateway_ip[0], cfg.gateway_ip[1], cfg.gateway_ip[2], cfg.gateway_ip[3],
            cfg.dns_ip[0], cfg.dns_ip[1], cfg.dns_ip[2], cfg.dns_ip[3]);
        true
    } else {
        serial_println!("[NET] DHCP falhou. Usando configuracao estatica (10.0.2.15).");
        println!("[NET] DHCP indisponivel. Usando IP estatico.");
        let mut cfg = NET_CONFIG.lock();
        cfg.ip = [10, 0, 2, 15];
        cfg.configured = true;
        true
    }
}

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

pub unsafe fn resolve_gateway() -> bool {
    let cfg = NET_CONFIG.lock();
    if cfg.configured && cfg.gateway_mac != [0; 6] {
        return true;
    }
    drop(cfg);

    // Send ARP request for gateway
    let guard = E1000.lock();
    let driver = guard.as_ref().unwrap();
    let mac = driver.mac();
    drop(guard);

    let cfg = NET_CONFIG.lock();
    let gw = cfg.gateway_ip;
    drop(cfg);

    crate::proto::arp_request(mac, gw);
    for _ in 0..10000000 { core::hint::spin_loop(); }

    // Check for ARP reply
    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();
    if let Some(packet) = driver.recv() {
        if let Some(gw_mac) = crate::proto::parse_arp_reply(&packet) {
            drop(guard);
            NET_CONFIG.lock().gateway_mac = gw_mac;
            serial_println!("[ARP] Gateway MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                gw_mac[0], gw_mac[1], gw_mac[2], gw_mac[3], gw_mac[4], gw_mac[5]);
            return true;
        }
    }
    false
}

pub unsafe fn ping(ip: [u8; 4], timeout_iters: u64) -> Option<u64> {
    let cfg = NET_CONFIG.lock();
    let our_ip = cfg.ip;
    let our_mac = cfg.mac;
    let gw_mac = cfg.gateway_mac;
    drop(cfg);

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();

    let ident = 0x1234;
    let seq = 1;
    crate::proto::icmp_echo_request(driver, our_mac, gw_mac, our_ip, ip, ident, seq);

    // Wait for reply
    for _ in 0..timeout_iters {
        if let Some(packet) = driver.recv() {
            if let Some(rtt) = crate::proto::parse_icmp_reply(&packet, our_mac, ip, ident, seq) {
                drop(guard);
                return Some(rtt);
            }
        }
        core::hint::spin_loop();
    }
    drop(guard);
    None
}

pub unsafe fn dns_lookup(hostname: &str, timeout_iters: u64) -> Option<[u8; 4]> {
    let cfg = NET_CONFIG.lock();
    let our_ip = cfg.ip;
    let our_mac = cfg.mac;
    let dns_ip = cfg.dns_ip;
    let gw_mac = cfg.gateway_mac;
    drop(cfg);

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();

    let txid = 0xABCD;
    crate::proto::dns_query(driver, our_mac, gw_mac, our_ip, dns_ip, hostname, txid);

    for _ in 0..timeout_iters {
        if let Some(packet) = driver.recv() {
            if let Some(ip) = crate::proto::parse_dns_response(&packet, our_mac, dns_ip, txid) {
                drop(guard);
                return Some(ip);
            }
        }
        core::hint::spin_loop();
    }
    drop(guard);
    None
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
    report.push_str(&alloc::format!(
        "=== Diagnóstico de Rede [IA] ===\n"
    ));

    if !configured {
        report.push_str("❌ Rede não configurada.\n");
        report.push_str("ℹ️ Causa possível: driver não encontrado ou link down.\n");
        report.push_str("📋 Recomendação: verifique o adaptador de rede (e1000 ou VirtIO) no QEMU.\n");
        return report;
    }

    report.push_str(&alloc::format!(
        "📡 MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ));
    report.push_str(&alloc::format!(
        "🌐 IP: {}.{}.{}.{} / GW: {}.{}.{}.{} / DNS: {}.{}.{}.{}\n",
        ip[0], ip[1], ip[2], ip[3],
        gw[0], gw[1], gw[2], gw[3],
        dns[0], dns[1], dns[2], dns[3]
    ));

    // Ping gateway
    let ping_result = unsafe { ping(gw, 5000000) };
    if let Some(rtt) = ping_result {
        report.push_str(&alloc::format!("✅ Gateway {}ms - conectividade OK.\n", rtt));
    } else {
        report.push_str("⚠️ Gateway sem resposta. Pode ser firewall ou link down.\n");
    }

    // Try DNS
    let dns_result = unsafe { dns_lookup("google.com", 3000000) };
    if let Some(dns_ip) = dns_result {
        report.push_str(&alloc::format!(
            "✅ DNS funcional: google.com → {}.{}.{}.{}\n",
            dns_ip[0], dns_ip[1], dns_ip[2], dns_ip[3]
        ));
        report.push_str("🌍 Conexão com a internet estabelecida.\n");
    } else {
        report.push_str("⚠️ DNS sem resposta. Internet pode estar indisponível.\n");
    }

    report.push_str("\n🧠 Análise IA:\n");
    if online || ping_result.is_some() {
        report.push_str("Sistema com capacidade de rede. Skills podem buscar atualizações remotas.\n");
        report.push_str("Recomendação: Neo Hermes Terminal pode receber comandos via rede (Sprint 24+).\n");
    } else if configured {
        report.push_str("Driver configurado mas sem conectividade. Verifique firewall ou gateway.\n");
        report.push_str("Recomendação: tente /netconfig para reconfigurar rede.\n");
    } else {
        report.push_str("Sistema offline. Operação normal continua sem rede.\n");
        report.push_str("Recomendação: conecte o cabo de rede e reinicie /netinit.\n");
    }

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
