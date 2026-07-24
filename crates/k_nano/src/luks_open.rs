//! LUKS open MVP — passphrase HITL (Labor 52). Detect já em vol_mgr; open = escalate.

use alloc::string::String;
use spin::Mutex;

static LAST: Mutex<Option<String>> = Mutex::new(None);

/// Solicita open — **não** decripta inventado. HITL escalate.
pub fn request_open(device: &str, _passphrase: &str) -> Result<(), &'static str> {
    if device.is_empty() {
        return Err("empty_device");
    }
    *LAST.lock() = Some(String::from(device));
    crate::slog_nano!(
        "LUKS",
        "info",
        "step=open status=ESCALATE VERDICT=PARTIAL reason=hitl_passphrase device={}",
        device
    );
    Err("hitl_required")
}

pub fn boot_smoke() {
    let _ = request_open("nvme0p3", "demo");
    crate::slog_nano!(
        "LUKS",
        "info",
        "step=smoke status=OK VERDICT=PARTIAL reason=open_api_no_decrypt"
    );
}