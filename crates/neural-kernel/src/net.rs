use crate::rtl8139::Rtl8139Driver;
use crate::e1000::{E1000Driver, REG_STATUS};
use crate::i225::I225Driver;
use crate::{println};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

pub const TOPIC_HW_NET_RTL8139: &str = "HW_NET_RTL8139";
pub const TOPIC_NETWORK_CONFIGURED: &str = "NETWORK_CONFIGURED";
pub const TOPIC_NETWORK_DEGRADED: &str = "NETWORK_DEGRADED";
pub const TOPIC_NETWORK_HEALTH: &str = "NETWORK_HEALTH";

// Driver statics agora vivem em k_nano (transporte P2P R0 usa o mesmo NIC).
// Re-export: `crate::net::E1000` escreve/le o static canônico de k_nano.
pub use k_nano::nic_globals::{RTL8139, E1000, VIRTIO_DEV};
pub static I225: spin::Mutex<Option<I225Driver>> = spin::Mutex::new(None);
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
    let pci_devices = crate::pci::scan_pci();
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

    let hw_event = crate::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_HW_NET_RTL8139),
        payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
        token: crate::CapabilityToken::Legacy(1),
    };
    let _ = crate::EVENT_BUS.publish(hw_event);
    true
}

/// Inicializa driver e1000e (Intel Gigabit Ethernet).
/// Tentado como fallback se RTL8139 nao for encontrado.
pub unsafe fn init_driver_e1000() -> bool {
    if E1000.lock().is_some() { return true; }
    let pci_devices = crate::pci::scan_pci();
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

                    let _ = crate::EVENT_BUS.publish(crate::Event {
                        id: 0,
                        topic: alloc::string::String::from("HW_NET_E1000"),
                        payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
                        token: crate::CapabilityToken::Legacy(1),
                    });
                    return true;
                }
            }
        }
    }
    k_nano::slog_hermes!("Net", "info", "e1000 nao encontrado.");
    false
}

/// ADR-0062 P7 / Labor 2: Intel I225/I226 (igc). Fallback quando e1000 ausente.
/// Honesty: QEMU não emula i225 — path real = HW; aqui só probe+init se DID casa.
pub unsafe fn init_driver_i225() -> bool {
    if I225.lock().is_some() {
        return true;
    }
    let pci_devices = crate::pci::scan_pci();
    for dev in &pci_devices {
        if !crate::i225::is_i225_family(dev.vendor_id, dev.device_id) {
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
                mac[0],
                mac[1],
                mac[2],
                mac[3],
                mac[4],
                mac[5]
            );
            let _ = crate::EVENT_BUS.publish(crate::Event {
                id: 0,
                topic: alloc::string::String::from("HW_NET_I225"),
                payload: alloc::vec![mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
                token: crate::CapabilityToken::Legacy(1),
            });
            return true;
        }
        k_nano::slog_hermes!("Net", "i225", "init() FAIL DID={:#06x}", dev.device_id);
    }
    k_nano::slog_hermes!("Net", "i225", "nao encontrado (esperado em QEMU — sem emu i225)");
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
static E1000_LINK_WAS_UP: AtomicBool = AtomicBool::new(false);

pub unsafe fn dump_e1000_status() {
    let mut guard = E1000.lock();
    if let Some(ref mut nic) = *guard {
        nic.dump_status();
        let status = nic.read32(REG_STATUS);
        let link_up = status & 0x02 != 0;
        // Kick ONLY on link-up transition (NOT every time RDH==0 — that resets RX mid-poll).
        if link_up && !E1000_LINK_WAS_UP.load(Ordering::Relaxed) {
            nic.kick_rx();
        }
        E1000_LINK_WAS_UP.store(link_up, Ordering::Relaxed);
    }
}

/// Phys loader @0x164000000: 'B' = bridge/TAP (DHCP), 'U' = user/slirp (static 10.0.2.15),
/// 'S' + 4 bytes = static IP customizado (ex: S\x02\x00\x03\x02 = 10.0.3.2).
/// SESSION_233: corrigido! Era 0x1640_0000_00 = 0x1640000000 (89GB, FORA da RAM
/// de 8GB) — o QEMU loader escrevia o flag num endereço que o kernel nunca lia
/// → detect_qemu_net_mode caía no default USER/slirp. Agora 0x164000000 = 5.56GB
/// (dentro de 8GB), alinhado ao comentário e ao run-qemu-p2p-mesh.ps1.
pub const NETMODE_LOADER_PHYS: u64 = 0x1640_0000_00 >> 4; // 0x164000000 (5.56GB)

/// Modo de rede detectado + IP customizado (se 'S').
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QemuNetMode {
    User,           // 10.0.2.15 padrao
    Bridge,         // DHCP
    Static([u8;4]), // IP customizado ex: 10.0.3.2
}

/// Read netmode.flag from QEMU loader window (written by run-qemu-*.ps1).
/// SESSION_248: os loaders MoE (auto-scan do PS1) começam em 0x100000000 e o
/// LLAMA8B (1.9GB) cobriria o endereço fixo antigo 0x164000000 — o flag viraria
/// lixo. Agora o kernel ESCANEIA o flag de TRÁS para frente (do topo da RAM
/// 8G para baixo, 1MB-aligned): o PS1 escreve o netmode.flag DEPOIS do último
/// loader, então ele é o primeiro candidato 'S'/'B' achado — a RAM livre acima
/// dos loaders é zero (QEMU zera o guest) e os dados de modelo nunca são lidos.
pub fn detect_qemu_net_mode() -> QemuNetMode {    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return QemuNetMode::User;
    }
    unsafe {
        // Topo da RAM = TOTAL_RAM_MB (setado pelo frame allocator no boot).
        // -m 6G padrão do mesh → 0x180000000. Nunca ler além da RAM real.
        let ram_end = crate::memory::TOTAL_RAM_MB
            .load(core::sync::atomic::Ordering::Relaxed)
            .saturating_mul(1024 * 1024)
            .min(0x200000000);
        if ram_end <= 0x100000000 {
            return QemuNetMode::User;
        }
        let mut addr: u64 = ram_end;
        while addr > 0x100000000 {
            addr -= 0x100000; // 1MB steps (alinhamento dos loaders do PS1)
            // Evita #PF: bootloader HHDM pode não cobrir janelas altas até touch.
            crate::apic::map_page_uc(addr, pmoff);
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
                    // Valida IP plausível (evita falso 'S').
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

/// Prove e1000 RX with ARP kick before DNS. Returns true if any packet/DD observed.
pub unsafe fn prove_e1000_rx(sip: [u8; 4], tip: [u8; 4]) -> bool {
    let mut guard = E1000.lock();
    if let Some(ref mut nic) = *guard {
        // ~3× ARP × ~2000×200µs ≈ 1.2s wall — slirp/WHPX precisa latência real
        let (rdh, dd, ok) = nic.prove_rx(sip, tip, 6_000);
        k_nano::slog_bin!("Net", "e1000", "prove_rx: ok={} rdh={} dd={} (ARP who-has {}.{}.{}.{})", ok, rdh, dd, tip[0], tip[1], tip[2], tip[3]);
        return ok;
    }
    k_nano::slog_bin!("Net", "e1000", "prove_rx SKIP: no e1000");
    false
}

/// Trigger OTA via flag QEMu-loader (padrão `netmode.flag`, SESSION_252).
/// O launch grava 'O' num endereço 1MB-aligned dentro da RAM; o kernel escaneia
/// a mesma janela (topo→baixo) procurando o marcador. Dispara
/// `check_for_update()` no boot SEM depender do teclado — o IRQ1 do teclado
/// não é entregue via IOAPIC no QEMU (bug documentado: sendkey nunca chegava
/// ao shell). Usado pelo loop smoke OTA (tools/qemu_ota_loop.ps1).
pub fn detect_qemu_ota_trigger() -> bool {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return false;
    }
    unsafe {
        let ram_end = crate::memory::TOTAL_RAM_MB
            .load(core::sync::atomic::Ordering::Relaxed)
            .saturating_mul(1024 * 1024)
            .min(0x200000000);
        if ram_end <= 0x100000000 {
            return false;
        }
        let mut addr: u64 = ram_end;
        while addr > 0x100000000 {
            addr -= 0x100000; // 1MB steps (alinhamento dos loaders do PS1)
            crate::apic::map_page_uc(addr, pmoff);
            let p = (addr + pmoff) as *const u8;
            if core::ptr::read_volatile(p) == b'O' {
                k_nano::slog_bin!("OTA", "info", "trigger flag encontrado @ {:#x}", addr);
                return true;
            }
        }
    }
    false
}

/// HTTP GET real via netstack. Usa o socket TCP do smoltcp.
/// HTTP GET real via NetStack::http_new + http_poll + http_close
pub unsafe fn http_get(host: [u8; 4], port: u16, path: &str) -> Option<Vec<u8>> {
    http_get_host(host, port, path, None)
}

/// HTTP GET with Host header (required for hostname/CDN targets).
pub unsafe fn http_get_host(
    host: [u8; 4],
    port: u16,
    path: &str,
    host_header: Option<&str>,
) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;

    let mut conn = stack.http_new_host(host, port, path, host_header);
    // Limite alto: downloads grandes (KERNEL.BIN ~17MB) sob TCG/slirp drenam
    // ~2KB por poll — 8000 polls cortava em ~17.4MB (hash_mismatch no OTA).
    // O server manda Connection: close → o socket fecha sozinho no fim do corpo.
    for _ in 0..200_000 {
        // Re-ler o tick a cada poll: o smoltcp precisa de TEMPO AVANÇANDO para
        // processar ACK/window/retransmissão. ANTES o `now` era lido uma vez e
        // congelado — downloads grandes (>8KB) truncavam no fim (o KERNEL.BIN
        // de 17MB vinha com 1748 bytes a menos → hash_mismatch no OTA).
        let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
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

/// Envia dados brutos via TCP e recebe resposta (SMTP / legado).
pub unsafe fn http_get_raw(host: [u8; 4], port: u16, data: &[u8]) -> Option<Vec<u8>> {
    tcp_exchange(host, port, data)
}

/// TCP framing exchange (NetFs #418, SMTP residual).
pub unsafe fn tcp_exchange(host: [u8; 4], port: u16, data: &[u8]) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    stack.tcp_exchange(host, port, data, now as u64)
}

/// Parse `http://host[:port]/path`. HTTPS → use `parse_https_url` / `https_get`.
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

/// DNS resolve via NETSTACK (raw UDP) using NET_CONFIG.dns_ip (fallback 10.0.2.3).
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

/// UDP exchange raw (NTP etc.) — payload in/out sem headers L2/L3/L4.
pub fn udp_exchange_safe(dst: [u8; 4], dst_port: u16, payload: &[u8]) -> Option<Vec<u8>> {
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut()?;
    stack.udp_exchange_raw(dst, dst_port, payload)
}

/// DNS resolve safe (bridge Hermes).
pub fn dns_resolve_host_safe(hostname: &str) -> Option<[u8; 4]> {
    unsafe { dns_resolve_host(hostname) }
}

/// Resolve hostname + HTTP(S) GET. Body only (headers stripped).
/// `https://` → TLS N4 (`https_get`); never silently strip to port 80.
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
        ip[0],
        ip[1],
        ip[2],
        ip[3],
        port,
        path,
        host
    );
    match http_get_host(ip, port, &path, Some(host.as_str())) {
        Some(body) if !body.is_empty() => Ok(body),
        Some(_) => Err("http_empty"),
        None => Err("http_failed"),
    }
}

/// Safe wrapper for hermes net_bridge (no `unsafe` in FE call sites).
pub fn resolve_and_http_get_safe(url: &str) -> Result<Vec<u8>, &'static str> {
    unsafe { resolve_and_http_get(url) }
}

/// HTTPS GET — ADR-0016 N4 (`embedded-tls` 0.19). Trust = unsecure (sem PKI).
/// Never strips https→http:80.
pub fn https_get(url: &str) -> Result<Vec<u8>, &'static str> {
    let (host, port, path) = crate::tls_client::parse_https_url(url)?;
    let ip = unsafe { dns_resolve_host(&host) }.ok_or("dns_failed")?;
    k_nano::slog_bin!(
        "TLS",
        "info",
        "GET {}.{}.{}.{}:{}{} Host={} trust=hybrid",
        ip[0],
        ip[1],
        ip[2],
        ip[3],
        port,
        path,
        host
    );
    let mut stack_guard = NETSTACK.lock();
    let stack = stack_guard.as_mut().ok_or("no_netstack")?;
    let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    match crate::tls_client::https_get_on_stack(stack, ip, port, &host, &path, now) {
        Ok(raw) => {
            let body = strip_http_envelope(&raw);
            if body.is_empty() {
                k_nano::slog_bin!("TLS", "info", "VERDICT=FAIL reason=http_empty");
                Err("http_empty")
            } else {
                k_nano::slog_bin!(
                    "TLS",
                    "info",
                    "VERDICT=PASS bytes={} trust={}",
                    body.len(),
                    crate::tls_trust::last_trust().as_str()
                );
                Ok(body)
            }
        }
        Err(e) => {
            k_nano::slog_bin!("TLS", "info", "VERDICT=FAIL reason={}", e);
            Err(e)
        }
    }
}

/// Call once at boot — wired N4, trust unsecure até PKI.
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

/// Smoke HTTPS pós-L5 (fora de `NETSTACK.lock` do bootstrap). Best-effort.
pub fn smoke_https_if_online() {
    use core::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::Relaxed) {
        return;
    }
    // TCG + soft-AES: handshake pode #UD (IP em faixa loader BitNet) e o handler faz hlt forever.
    // Smoke HTTPS canônico = WHPX `-cpu qemu64` (SESSION_157). Não bloquear AgentFleet/JARVIS.
    if matches!(
        k_nano::platform_probe::hypervisor(),
        k_nano::platform_probe::HypervisorKind::Tcg
    ) {
        k_nano::slog_bin!(
            "TLS",
            "info",
            "smoke=SKIP reason=tcg_ud_risk (use WHPX qemu64 p/ smoke=PASS)"
        );
        return;
    }
    // Só se L5 HTTP deste boot passou (internet real via e1000/slirp).
    let st = crate::network_agent::early_smoke_status();
    if st != "L5_OK" {
        k_nano::slog_bin!("TLS", "info", "smoke=SKIP reason=no_L5_OK status={}", st);
        return;
    }
    // google.com: vizinho/DNS já aquecidos no L4/L5 deste boot.
    // 1ª chamada → trust=root_learn; 2ª → trust=root_pin (pin sticky RAM).
    k_nano::slog_bin!("TLS", "info", "smoke=START url=https://www.google.com/");
    match https_get("https://www.google.com/") {
        Ok(body) => {
            k_nano::slog_bin!(
                "TLS",
                "info",
                "smoke=PASS bytes={} trust={} (google#1)",
                body.len(),
                crate::tls_trust::last_trust().as_str()
            );
        }
        Err(e) => {
            k_nano::slog_bin!("TLS", "info", "smoke=FAIL reason={}", e);
            return;
        }
    }
    match https_get("https://www.google.com/") {
        Ok(body) => {
            k_nano::slog_bin!(
                "TLS",
                "info",
                "smoke=PASS bytes={} trust={} (google#2)",
                body.len(),
                crate::tls_trust::last_trust().as_str()
            );
        }
        Err(e) => {
            k_nano::slog_bin!("TLS", "info", "smoke=FAIL reason={} (google#2)", e);
        }
    }
}

/// Resultado de GET com Range (AirLLM stream-to-disk).
pub struct HttpRangeBody {
    pub status: u16,
    pub body: Vec<u8>,
    /// Total do `Content-Range: bytes a-b/TOTAL` quando 206.
    pub total: Option<usize>,
}

/// HTTP GET ranged. Body sem headers; `total` se servidor enviar Content-Range.
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
    let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
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
            preconditions: alloc::vec![],
            context_links: alloc::vec![],
            output_schema: crate::OutputSchema::Any,
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
