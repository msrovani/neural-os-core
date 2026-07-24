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

static SYNCED: AtomicBool = AtomicBool::new(false);
static UNIX_AT_SYNC: AtomicU64 = AtomicU64::new(0);
static TICKS_AT_SYNC: AtomicU64 = AtomicU64::new(0);
static LAST_SERVER: Mutex<[u8; 4]> = Mutex::new([0; 4]);
static TRIED: AtomicBool = AtomicBool::new(false);

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
}

/// Sync one-shot. Non-fatal. Retorna true se PASS.
pub fn sync_once() -> bool {
    k_nano::slog_bin!("NTP", "info", "step=sync status=START");

    let server = crate::net_bridge::dns_resolve("time.cloudflare.com")
        .or_else(|| crate::net_bridge::dns_resolve("time.google.com"))
        .unwrap_or(FALLBACK_NTP_IP);

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

/// Boot/Cron: no máximo uma tentativa automática (evita flood UDP).
pub fn try_sync() -> bool {
    if is_synced() {
        return true;
    }
    if TRIED.swap(true, Ordering::Relaxed) {
        return false;
    }
    sync_once()
}

/// Status line para Cron/shell.
pub fn status_line() -> alloc::string::String {
    if let Some(u) = now_unix() {
        let (h, m, s) = format_hms(u);
        alloc::format!("NTP synced unix={} utc={:02}:{:02}:{:02}", u, h, m, s)
    } else if TRIED.load(Ordering::Relaxed) {
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
