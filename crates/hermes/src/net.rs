use k_nano::rtl8139::Rtl8139Driver;
use k_nano::e1000::{E1000Driver, REG_STATUS, REG_RDH};
use k_nano::{println};
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

pub const TOPIC_HW_NET_RTL8139: &str = "HW_NET_RTL8139";
pub const TOPIC_NETWORK_CONFIGURED: &str = "NETWORK_CONFIGURED";
pub const TOPIC_NETWORK_DEGRADED: &str = "NETWORK_DEGRADED";
pub const TOPIC_NETWORK_HEALTH: &str = "NETWORK_HEALTH";

pub static RTL8139: spin::Mutex<Option<Rtl8139Driver>> = spin::Mutex::new(None);
pub static E1000: spin::Mutex<Option<E1000Driver>> = spin::Mutex::new(None);
pub static VIRTIO_DEV: spin::Mutex<Option<k_nano::virtio_net::VirtIoDevice>> = spin::Mutex::new(None);
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
    pub is_dev_env: bool,  // QEMU/VBox dev/debug environment
}

pub static NET_CONFIG: spin::Mutex<NetConfig> = spin::Mutex::new(NetConfig {
    mac: [0; 6],
    ip: [0; 4],
    gateway_ip: [10, 0, 2, 2],  // QEMU SLiRP gateway is 10.0.2.2, not 10.0.2.1
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
        // Safety fallback: se ticks nao avancam, timeout apos ~1B iteracoes
        if guard >= 100_000_000 { break; }
        guard += 1;
        x86_64::instructions::hlt();
    }
}

/// Retorna nome do hypervisor detectado via CPUID
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

/// Detecta se estamos rodando em ambiente dev/debug (QEMU/VBox)
/// Usa CPUID hypervisor bit e hypervisor name
pub fn detect_dev_env() -> bool {
    unsafe {
        // CPUID leaf 0x1: ECX bit 31 indica presença de hypervisor
        let leaf1 = core::arch::x86_64::__cpuid(1);
        let has_hypervisor = (leaf1.ecx & (1 << 31)) != 0;
        
        if has_hypervisor {
            // Hypervisor detectado, verificar nome
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
                
                // QEMU, KVM, VBox, VMware, WHPX sao ambientes dev
                let name_lower = name.to_ascii_lowercase();
                return name_lower.contains("qemu") || 
                       name_lower.contains("kvm") || 
                       name_lower.contains("vbox") || 
                       name_lower.contains("vmware") ||
                       name_lower.contains("tcg") ||  // QEMU TCG puro
                       name_lower.contains("micr"); // Microsoft Hv = WHPX
            }
        }
        
        // Fallback: verificar MAC address (VirtualBox = 08:00:27, QEMU = 52:54:00)
        // Isso detecta VirtualBox mesmo SEM hypervisor visivel (ex: sem VT-x)
        let mac = NET_CONFIG.lock().mac;
        if mac[0] == 0x08 && mac[1] == 0x00 && mac[2] == 0x27 { return true; } // VBox
        if mac[0] == 0x52 && mac[1] == 0x54 && mac[2] == 0x00 { return true; } // QEMU
        
        false
    }
}

pub unsafe fn init_driver_rtl8139() -> bool {
    // Ja inicializado? (ex: chamado em kernel_main antes dos agentes)
    if RTL8139.lock().is_some() { return true; }
    let pci_devices = k_nano::pci::scan_pci();
    let mut dev_opt = None;
    for dev in &pci_devices {
        if dev.vendor_id == 0x10EC && dev.device_id == 0x8139 {
            k_nano::slog_hermes!("Net", "info", "RTL8139 detectado: {:02x}:{:02x}.{:02x}", dev.bus, dev.device, dev.function);
            println!("[NET] RTL8139 detectado.");
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
    println!("[NET] RTL8139 iniciado.");

    let hw_event = event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_HW_NET_RTL8139),
        payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
        token: event_bus::CapabilityToken::Legacy(1),
    };
    let _ = k_nano::EVENT_BUS.publish(hw_event);
    true
}

/// Inicializa driver e1000e (Intel Gigabit Ethernet).
/// Tentado como fallback se RTL8139 nao for encontrado.
pub unsafe fn init_driver_e1000() -> bool {
    if E1000.lock().is_some() { return true; }
    let pci_devices = k_nano::pci::scan_pci();
    for dev in &pci_devices {
        if dev.vendor_id == 0x8086 {
            let valid_devices = [0x100E, 0x10D3, 0x1502];
            if valid_devices.contains(&dev.device_id) {
                k_nano::slog_hermes!("Net", "info", "e1000 detectado: {:02x}:{:02x}.{:02x} device={:#06x}", dev.bus, dev.device, dev.function, dev.device_id);
                let mut driver = match E1000Driver::new(dev) { Some(d) => d, None => { k_nano::slog_hermes!("Net", "info", "E1000 new() falhou"); return false; } };
                if driver.init() {
                    let mac = driver.mac();
                    NET_CONFIG.lock().mac = mac;
                    *E1000.lock() = Some(driver);

                    k_nano::slog_hermes!("Net", "info", "e1000 iniciado. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
                    println!("[NET] e1000 iniciado.");

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
    }
    k_nano::slog_hermes!("Net", "info", "e1000 nao encontrado.");
    false
}

/// Inicializa serial tunnel (SLIP) como fallback quando nenhuma NIC existe.
pub unsafe fn init_serial_tunnel() -> bool {
    k_nano::slog_hermes!("Net", "info", "Inicializando serial tunnel (COM2 bypass)...");
    let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0xFE];
    NET_CONFIG.lock().mac = mac;
    k_nano::slog_hermes!("Net", "info", "Serial tunnel ativo. Fake MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    k_nano::slog_hermes!("Net", "info", "Modo offline — aguardando trafego via serial tunnel.");
    true
}

/// Track last known link state for RX kick logic
static mut E1000_LINK_WAS_UP: bool = false;

pub unsafe fn dump_e1000_status() {
    let mut guard = E1000.lock();
    if let Some(ref mut nic) = *guard {
        nic.dump_status();
        // Read current link state from STATUS register
        let status = nic.read32(REG_STATUS);
        let link_up = status & 0x02 != 0;
        let rdh = nic.read32(REG_RDH);
        // If link just came up or RDH never moved, kick RX
        if (link_up && !E1000_LINK_WAS_UP) || (link_up && rdh == 0) {
            nic.kick_rx();
        }
        E1000_LINK_WAS_UP = link_up;
    }
}

/// HTTP GET real via netstack. Usa o socket TCP do smoltcp.
/// HTTP GET real via NetStack::http_new + http_poll + http_close
pub unsafe fn http_get(host: [u8; 4], port: u16, path: &str) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

    let mut conn = stack.http_new(host, port, path);
    for _ in 0..2000 {
        stack.http_poll(&mut conn, now as u64);
        match conn.state {
            crate::netstack::HttpState::Done(ref data) => { return Some(data.clone()); }
            crate::netstack::HttpState::Failed => { break; }
            _ => { core::hint::spin_loop(); }
        }
    }
    None
}

/// Envia dados brutos via TCP e recebe resposta (usado por SMTP, etc)
pub unsafe fn http_get_raw(host: [u8; 4], port: u16, data: &[u8]) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

    let mut conn = stack.http_new(host, port, "/");
    stack.http_send_raw(&mut conn, data);
    for _ in 0..2000 {
        stack.http_poll(&mut conn, now as u64);
        match conn.state {
            crate::netstack::HttpState::Done(ref d) => { return Some(d.clone()); }
            crate::netstack::HttpState::Failed => { break; }
            _ => { core::hint::spin_loop(); }
        }
    }
    None
}

pub unsafe fn ping(_target_ip: [u8; 4]) -> Option<u64> { None }

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
