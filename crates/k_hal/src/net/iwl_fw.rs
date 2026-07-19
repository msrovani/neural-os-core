//! Prep S1 iwlwifi — DID→blob FAT + resolve (sem claim ALIVE).
//! Fingerprints: short names mkfat32 (`FW_CC77.BIN` …). pnvm = None até refresh.

use alloc::vec::Vec;

/// Spec resolvida: ucode no FAT + pnvm opcional (ausente no repo).
#[derive(Clone, Copy)]
pub struct IwlFwSpec {
    pub did: u16,
    pub family: &'static str,
    pub fat_ucode: &'static str,
    pub fat_pnvm: Option<&'static str>,
}

/// DIDs alinhados a `generic_wifi::runtime_probe_and_bind` (Intel) + inventário API77.
pub fn resolve_iwl_fw(did: u16) -> Option<IwlFwSpec> {
    let (family, fat_ucode) = match did {
        // AX200 (cc-a0)
        0x2723 | 0x08B1 | 0x08B2 => ("AX200", "FW_CC77.BIN"),
        // AX201 (so-a0-gf)
        0x06F0 | 0x02F0 | 0xA0F0 => ("AX201", "FW_SOGF.BIN"),
        // AX210 (so-a0-hr)
        0x2725 | 0x24F3 | 0x24F4 | 0x24F5 | 0x24F6 | 0x24FD => ("AX210", "FW_SOHR.BIN"),
        // AX211 (ty-a0-gf)
        0x2726 | 0x2526 | 0x2527 => ("AX211", "FW_TYGF.BIN"),
        // AX101 (Qu-b0-hr)
        0x3165 | 0x3166 => ("AX101", "FW_QUHR.BIN"),
        _ => return None,
    };
    Some(IwlFwSpec {
        did,
        family,
        fat_ucode,
        fat_pnvm: None,
    })
}

fn read_fat32_root(name: &str) -> Option<Vec<u8>> {
    unsafe {
        let ata = k_nano::ATA_DRIVER.lock();
        let ata = ata.as_ref()?;
        let parts = k_nano::fat32::read_mbr(ata);
        for p in &parts {
            if !matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0x73) {
                continue;
            }
            if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                if let Some(data) = fs.read_file(name) {
                    return Some(data);
                }
            }
        }
    }
    None
}

/// Validação mínima de header (sem MMIO / sem ALIVE).
fn check_ucode_header(blob: &[u8]) -> bool {
    if blob.len() < 12 {
        return false;
    }
    let count = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
    let total = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
    count > 0 && total > 0 && (total as usize) <= blob.len()
}

/// Tenta ler ucode do FAT. Nunca chama load_ucode / nunca claim alive.
pub fn try_load_iwl_fw_from_fat(spec: &IwlFwSpec) -> Result<Vec<u8>, &'static str> {
    let pnvm = spec.fat_pnvm.unwrap_or("none");
    k_nano::slog_hal!(
        "IWL",
        "info",
        "fw_resolve did={:04x} family={} blob={} pnvm={}",
        spec.did,
        spec.family,
        spec.fat_ucode,
        pnvm
    );

    // Disco ausente (boot cedo / sem ATA).
    let has_ata = unsafe { k_nano::ATA_DRIVER.lock().is_some() };
    if !has_ata {
        k_nano::slog_hal!("IWL", "info", "fw_fat status=NO_DISK bytes=0");
        return Err("no_disk");
    }

    match read_fat32_root(spec.fat_ucode) {
        Some(data) if !data.is_empty() => {
            let hdr = if check_ucode_header(&data) {
                "OK"
            } else {
                "BAD"
            };
            k_nano::slog_hal!(
                "IWL",
                "info",
                "fw_fat status=FOUND bytes={} fw_header={}",
                data.len(),
                hdr
            );
            if hdr == "BAD" {
                return Err("bad_header");
            }
            Ok(data)
        }
        _ => {
            k_nano::slog_hal!("IWL", "info", "fw_fat status=MISSING bytes=0");
            Err("missing")
        }
    }
}

/// Probe Intel: resolve + FAT. Loga SKIP/UNSUPPORTED; sem ALIVE.
pub fn probe_iwl_fw_for_did(did: u16) {
    match resolve_iwl_fw(did) {
        Some(spec) => {
            let _ = try_load_iwl_fw_from_fat(&spec);
        }
        None => {
            k_nano::slog_hal!(
                "IWL",
                "info",
                "fw_resolve did={:04x} status=UNSUPPORTED",
                did
            );
        }
    }
}

/// QEMU / sem radio 02:80 — um log, sem spam.
pub fn log_fw_resolve_skip_no_radio() {
    k_nano::slog_hal!("IWL", "info", "fw_resolve=SKIP reason=no_wifi_pci");
}
