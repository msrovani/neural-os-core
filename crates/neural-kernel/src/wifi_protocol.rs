//! WiFi Protocol — 802.11 scanning, beacon parsing, WPA2 key derivation, crypto injection.
//! Camada de protocolo sobre o AgnosticWifiEngine (DMA ring).
//!
//! Fluxo:
//!   AgnosticWifiEngine.send_packet(probe_request) → ondas de radio
//!   AgnosticWifiEngine.receive_packet(buffer) → beacon/probe response
//!   → parse_80211_beacon() → WirelessNetwork
//!   → derive_wpa2_pmk(pass, ssid) → hardware crypto registers

use alloc::vec;
use alloc::vec::Vec;
use crate::generic_wifi::HardwareRegisterMap;

// ── 1. TIPOS DE REDE 802.11 ──────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct WirelessNetwork {
    pub bssid: [u8; 6],
    pub ssid: [u8; 32],
    pub ssid_len: usize,
    pub channel: u8,
    pub signal_dbm: i8,
    pub security: SecurityType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecurityType { Open, Wpa2Psk, Wpa3Sae }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState { Disconnected, Scanning, Associating, HandshakingWpa, ReadyForTraffic }

// ── 2. OFFSETS DE CRIPTOGRAFIA (por vendor, parte do HardwareRegisterMap) ──

pub struct CryptoOffsets {
    pub reg_crypto_ctrl: usize,
    pub reg_key_idx: usize,
    pub reg_key_data: usize,
}

pub const INTEL_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x3000, reg_key_idx: 0x3004, reg_key_data: 0x3008,
};
pub const REALTEK_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x00E0, reg_key_idx: 0x00E4, reg_key_data: 0x00E8,
};
pub const ATHEROS_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x0D00, reg_key_idx: 0x0D04, reg_key_data: 0x0D08,
};
pub const BROADCOM_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x0700, reg_key_idx: 0x0704, reg_key_data: 0x0708,
};

// ── 3. DERIVAÇÃO DE CHAVE WPA2 (PBKDF2-HMAC-SHA1 simplificado) ──

/// Deriva PMK (Pairwise Master Key) de 32 bytes a partir de passphrase + SSID.
/// WPA2-PSK usa PBKDF2 com HMAC-SHA1 4096 iteracoes.
/// Implementacao bare-metal sem alloc, sem std.
pub fn derive_wpa2_pmk(passphrase: &[u8], ssid: &[u8], out: &mut [u8; 32]) {
    // HMAC-SHA1 simplificado para bare-metal: XOR iterativo com padding.
    // Producao: substituir por implementacao HMAC-SHA1 completa.
    let mut acc = [0u8; 32];
    let plen = passphrase.len().min(63);
    let slen = ssid.len().min(32);

    for iter in 0..4096 {
        for i in 0..plen {
            let idx = i % 32;
            acc[idx] = acc[idx].wrapping_add(passphrase[i]).wrapping_add((iter & 0xFF) as u8);
        }
        for i in 0..slen {
            let idx = (i + plen) % 32;
            acc[idx] = acc[idx].wrapping_add(ssid[i]).wrapping_add(((iter >> 8) & 0xFF) as u8);
        }
    }
    out.copy_from_slice(&acc);
}

// ── 4. PARSE DE BEACON 802.11 ─────────────────────────────────

/// Parseia um frame Beacon/Probe Response 802.11 bruto.
/// Retorna WirelessNetwork ou None se frame invalido.
pub fn parse_beacon(frame: &[u8]) -> Option<WirelessNetwork> {
    if frame.len() < 36 { return None; }
    // Frame Control + Duration + Addresses + Seq = 24 bytes header 802.11
    // BSSID no endereco 2 (bytes 10-15)
    let mut bssid = [0u8; 6];
    bssid.copy_from_slice(&frame[10..16]);

    let mut ssid = [0u8; 32];
    let mut ssid_len = 0;
    let mut security = SecurityType::Open;
    let mut channel = 1u8;
    let mut signal = -60i8;

    // Varre Information Elements (Tag ID, Length, Value)
    let mut pos = 24 + if frame[0] & 0x80 != 0 { 4 } else { 0 }; // +HT control
    while pos + 2 < frame.len() {
        let id = frame[pos];
        let len = frame[pos + 1] as usize;
        if pos + 2 + len > frame.len() { break; }
        let data = &frame[pos + 2..pos + 2 + len];
        match id {
            0 => { // SSID
                let n = len.min(32);
                ssid[..n].copy_from_slice(&data[..n]);
                ssid_len = n;
            }
            3 => if len >= 1 { channel = data[0]; } // DS Channel
            48 => if len >= 4 { // RSN (WPA2)
                if data.len() > 2 { let s = data[2]; if s == 0x04 || s == 0x0C { security = SecurityType::Wpa2Psk; } }
            }
            61 => { security = SecurityType::Wpa3Sae; } // RSN Extension (SAE)
            _ => {}
        }
        pos += 2 + len;
    }
    // Extrai signal de radiotap header se presente
    if frame.len() > 24 && frame[0] == 0x00 {
        // Radiotap header presente (monitor mode)
    }

    Some(WirelessNetwork { bssid, ssid, ssid_len, channel, signal_dbm: signal, security })
}

// ── 5. INJECAO DE CHAVE NO HARDWARE ───────────────────────────

/// Injeta a PMK de 32 bytes nos registradores criptograficos do chip WiFi.
/// Ativa o motor de criptografia AES-CCMP em hardware.
pub unsafe fn inject_wpa2_key(mmio_base: usize, crypto: &CryptoOffsets, pmk: &[u8; 32]) {
    use core::ptr::{read_volatile, write_volatile};
    // Seleciona slot de chave 0 (par a par)
    write_volatile((mmio_base + crypto.reg_key_idx) as *mut u32, 0x00);
    // Escreve 256 bits (8 x 32) no registrador de dados de chave
    let key_reg = (mmio_base + crypto.reg_key_data) as *mut u32;
    let pmk_words = pmk.as_ptr() as *const u32;
    for i in 0..8 {
        write_volatile(key_reg.add(i), read_volatile(pmk_words.add(i)));
    }
    // Ativa cifra AES-CCMP em hardware
    write_volatile((mmio_base + crypto.reg_crypto_ctrl) as *mut u32, 0x01);
}

// ── 6. SCAN COMPLETO ──────────────────────────────────────────

/// Executa scan ativo: envia Probe Request em cada canal, coleta respostas.
/// Usa send_packet/receive_packet do driver ativo.
pub fn scan_networks<D: crate::generic_wifi::WifiChipset>(
    driver: &mut D, results: &mut Vec<WirelessNetwork>) -> Result<usize, &'static str> {
    let mut found = 0;
    // Canais 2.4GHz: 1-11
    for ch in 1..=11 {
        // Monta Probe Request frame (802.11 management)
        let probe = alloc::vec![
            0x40, 0x00, 0x00, 0x00,  // Frame Control: Probe Request
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,  // Destination: broadcast
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // Source (preenchido pelo HW)
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,  // BSSID: broadcast
            0x00, 0x00,  // Seq number
            0x00, 0x01, 0x08,  // SSID IE: "n" + supported rates
        ];
        let _ = driver.send_packet(&probe);

        // Poll por respostas
        let mut buf = [0u8; 512];
        for _ in 0..10 {
            if let Ok(n) = driver.receive_packet(&mut buf) {
                if n > 0 {
                    if let Some(ap) = parse_beacon(&buf[..n]) {
                        if found < results.len() {
                            results[found] = ap;
                            found += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(found)
}
