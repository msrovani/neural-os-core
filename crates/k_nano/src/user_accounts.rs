//! User accounts locais SHA-256 (Labor 45). Extende multi_user com passphrase hash.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

struct Acct {
    name: String,
    /// SHA-256(passphrase) hex-ish first 16 bytes stored as raw.
    pass_hash: [u8; 32],
}

static ACCTS: Mutex<Vec<Acct>> = Mutex::new(Vec::new());

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    crate::tpm::sha256(data)
}

pub fn add_user(name: &str, passphrase: &str) -> bool {
    if name.is_empty() || passphrase.is_empty() {
        return false;
    }
    let h = sha256_bytes(passphrase.as_bytes());
    let mut g = ACCTS.lock();
    if g.iter().any(|a| a.name == name) {
        return false;
    }
    g.push(Acct {
        name: String::from(name),
        pass_hash: h,
    });
    true
}

pub fn auth(name: &str, passphrase: &str) -> bool {
    let h = sha256_bytes(passphrase.as_bytes());
    let g = ACCTS.lock();
    g.iter().any(|a| a.name == name && a.pass_hash == h)
}

pub fn boot_smoke() -> bool {
    let _ = add_user("lab", "neural-pass");
    let ok = auth("lab", "neural-pass") && !auth("lab", "wrong");
    crate::slog_nano!(
        "USERS",
        "info",
        "step=sha256_acct status={} VERDICT={}",
        if ok { "OK" } else { "FAIL" },
        if ok { "PASS" } else { "FAIL" }
    );
    ok
}