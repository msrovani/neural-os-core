//! ath10k A6 — WMI associate/connect TLV (Labor 14 / ADR-0066).
//! Connected só com evidência FW; QEMU sem RF → AWAITING. Sem SoftMAC.

use crate::net::ath10k_ce_bmi::CeBmi;
use crate::net::ath10k_wmi_scan;

/// WMI-TLV VDEV_UP / START approx — best-effort cmd id (assoc group).
const WMI_TLV_VDEV_START_CMDID: u32 = 0x3002;
const WMI_TLV_TAG_ARRAY_BYTE: u16 = 16;

static ASSOC_OK: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

pub fn assoc_ok() -> bool {
    ASSOC_OK.load(core::sync::atomic::Ordering::Relaxed)
}

/// Tenta assoc ao primeiro BSS do LAST_SCAN. Retorna true só se FW ACK suspeito + ssid.
pub fn a6_try_assoc(ce: &mut CeBmi, wmi_eid: u8, ssid: &str) -> bool {
    let scan = ath10k_wmi_scan::last_scan_bss();
    if scan.is_empty() {
        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "step=assoc status=SKIP reason=no_scan_bss ssid={}",
            ssid
        );
        return false;
    }
    let bss = scan
        .iter()
        .find(|b| b.ssid == ssid)
        .unwrap_or(&scan[0]);

    k_nano::slog_hal!(
        "ATH10K",
        "info",
        "step=assoc status=START eid={} ssid={} ch={}",
        wmi_eid,
        bss.ssid.as_str(),
        bss.channel
    );

    let mut frame = [0u8; 96];
    let payload_len: u16 = 52;
    frame[0] = wmi_eid;
    frame[2..4].copy_from_slice(&payload_len.to_le_bytes());
    frame[8..12].copy_from_slice(&WMI_TLV_VDEV_START_CMDID.to_le_bytes());
    frame[12..14].copy_from_slice(&WMI_TLV_TAG_ARRAY_BYTE.to_le_bytes());
    frame[14..16].copy_from_slice(&40u16.to_le_bytes());
    // BSSID
    frame[16..22].copy_from_slice(&bss.bssid);
    frame[22] = bss.channel;
    let sb = bss.ssid.as_bytes();
    let n = sb.len().min(32);
    frame[23] = n as u8;
    frame[24..24 + n].copy_from_slice(&sb[..n]);

    let mut resp = [0u8; 128];
    match ce.exchange(&frame[..8 + payload_len as usize], &mut resp) {
        Ok(rn) if rn > 0 => {
            // Honesty: ACK bytes ≠ prova de link 802.11. Note deve confirmar RF.
            // Em QEMU CE falha → Ok path raro. Grant só se scan_had_rf (SSID real).
            if ath10k_wmi_scan::scan_had_rf() {
                ASSOC_OK.store(true, core::sync::atomic::Ordering::Relaxed);
                crate::unlock_dag::grant(crate::unlock_dag::CapToken::WifiAssociated);
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=assoc status=OK ssid={} resp={} VERDICT=PASS reason=wmi_assoc_ack_rf",
                    bss.ssid.as_str(),
                    rn
                );
                true
            } else {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=assoc status=PARTIAL ssid={} VERDICT=AWAITING_REAL_HW reason=no_rf_scan",
                    bss.ssid.as_str()
                );
                false
            }
        }
        Ok(_) | Err(_) => {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "step=assoc status=FAIL ssid={} VERDICT=AWAITING_REAL_HW reason=wmi_exchange",
                bss.ssid.as_str()
            );
            false
        }
    }
}
