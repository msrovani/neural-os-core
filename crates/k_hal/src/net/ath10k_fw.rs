//! ath10k QCA6174 — DID→FAT + parse firmware-6 IE (FW_IMAGE). Sem claim ready.

use alloc::vec::Vec;

const ATH10K_MAGIC: &[u8] = b"QCA-ATH10K\0";
const IE_FW_VERSION: u32 = 0;
const IE_FW_IMAGE: u32 = 3;
const IE_OTP_IMAGE: u32 = 4;

#[derive(Clone, Copy)]
pub struct Ath10kFwSpec {
    pub did: u16,
    pub family: &'static str,
    pub fat_fw: &'static str,
    pub fat_board2: &'static str,
    pub fat_board: &'static str,
}

pub struct Ath10kBlobs {
    pub fw_file: Vec<u8>,
    pub fw_image: Vec<u8>,
    pub otp_image: Vec<u8>,
    pub board: Vec<u8>,
    pub fw_version: [u8; 32],
    pub fw_version_len: usize,
}

/// QCA6174 / QCA6164 (ath10k) — Note 1050 QCA61x4A ≈ 168C:003E.
pub fn resolve_ath10k_fw(did: u16) -> Option<Ath10kFwSpec> {
    let family = match did {
        0x003E => "QCA6174",
        0x0041 => "QCA6164",
        _ => return None,
    };
    Some(Ath10kFwSpec {
        did,
        family,
        fat_fw: "AT10K_F6.BIN",
        fat_board2: "AT10K_B2.BIN",
        fat_board: "AT10K_BD.BIN",
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

/// Parse `QCA-ATH10K` + IEs → FW_IMAGE / OTP (Linux ath10k_core_fetch_firmware_api_n).
pub fn parse_firmware_ies(file: &[u8]) -> Result<(Vec<u8>, Vec<u8>, [u8; 32], usize), &'static str> {
    let magic_len = ATH10K_MAGIC.len();
    if file.len() < magic_len || &file[..magic_len] != ATH10K_MAGIC {
        return Err("bad_magic");
    }
    // ALIGN(magic_len, 4) — Linux
    let mut off = (magic_len + 3) & !3;
    let mut fw_image = Vec::new();
    let mut otp = Vec::new();
    let mut ver = [0u8; 32];
    let mut ver_len = 0usize;

    while off + 8 <= file.len() {
        let id = u32::from_le_bytes(file[off..off + 4].try_into().unwrap());
        let len = u32::from_le_bytes(file[off + 4..off + 8].try_into().unwrap()) as usize;
        off += 8;
        if off + len > file.len() {
            return Err("ie_truncated");
        }
        let data = &file[off..off + len];
        match id {
            IE_FW_VERSION => {
                ver_len = core::cmp::min(len, 31);
                ver[..ver_len].copy_from_slice(&data[..ver_len]);
            }
            IE_FW_IMAGE => {
                fw_image = data.to_vec();
            }
            IE_OTP_IMAGE => {
                otp = data.to_vec();
            }
            _ => {}
        }
        // Linux: jump over padding — ALIGN(ie_len, 4)
        off += (len + 3) & !3;
    }
    if fw_image.is_empty() {
        return Err("no_fw_image");
    }
    Ok((fw_image, otp, ver, ver_len))
}

/// Carrega blobs FAT + parse IE. Logs FOUND/MISSING.
pub fn load_ath10k_blobs(spec: &Ath10kFwSpec) -> Result<Ath10kBlobs, &'static str> {
    k_nano::slog_hal!(
        "ATH10K",
        "info",
        "fw_resolve did={:04x} family={} fw={} board2={} board={}",
        spec.did,
        spec.family,
        spec.fat_fw,
        spec.fat_board2,
        spec.fat_board
    );

    let has_ata = unsafe { k_nano::ATA_DRIVER.lock().is_some() };
    if !has_ata {
        k_nano::slog_hal!("ATH10K", "info", "fw_fat status=NO_DISK");
        return Err("no_disk");
    }

    let fw_file = read_fat32_root(spec.fat_fw).ok_or("fw_missing")?;
    let board2 = read_fat32_root(spec.fat_board2);
    let board1 = read_fat32_root(spec.fat_board);
    let board = board2.or(board1).unwrap_or_default();

    k_nano::slog_hal!(
        "ATH10K",
        "info",
        "fw_fat fw=FOUND bytes={} board_bytes={}",
        fw_file.len(),
        board.len()
    );

    let (fw_image, otp_image, fw_version, fw_version_len) = parse_firmware_ies(&fw_file)?;
    k_nano::slog_hal!(
        "ATH10K",
        "info",
        "fw_ie image={} otp={} ver_len={}",
        fw_image.len(),
        otp_image.len(),
        fw_version_len
    );

    Ok(Ath10kBlobs {
        fw_file,
        fw_image,
        otp_image,
        board,
        fw_version,
        fw_version_len,
    })
}

/// Resolve + probe FAT (sem BMI). Usado no bind antes do A3.
pub fn try_probe_ath10k_fw_from_fat(spec: &Ath10kFwSpec) {
    match load_ath10k_blobs(spec) {
        Ok(b) => {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "fw_fat note=blobs_ok image={} (bmi_next)",
                b.fw_image.len()
            );
        }
        Err(e) => {
            k_nano::slog_hal!("ATH10K", "info", "fw_fat status=FAIL reason={}", e);
        }
    }
}

pub fn probe_ath10k_fw_for_did(did: u16) {
    match resolve_ath10k_fw(did) {
        Some(spec) => try_probe_ath10k_fw_from_fat(&spec),
        None => {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "fw_resolve did={:04x} status=UNSUPPORTED",
                did
            );
        }
    }
}

pub fn log_fw_resolve_skip_no_radio() {
    k_nano::slog_hal!("ATH10K", "info", "fw_resolve=SKIP reason=no_wifi_pci");
}
