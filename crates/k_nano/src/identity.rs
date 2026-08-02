//! Identity Ed25519 — trusted keys + session keypair (sign artifacts / audit).
//! Wave 0 HANR: Hermes assina drafts; verify_trusted aceita trusted OU session PK.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

const TRUSTED_PUBLIC_KEYS: &[[u8; PUBLIC_KEY_LEN]] = &[
    [
        0xC7, 0x4C, 0xF7, 0xA7, 0xA9, 0x3F, 0xDE, 0xD6, 0xCF, 0x79, 0x6A, 0x04, 0x3A, 0xC7, 0x29,
        0x06, 0x90, 0x60, 0xD0, 0x6B, 0x39, 0x38, 0x29, 0x0D, 0x59, 0x40, 0xD6, 0xD5, 0xF7, 0x03,
        0x8D, 0x24,
    ],
];

static SESSION_READY: AtomicBool = AtomicBool::new(false);
static SESSION_SK: Mutex<Option<[u8; 32]>> = Mutex::new(None);
static SESSION_PK: Mutex<[u8; PUBLIC_KEY_LEN]> = Mutex::new([0u8; PUBLIC_KEY_LEN]);

pub fn challenge_message(agent: &str, tick: u64) -> Vec<u8> {
    let mut msg = Vec::with_capacity(agent.len() + 8);
    msg.extend_from_slice(agent.as_bytes());
    msg.extend_from_slice(&tick.to_le_bytes());
    msg
}

pub fn verify_signature(
    public_key: &[u8; PUBLIC_KEY_LEN],
    message: &[u8],
    signature: &[u8; SIGNATURE_LEN],
) -> bool {
    use ed25519_compact::*;
    let pk = match PublicKey::from_slice(public_key) {
        Ok(p) => p,
        _ => return false,
    };
    let sig = match Signature::from_slice(signature) {
        Ok(s) => s,
        _ => return false,
    };
    pk.verify(message, &sig).is_ok()
}

/// Aceita chave trusted embutida **ou** public key da sessão boot.
pub fn verify_trusted(message: &[u8], signature: &[u8; SIGNATURE_LEN]) -> bool {
    for key in TRUSTED_PUBLIC_KEYS {
        if verify_signature(key, message, signature) {
            return true;
        }
    }
    if SESSION_READY.load(Ordering::Acquire) {
        let pk = *SESSION_PK.lock();
        if pk != [0u8; PUBLIC_KEY_LEN] && verify_signature(&pk, message, signature) {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone)]
pub struct IdentityToken {
    pub public_key: [u8; PUBLIC_KEY_LEN],
    pub signature: [u8; SIGNATURE_LEN],
    pub agent_name: String,
    pub tick: u64,
}

impl IdentityToken {
    pub fn verify(&self) -> bool {
        let msg = challenge_message(&self.agent_name, self.tick);
        verify_signature(&self.public_key, &msg, &self.signature)
    }
}

pub fn verify_kernel_signature(kernel_data: &[u8]) -> bool {
    if kernel_data.len() < SIGNATURE_LEN {
        return false;
    }
    let data_len = kernel_data.len() - SIGNATURE_LEN;
    let data = &kernel_data[..data_len];
    let sig_bytes = &kernel_data[data_len..];
    let mut sig = [0u8; SIGNATURE_LEN];
    sig.copy_from_slice(sig_bytes);
    verify_trusted(data, &sig)
}

pub fn legacy_token_to_identity(token: u64) -> [u8; PUBLIC_KEY_LEN] {
    let mut key = [0u8; PUBLIC_KEY_LEN];
    let bytes = token.to_le_bytes();
    key[..8].copy_from_slice(&bytes);
    key
}

/// Entropia bare-metal (RDTSC + PIT ticks) — sem getrandom.
fn mix_session_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    let tsc = unsafe { core::arch::x86_64::_rdtsc() };
    let ticks = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let mix = tsc
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(ticks.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    for i in 0..4 {
        let v = mix
            .wrapping_mul(0x85EB_CA6B + i as u64)
            .wrapping_add(tsc.rotate_left((i as u32) * 7));
        seed[i * 8..(i + 1) * 8].copy_from_slice(&v.to_le_bytes());
    }
    // Segunda passagem com RDTSC fresco
    let tsc2 = unsafe { core::arch::x86_64::_rdtsc() };
    for i in 0..32 {
        seed[i] ^= ((tsc2 >> (i % 8)) as u8).wrapping_add(i as u8);
    }
    seed
}

fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// Gera keypair de sessão uma vez. Idempotente. Nunca loga secret.
pub fn init_session_identity() {
    if SESSION_READY.swap(true, Ordering::AcqRel) {
        return;
    }
    use ed25519_compact::{KeyPair, Seed};
    let seed_bytes = mix_session_seed();
    let kp = KeyPair::from_seed(Seed::new(seed_bytes));
    let mut pk = [0u8; PUBLIC_KEY_LEN];
    pk.copy_from_slice(kp.pk.as_ref());
    let mut sk = [0u8; 32];
    // SecretKey seed material — ed25519-compact SecretKey is 64 bytes (seed||pk).
    // Keep only first 32 (seed) for KeyPair::from_seed rebuild on sign.
    sk.copy_from_slice(&seed_bytes);
    *SESSION_SK.lock() = Some(sk);
    *SESSION_PK.lock() = pk;
    crate::slog_nano!("Trust", "info", "session_pk={}", hex32(&pk));
}

pub fn session_public_key() -> Option<[u8; PUBLIC_KEY_LEN]> {
    if !SESSION_READY.load(Ordering::Acquire) {
        return None;
    }
    let pk = *SESSION_PK.lock();
    if pk == [0u8; PUBLIC_KEY_LEN] {
        None
    } else {
        Some(pk)
    }
}

pub fn session_ready() -> bool {
    SESSION_READY.load(Ordering::Acquire)
}

/// Seed da sessão (32B) — usada pelo `crypto::aead_key_for` para derivar a
/// sk X25519 local (X25519 DH com o peer). pub(crate): só dentro do k_nano.
pub(crate) fn session_seed() -> Option<[u8; 32]> {
    *SESSION_SK.lock()
}

/// Assina com a chave de sessão. Retorna None se sessão não inicializada.
pub fn sign_session(message: &[u8]) -> Option<[u8; SIGNATURE_LEN]> {
    use ed25519_compact::{KeyPair, Seed};
    let seed = *SESSION_SK.lock().as_ref()?;
    let kp = KeyPair::from_seed(Seed::new(seed));
    let sig = kp.sk.sign(message, None);
    let mut out = [0u8; SIGNATURE_LEN];
    out.copy_from_slice(sig.as_ref());
    Some(out)
}

pub fn hex_signature(sig: &[u8; SIGNATURE_LEN]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(128);
    for b in sig {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}
