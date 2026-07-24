//! WPA2 4-way handshake MVP — Labor 30 / ADR-0066.
//! Reuse: derive_wpa2_pmk + wifi_crypto inject. Sem inventar Connected IP.

use core::sync::atomic::{AtomicU8, Ordering};

use crate::wifi_protocol::{derive_wpa2_pmk, ConnectionState};

static HS: AtomicU8 = AtomicU8::new(0);

#[repr(u8)]
enum HsPhase {
    Idle = 0,
    PmkReady = 1,
    Msg1 = 2,
    Msg2 = 3,
    Msg3 = 4,
    Msg4Done = 5,
    Failed = 6,
}

/// Deriva PMK e arma handshake (passphrase ASCII + SSID).
pub fn start_psk(passphrase: &str, ssid: &str) -> bool {
    if passphrase.is_empty() || ssid.is_empty() {
        HS.store(HsPhase::Failed as u8, Ordering::Relaxed);
        return false;
    }
    let mut pmk = [0u8; 32];
    derive_wpa2_pmk(passphrase.as_bytes(), ssid.as_bytes(), &mut pmk);
    // Guardar PMK em static (32 B) — inject quando SoftMAC/assoc
    *PMK.lock() = Some(pmk);
    HS.store(HsPhase::PmkReady as u8, Ordering::Relaxed);
    k_nano::slog_bin!(
        "WIFI-HW",
        "info",
        "step=wpa2 status=OK phase=pmk ssid_len={} VERDICT=PARTIAL reason=pmk_derived",
        ssid.len()
    );
    true
}

use spin::Mutex;
static PMK: Mutex<Option<[u8; 32]>> = Mutex::new(None);

/// Avança 4-way best-effort (sem EAPOL real do ar = simula Msg1–4 em lab).
pub fn tick_handshake_demo() -> ConnectionState {
    let phase = HS.load(Ordering::Relaxed);
    match phase {
        x if x == HsPhase::PmkReady as u8 => {
            HS.store(HsPhase::Msg1 as u8, Ordering::Relaxed);
            ConnectionState::HandshakingWpa
        }
        x if x == HsPhase::Msg1 as u8 => {
            HS.store(HsPhase::Msg2 as u8, Ordering::Relaxed);
            ConnectionState::HandshakingWpa
        }
        x if x == HsPhase::Msg2 as u8 => {
            HS.store(HsPhase::Msg3 as u8, Ordering::Relaxed);
            ConnectionState::HandshakingWpa
        }
        x if x == HsPhase::Msg3 as u8 => {
            if let Some(_pmk) = *PMK.lock() {
                // Inject real só com BAR MMIO vivo (Note); demo não escreve phys 0.
                k_nano::slog_bin!(
                    "WIFI-HW",
                    "info",
                    "step=wpa2 status=OK phase=msg4 VERDICT=PARTIAL reason=hs_demo_no_mmio (inject on Note BAR)"
                );
            }
            HS.store(HsPhase::Msg4Done as u8, Ordering::Relaxed);
            ConnectionState::ReadyForTraffic
        }
        x if x == HsPhase::Msg4Done as u8 => ConnectionState::ReadyForTraffic,
        _ => ConnectionState::Disconnected,
    }
}

pub fn boot_smoke() {
    if start_psk("neural-lab-passphrase", "NeuralOS") {
        let _ = tick_handshake_demo();
        let _ = tick_handshake_demo();
        let _ = tick_handshake_demo();
        let st = tick_handshake_demo();
        let _ = st;
    }
}
