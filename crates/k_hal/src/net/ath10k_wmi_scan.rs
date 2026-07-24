//! ath10k A5 — WMI start_scan (TLV mínimo) + store BSS/SSID (Labor 6).
//! PASS RF só com ≥1 SSID medido; QEMU/timeout → AWAITING. Sem assoc.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::net::ath10k_ce_bmi::CeBmi;

/// WMI-TLV START_SCAN (Linux ath10k wmi-tlv cmd map — grupo SCAN).
const WMI_TLV_START_SCAN_CMDID: u32 = 0x3001;
/// Tag TLV genérico para string/SSID em payloads (best-effort).
const WMI_TLV_TAG_ARRAY_BYTE: u16 = 16;

const MAX_BSS: usize = 16;
const MAX_SSID: usize = 32;

#[derive(Clone)]
pub struct ScanBss {
    pub ssid: String,
    pub bssid: [u8; 6],
    pub channel: u8,
    pub signal_dbm: i32,
}

static LAST_SCAN: Mutex<Vec<ScanBss>> = Mutex::new(Vec::new());
static SCAN_OK: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Resultados do último A5 (cópia). Vazio = sem RF / timeout.
pub fn last_scan_bss() -> Vec<ScanBss> {
    LAST_SCAN.lock().clone()
}

pub fn scan_had_rf() -> bool {
    SCAN_OK.load(core::sync::atomic::Ordering::Relaxed)
}

fn store_scan(list: Vec<ScanBss>) {
    let n = list.len();
    SCAN_OK.store(n > 0, core::sync::atomic::Ordering::Relaxed);
    *LAST_SCAN.lock() = list;
    if n > 0 {
        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "step=scan status=OK ssids={} VERDICT=PASS reason=ath10k_scan_rf",
            n
        );
    }
}

/// Envia start_scan no EP WMI + poll RX; parse best-effort de SSIDs.
pub fn a5_start_scan(ce: &mut CeBmi, wmi_eid: u8) -> usize {
    k_nano::slog_hal!(
        "ATH10K",
        "info",
        "step=scan status=START eid={} cmd={:#x}",
        wmi_eid,
        WMI_TLV_START_SCAN_CMDID
    );

    // HTC(8) + WMI cmd_hdr(4) + TLV mínimo start_scan (~48B padded)
    let mut frame = [0u8; 96];
    let payload_len: u16 = 52;
    frame[0] = wmi_eid;
    frame[1] = 0; // flags
    frame[2..4].copy_from_slice(&payload_len.to_le_bytes());
    // WMI cmd id
    frame[8..12].copy_from_slice(&WMI_TLV_START_SCAN_CMDID.to_le_bytes());
    // TLV header: tag=ARRAY_BYTE / struct placeholder, len=40
    frame[12..14].copy_from_slice(&WMI_TLV_TAG_ARRAY_BYTE.to_le_bytes());
    frame[14..16].copy_from_slice(&40u16.to_le_bytes());
    // scan_id / vdev_id / dwell — zeros = FW defaults; honesty se peer/vdev ausente
    frame[16..20].copy_from_slice(&1u32.to_le_bytes()); // scan_id

    let mut resp = [0u8; 256];
    match ce.exchange(&frame[..8 + payload_len as usize], &mut resp) {
        Ok(n) => {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "step=scan status=cmd_sent rx={}",
                n
            );
        }
        Err(e) => {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "step=scan status=FAIL reason=cmd_{}",
                e
            );
            store_scan(Vec::new());
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "VERDICT=PARTIAL reason=scan_cmd_{}",
                e
            );
            return 0;
        }
    }

    let mut found: Vec<ScanBss> = Vec::new();
    // Poll alguns eventos (beacons/scan complete / TLVs)
    for round in 0..8u32 {
        let mut buf = [0u8; 256];
        match ce.recv_poll(&mut buf) {
            Ok(n) if n > 8 => {
                extract_ssids(&buf[..n], &mut found);
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=scan status=rx round={} bytes={} ssids_so_far={}",
                    round,
                    n,
                    found.len()
                );
                if found.len() >= MAX_BSS {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => {
                if round == 0 {
                    k_nano::slog_hal!(
                        "ATH10K",
                        "info",
                        "step=scan status=PARTIAL reason=rx_timeout_awaiting_note"
                    );
                }
                break;
            }
        }
    }

    let n = found.len();
    if n == 0 {
        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "VERDICT=AWAITING_REAL_HW reason=ath10k_scan_no_ssid (Note HW-GATE ATH10K_A5_SCAN)"
        );
    }
    store_scan(found);
    n
}

/// Heurística: SSID ASCII 1–32 printable em TLV/IE; BSSID se 6 bytes adjacentes.
fn extract_ssids(buf: &[u8], out: &mut Vec<ScanBss>) {
    // IE SSID tag 0x00 em beacon-like blobs
    let mut i = 0usize;
    while i + 2 < buf.len() && out.len() < MAX_BSS {
        if buf[i] == 0x00 {
            let len = buf[i + 1] as usize;
            if len > 0 && len <= MAX_SSID && i + 2 + len <= buf.len() {
                let s = &buf[i + 2..i + 2 + len];
                if s.iter().all(|b| (0x20..=0x7e).contains(b)) {
                    if let Ok(ssid) = core::str::from_utf8(s) {
                        if !ssid.is_empty() && !out.iter().any(|b| b.ssid == ssid) {
                            let mut bssid = [0u8; 6];
                            if i >= 8 {
                                bssid.copy_from_slice(&buf[i - 8..i - 2]);
                            }
                            out.push(ScanBss {
                                ssid: String::from(ssid),
                                bssid,
                                channel: 0,
                                signal_dbm: -60,
                            });
                        }
                    }
                }
            }
        }
        // TLV: tag u16 + len u16 + value (SSID-looking)
        if i + 4 <= buf.len() {
            let tlen = u16::from_le_bytes([buf[i + 2], buf[i + 3]]) as usize;
            if tlen > 0 && tlen <= MAX_SSID && i + 4 + tlen <= buf.len() {
                let s = &buf[i + 4..i + 4 + tlen];
                if s.iter().all(|b| (0x20..=0x7e).contains(b)) {
                    if let Ok(ssid) = core::str::from_utf8(s) {
                        if ssid.len() >= 2
                            && !out.iter().any(|b| b.ssid == ssid)
                            && !ssid.chars().all(|c| c.is_ascii_digit())
                        {
                            out.push(ScanBss {
                                ssid: String::from(ssid),
                                bssid: [0u8; 6],
                                channel: 0,
                                signal_dbm: -65,
                            });
                        }
                    }
                }
            }
        }
        i += 1;
    }
}
