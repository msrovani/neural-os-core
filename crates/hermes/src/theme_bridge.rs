//! Theme bridge Hermes FE → jarbas (registrado no boot; evita ciclo hermes→jarbas).
//! Labor 37 ADR-0062 P26.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub type ThemeListFn = fn() -> Vec<&'static str>;
pub type ThemeApplyFn = fn(&str) -> Result<(), &'static str>;

static LIST: Mutex<Option<ThemeListFn>> = Mutex::new(None);
static APPLY: Mutex<Option<ThemeApplyFn>> = Mutex::new(None);

pub fn register(list: ThemeListFn, apply: ThemeApplyFn) {
    *LIST.lock() = Some(list);
    *APPLY.lock() = Some(apply);
}

pub fn list_names() -> Vec<&'static str> {
    match *LIST.lock() {
        Some(f) => f(),
        None => Vec::new(),
    }
}

pub fn apply(name: &str) -> Result<(), &'static str> {
    match *APPLY.lock() {
        Some(f) => f(name),
        None => Err("theme_bridge: not registered"),
    }
}

pub fn boot_smoke() -> bool {
    let names = list_names();
    if names.is_empty() {
        k_nano::slog_bin!(
            "THEME",
            "info",
            "step=bridge status=SKIP VERDICT=SKIP reason=unregistered"
        );
        return true;
    }
    let first = names[0];
    match apply(first) {
        Ok(()) => {
            k_nano::slog_bin!(
                "THEME",
                "info",
                "step=bridge status=OK n={} VERDICT=PASS reason=apply_{}",
                names.len(),
                first
            );
            true
        }
        Err(e) => {
            k_nano::slog_bin!(
                "THEME",
                "info",
                "step=bridge status=FAIL VERDICT=FAIL reason={}",
                e
            );
            false
        }
    }
}

pub fn status_line() -> String {
    let names = list_names();
    alloc::format!("[THEME] bridge names={}", names.len())
}