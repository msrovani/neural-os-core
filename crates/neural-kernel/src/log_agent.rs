//! LogAgent — telemetria dev↔neural (ADR-0086 §3.5, gap I11).
//! O OS empurra o BOOT.LOG para o server do dev (POST /api/logs) — o opencode
//! analisa a quente e gera updates. Push (não pull): o neural já conhece a URL
//! do UPDATE.CFG; sem listener TCP no OS.

use alloc::string::String;
use alloc::format;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// T-026: não spam POST (PIT ~18Hz → 2000 ticks ≈ 110s).
static LAST_PUSH_TICK: AtomicU64 = AtomicU64::new(0);
const PUSH_BACKOFF_TICKS: u64 = 2000;

/// Envia o BOOT.LOG atual para o server (POST /api/logs). Best-effort.
/// URL base vem do UPDATE.CFG (config file, nunca hardcoded).
pub fn push_boot_log() -> String {
    let Some(base) = crate::self_update::read_update_cfg() else {
        return String::from("telemetry skip: sem UPDATE.CFG\n");
    };
    // Extrai host:port do base (ex: http://10.0.2.2:8080/UPDATE.MANIFEST)
    let Ok((host, port)) = host_port_from_cfg(&base) else {
        return String::from("telemetry skip: URL invalida no UPDATE.CFG\n");
    };
    let log = crate::boot_log_agent::BootLogAgent::read_last_boot_log()
        .unwrap_or_else(|| String::from("[BOOT.LOG indisponivel]"));
    let body = log.into_bytes();
    let req = build_post_req(host, port, &body);
    let resp = unsafe { crate::net::tcp_exchange(host, port, &req) };
    match resp {
        Some(r) => {
            let code = if r.len() > 12 { String::from_utf8_lossy(&r[9..12]).into_owned() } else { String::from("???") };
            k_nano::slog_bin!("TELE", "info", "push boot.log -> {}:{} (HTTP {}) bytes={}",
                host[0], host[3], code, body.len());
            alloc::format!("Telemetry push: HTTP {} ({} bytes)\n", code, body.len())
        }
        None => String::from("Telemetry push FAIL (rede indisponivel)\n"),
    }
}

/// Cron/Continuous: POST /api/logs com backoff s269 (ADR-0100 T-026).
pub fn maybe_push_periodic(tick: u64) {
    let last = LAST_PUSH_TICK.load(Ordering::Relaxed);
    if last != 0 && tick.saturating_sub(last) < PUSH_BACKOFF_TICKS {
        return;
    }
    if !crate::net::NET_CONFIG.lock().online {
        return;
    }
    LAST_PUSH_TICK.store(tick, Ordering::Relaxed);
    let _ = push_boot_log();
}

/// Monta request POST /api/logs com Content-Length. ponytail: connection close,
/// sem keep-alive (smoltcp simples).
fn build_post_req(host: [u8; 4], port: u16, body: &[u8]) -> Vec<u8> {
    let mut req = Vec::new();
    req.extend_from_slice(b"POST /api/logs HTTP/1.1\r\n");
    req.extend_from_slice(b"Host: ");
    req.extend_from_slice(host_str(host).as_bytes());
    req.extend_from_slice(b"\r\nContent-Type: text/plain\r\n");
    req.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    req.extend_from_slice(b"Connection: close\r\n\r\n");
    req.extend_from_slice(body);
    let _ = port;
    req
}

fn host_str(host: [u8; 4]) -> String {
    alloc::format!("{}.{}.{}.{}", host[0], host[1], host[2], host[3])
}

/// Extrai (host, port) do base URL (http://h:p/...). Default port 8080.
fn host_port_from_cfg(base: &str) -> Result<([u8; 4], u16), &'static str> {
    let rest = base
        .strip_prefix("http://")
        .or_else(|| base.strip_prefix("HTTP://"))
        .ok_or("bad_url")?;
    let hostport = match rest.find('/') {
        Some(i) => &rest[..i],
        None => rest,
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(8080)),
        None => (hostport, 8080),
    };
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return Err("bad_host");
    }
    let mut ip = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        ip[i] = p.parse::<u8>().map_err(|_| "bad_ip")?;
    }
    Ok((ip, port))
}

#[cfg(test)]
mod tests {
    use super::{build_post_req, host_port_from_cfg};

    #[test]
    fn cfg_host_port() {
        let (ip, port) = host_port_from_cfg("http://10.0.2.2:8080/UPDATE.MANIFEST").unwrap();
        assert_eq!(ip, [10, 0, 2, 2]);
        assert_eq!(port, 8080);
        let (ip, port) = host_port_from_cfg("http://192.168.137.1:8080/UPDATE.MANIFEST").unwrap();
        assert_eq!(ip, [192, 168, 137, 1]);
        assert_eq!(port, 8080);
    }

    #[test]
    fn post_has_length() {
        let req = build_post_req([10, 0, 2, 2], 8080, b"hello");
        let s = String::from_utf8_lossy(&req);
        assert!(s.starts_with("POST /api/logs HTTP/1.1"));
        assert!(s.contains("Content-Length: 5"));
        assert!(s.ends_with("hello"));
    }
}
