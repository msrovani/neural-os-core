//! NTP client NTPv4 (ADR-0069 / Labor 10 P18).
//! WallClock = unix_at_sync + (ticks - ticks_at_sync) / TICKS_PER_SEC.
//! Sem rede → AWAITING_NET; sem escrever CMOS RTC.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// PIT ~18.2 Hz — shell já usa `/18` (honesty aproximado).
const TICKS_PER_SEC: u64 = 18;
/// Segundos NTP (1900) → Unix (1970).
const NTP_UNIX_DELTA: u64 = 2_208_988_800;
const NTP_PORT: u16 = 123;
/// Cloudflare time.anycast — fallback sem DNS.
const FALLBACK_NTP_IP: [u8; 4] = [162, 159, 200, 1];
/// Resync interval: ~200s at 18 Hz
const RESYNC_INTERVAL_TICKS: u64 = 3600;
/// Cooldown after failed attempt: ~30s
const FAIL_COOLDOWN_TICKS: u64 = 540;

static SYNCED: AtomicBool = AtomicBool::new(false);
static UNIX_AT_SYNC: AtomicU64 = AtomicU64::new(0);
static TICKS_AT_SYNC: AtomicU64 = AtomicU64::new(0);
static LAST_SERVER: Mutex<[u8; 4]> = Mutex::new([0; 4]);
static LAST_ATTEMPT_TICKS: AtomicU64 = AtomicU64::new(0);
static LAST_SERVER_IDX: Mutex<usize> = Mutex::new(0);

const NTP_SERVERS: &[&str] = &["time.cloudflare.com", "time.google.com", "pool.ntp.org"];

pub fn is_synced() -> bool {
    SYNCED.load(Ordering::Relaxed)
}

/// Unix time atual se synced; senão None.
pub fn now_unix() -> Option<u64> {
    if !is_synced() {
        return None;
    }
    let unix0 = UNIX_AT_SYNC.load(Ordering::Relaxed);
    let ticks0 = TICKS_AT_SYNC.load(Ordering::Relaxed);
    let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let elapsed = now.saturating_sub(ticks0) / TICKS_PER_SEC;
    Some(unix0.saturating_add(elapsed))
}

/// Formata HH:MM:SS UTC a partir de unix secs.
pub fn format_hms(unix: u64) -> (u32, u32, u32) {
    let tod = (unix % 86400) as u32;
    (tod / 3600, (tod % 3600) / 60, tod % 60)
}

fn build_request() -> [u8; 48] {
    let mut p = [0u8; 48];
    // LI=0, VN=4, Mode=3 (client)
    p[0] = 0x23;
    p
}

fn parse_unix_from_reply(pkt: &[u8]) -> Option<u64> {
    if pkt.len() < 48 {
        return None;
    }
    let mode = pkt[0] & 0x07;
    // server mode 4 preferred; accept 3–5
    if mode < 3 || mode > 5 {
        return None;
    }
    let secs = u32::from_be_bytes([pkt[40], pkt[41], pkt[42], pkt[43]]) as u64;
    if secs < NTP_UNIX_DELTA {
        return None;
    }
    Some(secs - NTP_UNIX_DELTA)
}

fn apply_sync(unix: u64, server: [u8; 4]) {
    let ticks = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    UNIX_AT_SYNC.store(unix, Ordering::Relaxed);
    TICKS_AT_SYNC.store(ticks, Ordering::Relaxed);
    SYNCED.store(true, Ordering::Relaxed);
    *LAST_SERVER.lock() = server;
    LAST_ATTEMPT_TICKS.store(ticks, Ordering::Relaxed);
}

/// Sync one-shot. Non-fatal. Retorna true se PASS.
pub fn sync_once() -> bool {
    k_nano::slog_bin!("NTP", "info", "step=sync status=START");

    // Try servers in rotation
    let mut server_idx = LAST_SERVER_IDX.lock();
    let start_idx = *server_idx;
    let mut server = [0u8; 4];
    let mut resolved = false;

    for i in 0..NTP_SERVERS.len() {
        let idx = (start_idx + i) % NTP_SERVERS.len();
        if let Some(ip) = crate::net_bridge::dns_resolve(NTP_SERVERS[idx]) {
            server = ip;
            *server_idx = (idx + 1) % NTP_SERVERS.len();
            resolved = true;
            break;
        }
    }
    if !resolved {
        server = FALLBACK_NTP_IP;
    }

    let req = build_request();
    let resp = match crate::net_bridge::udp_xfer(server, NTP_PORT, &req) {
        Some(r) => r,
        None => {
            k_nano::slog_bin!(
                "NTP",
                "info",
                "step=sync status=FAIL VERDICT=AWAITING_NET reason=udp_timeout server={}.{}.{}.{}",
                server[0],
                server[1],
                server[2],
                server[3]
            );
            return false;
        }
    };

    let unix = match parse_unix_from_reply(&resp) {
        Some(u) => u,
        None => {
            k_nano::slog_bin!(
                "NTP",
                "info",
                "step=sync status=FAIL VERDICT=FAIL reason=bad_packet len={}",
                resp.len()
            );
            return false;
        }
    };

    apply_sync(unix, server);
    let (h, m, s) = format_hms(unix);
    k_nano::slog_bin!(
        "NTP",
        "info",
        "step=sync status=OK VERDICT=PASS unix={} utc={:02}:{:02}:{:02} server={}.{}.{}.{}",
        unix,
        h,
        m,
        s,
        server[0],
        server[1],
        server[2],
        server[3]
    );
    true
}

/// Boot/Cron: tenta sync com cooldown (não one-shot gate).
/// Se já synced, faz resync periódico a cada RESYNC_INTERVAL_TICKS.
pub fn try_sync() -> bool {
    let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let last = LAST_ATTEMPT_TICKS.load(Ordering::Relaxed);

    if is_synced() {
        // Periodic resync
        if now.saturating_sub(last) >= RESYNC_INTERVAL_TICKS {
            k_nano::slog_bin!("NTP", "info", "step=resync status=START periodic");
            if sync_once() {
                return true;
            }
        }
        return true;
    }

    // Not synced yet: respect fail cooldown
    if last != 0 && now.saturating_sub(last) < FAIL_COOLDOWN_TICKS {
        return false;
    }

    sync_once()
}

/// Status line para Cron/shell.
pub fn status_line() -> alloc::string::String {
    if let Some(u) = now_unix() {
        let (h, m, s) = format_hms(u);
        alloc::format!("NTP synced unix={} utc={:02}:{:02}:{:02}", u, h, m, s)
    } else if LAST_ATTEMPT_TICKS.load(Ordering::Relaxed) != 0 {
        alloc::string::String::from("NTP AWAITING_NET (ticks only)")
    } else {
        alloc::string::String::from("NTP not attempted")
    }
}

/// Labor 35: RTC CMOS write = off; NTS = SKIP honesty.
pub fn residual_boot_smoke() {
    k_nano::slog_bin!(
        "NTP",
        "info",
        "step=rtc status=SKIP VERDICT=SKIP reason=cmos_write_off"
    );
    k_nano::slog_bin!(
        "NTP",
        "info",
        "step=nts status=SKIP VERDICT=SKIP reason=nts_not_implemented"
    );
    if is_synced() {
        k_nano::slog_bin!(
            "NTP",
            "info",
            "step=wallclock status=OK VERDICT=PARTIAL reason=udp_synced"
        );
    } else {
        k_nano::slog_bin!(
            "NTP",
            "info",
            "step=wallclock status=AWAITING VERDICT=PARTIAL reason=ticks_only"
        );
    }
}






