use alloc::string::String;
use core::sync::atomic::Ordering;

/// Skill de diagnostico de rede para JARVIS.
/// Testa cada protocolo e reporta status no log.
pub fn run_network_test() -> String {
    let mut report = String::from("\n======= DIAGNOSTICO DE REDE =======\n");

    // 1. Ambiente
    let env = k_nano::env::get();
    report.push_str(&alloc::format!("Ambiente: {:?}\n", env));
    report.push_str(&alloc::format!("Sandbox: {}\n", k_nano::env::is_sandbox()));
    report.push_str(&alloc::format!("Online: {}\n", k_nano::env::is_online()));

    // 2. Netstack
    let ns_guard = crate::net::NETSTACK.lock();
    if ns_guard.is_some() {
        report.push_str("[OK] NETSTACK inicializado\n");
    } else {
        report.push_str("[FAIL] NETSTACK nao inicializado\n");
        return report;
    }
    drop(ns_guard);

    // 3. Configuracao IP
    let cfg = crate::net::NET_CONFIG.lock();
    report.push_str(&alloc::format!("MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n",
        cfg.mac[0], cfg.mac[1], cfg.mac[2], cfg.mac[3], cfg.mac[4], cfg.mac[5]));
    report.push_str(&alloc::format!("IP: {}.{}.{}.{}\n", cfg.ip[0], cfg.ip[1], cfg.ip[2], cfg.ip[3]));
    report.push_str(&alloc::format!("Gateway: {}.{}.{}.{}\n", cfg.gateway_ip[0], cfg.gateway_ip[1], cfg.gateway_ip[2], cfg.gateway_ip[3]));
    report.push_str(&alloc::format!("DNS: {}.{}.{}.{}\n", cfg.dns_ip[0], cfg.dns_ip[1], cfg.dns_ip[2], cfg.dns_ip[3]));
    report.push_str(&alloc::format!("Configurado: {}\n", cfg.configured));
    report.push_str(&alloc::format!("Online: {}\n", cfg.online));
    drop(cfg);

    // 4. Estatisticas de TX/RX
    let tx = crate::netstack::net_tx_count();
    let rx = crate::netstack::net_rx_count();
    report.push_str(&alloc::format!("TX total (NET): {}\n", tx));
    report.push_str(&alloc::format!("RX total (NET): {}\n", rx));
    let slip_tx = k_nano::slip::slip_tx_count();
    report.push_str(&alloc::format!("SLIP TX total: {}\n", slip_tx));

    // 5. Teste de resolucao DNS (usa smoltcp internamente)
    report.push_str("\n--- Teste DNS ---\n");
    let dns_ip = [8, 8, 8, 8]; // Google DNS
    let mut ns = crate::net::NETSTACK.lock();
    if let Some(ref mut netstack) = *ns {
        let result = netstack.dns_resolve("google.com", dns_ip);
        match result {
            Some(ip) => report.push_str(&alloc::format!("[OK] google.com -> {}.{}.{}.{}\n", ip[0], ip[1], ip[2], ip[3])),
            None => report.push_str("[FAIL] DNS timeout\n"),
        }
    }
    drop(ns);

    // 6. Teste HTTP (fetch google.com)
    report.push_str("\n--- Teste HTTP ---\n");
    let mut ns = crate::net::NETSTACK.lock();
    if let Some(ref mut netstack) = *ns {
        let target_ip = [142, 250, 80, 110]; // google.com IP fixo como fallback
        let mut conn = netstack.http_new(target_ip, 80, "/");
        let start = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        let _timeout = start.wrapping_add(200); // ~10 segundos
        loop {
            let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
            if now.wrapping_sub(start) > 200 { break; }
            netstack.http_poll(&mut conn, (now * 55) as u64);
            match &conn.state {
                crate::netstack::HttpState::Done(data) => {
                    let text = core::str::from_utf8(data).unwrap_or("<binario>");
                    report.push_str(&alloc::format!("[OK] HTTP OK ({} bytes): {}\n", data.len(), &text[..core::cmp::min(100, text.len())]));
                    break;
                }
                crate::netstack::HttpState::Failed => {
                    report.push_str("[FAIL] HTTP failed\n");
                    break;
                }
                _ => {}
            }
        }
        netstack.http_close(&mut conn);
    }
    drop(ns);

    report.push_str("\n======= FIM DIAGNOSTICO =======\n");
    report
}
