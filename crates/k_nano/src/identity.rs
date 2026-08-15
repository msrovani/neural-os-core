//! Identity Ed25519 — trusted keys + session keypair (sign artifacts / audit).
//! Wave 0 HANR: Hermes assina drafts; verify_trusted aceita trusted OU session PK.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

const TRUSTED_PUBLIC_KEYS: &[[u8; PUBLIC_KEY_LEN]] = &[
    // Chave release original (HANR/identity). A metade privada não está no repo.
    [
        0xC7, 0x4C, 0xF7, 0xA7, 0xA9, 0x3F, 0xDE, 0xD6, 0xCF, 0x79, 0x6A, 0x04, 0x3A, 0xC7, 0x29,
        0x06, 0x90, 0x60, 0xD0, 0x6B, 0x39, 0x38, 0x29, 0x0D, 0x59, 0x40, 0xD6, 0xD5, 0xF7, 0x03,
        0x8D, 0x24,
    ],
    // OTA release key (auditoria #7, SESSION_258): assina o digest sha256 do
    // KERNEL.BIN no manifest. Gerada com tools/ota_sign.py; a SEED vive no
    // host do dono (target/ota_release_seed.hex local, NUNCA commitada).
    // Rotação: gerar novo par, adicionar a pubkey aqui, assinar com a nova seed.
    [
        0x4c, 0xcd, 0xf9, 0xbb, 0x9b, 0x66, 0x4d, 0xaa, 0x0b, 0x29, 0x4d, 0xfb, 0xb6, 0xde, 0x74,
        0x63, 0x6e, 0x1e, 0xe7, 0x9f, 0x4b, 0x5d, 0x29, 0xab, 0x2a, 0xca, 0xcc, 0x15, 0x05, 0x6f,
        0x9e, 0x2e,
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

/// Auditoria #7: verifica a assinatura Ed25519 do UPDATE (manifest `sig`).
/// A assinatura cobre o DIGEST sha256 do blob (`digest`), não o blob inteiro —
/// o servidor assina `sha256(kernel)`, o kernel confere sha256 E assinatura.
/// Aceita SOMENTE as chaves trusted pinadas (NUNCA a session pk — update é
/// assinado pela release key do dono, não por uma identidade de boot local).
pub fn verify_update_signature(digest: &[u8; 32], signature: &[u8; SIGNATURE_LEN]) -> bool {
    for key in TRUSTED_PUBLIC_KEYS {
        if verify_signature(key, digest, signature) {
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

/// Entropia bare-metal: RDRAND (via `hw_rng`, com fallback ChaCha20) quando o
/// probe de plataforma confirma suporte; RDTSC + PIT ticks como stir adicional.
/// Auditoria 6.4: RDTSC/ticks puros são previsíveis no boot — RDRAND é a fonte
/// primária; o stir determinístico não reduz a entropia do RDRAND (bijection).
fn mix_session_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    // ADR-0082 gate: só executa RDRAND se o probe já rodou e confirmou suporte
    // (antes do probe, cpu_features() devolve default com rdrand=false).
    if crate::platform_probe::probe_done() && crate::platform_probe::cpu_features().rdrand {
        let _ = crate::hw_rng::HardwareRandom::fill_bytes(&mut seed);
    }
    // Stir adicional: RDTSC + ticks (não é fonte única de entropia).
    let tsc = unsafe { core::arch::x86_64::_rdtsc() };
    let ticks = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    for i in 0..32 {
        seed[i] ^= ((tsc >> (i % 8)) as u8)
            .wrapping_add((ticks >> (i % 8)) as u8)
            .wrapping_add(i as u8);
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_compact::{KeyPair, Seed};

    // Vetor 1 do RFC 8032 (msg vazia) — prova que o caminho sign→verify do
    // ed25519_compact (usado pelo OTA) interopera com o padrão.
    fn rfc8032_seed() -> [u8; 32] {
        [
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
            0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
        ]
    }

    fn rfc8032_pk() -> [u8; 32] {
        [
            0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
            0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
        ]
    }

    #[test]
    fn verify_signature_rfc8032_vector() {
        let seed = Seed::new(rfc8032_seed());
        let kp = KeyPair::from_seed(seed);
        let pk: [u8; 32] = kp.pk.as_ref().try_into().unwrap();
        assert_eq!(pk, rfc8032_pk(), "pubkey deve bater com o vetor RFC 8032");
        let sig = kp.sk.sign(b"", None);
        let sig_bytes: [u8; 64] = sig.as_ref().try_into().unwrap();
        assert!(verify_signature(&pk, b"", &sig_bytes));
        assert!(!verify_signature(&pk, b"tamper", &sig_bytes));
        assert!(!verify_signature(&[0u8; 32], b"", &sig_bytes));
    }

    #[test]
    fn verify_update_signature_rejects_garbage() {
        let digest = [7u8; 32];
        let mut sig = [0u8; 64];
        assert!(!verify_update_signature(&digest, &sig), "sig de zeros deve falhar");
        sig[0] = 1;
        sig[63] = 2;
        assert!(!verify_update_signature(&digest, &sig));
    }

    #[test]
    fn verify_update_signature_accepts_pinned_key() {
        // Só roda se a chave release pinada corresponder a uma seed conhecida
        // (dev machine). O kernel no_std valida em runtime; este teste prova o
        // mecanismo sign(sha256) → verify_update_signature end-to-end.
        let seed_hex = env_seed_hex();
        if seed_hex.is_empty() {
            return;
        }
        let mut seed_bytes = [0u8; 32];
        for (i, b) in seed_hex.as_bytes().chunks(2).enumerate() {
            if i >= 32 {
                break;
            }
            seed_bytes[i] = (hex_val(b[0]) << 4) | hex_val(b[1]);
        }
        let kp = KeyPair::from_seed(Seed::new(seed_bytes));
        let digest = crate::tpm::sha256(b"kernel-blob-test");
        let sig = kp.sk.sign(&digest, None);
        let sig_bytes: [u8; 64] = sig.as_ref().try_into().unwrap();
        assert!(verify_update_signature(&digest, &sig_bytes), "release key deve assinar");
        let other = crate::tpm::sha256(b"kernel-blob-tampered");
        assert!(!verify_update_signature(&other, &sig_bytes));
    }

    fn hex_val(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }

    /// Seed da release key via env (NUNCA embutida no código/tests).
    /// `OTA_RELEASE_SEED` — ex.: set em dev: `export OTA_RELEASE_SEED=$(cat target/ota_release_seed.hex)`
    fn env_seed_hex() -> String {
        core::option_env!("OTA_RELEASE_SEED").unwrap_or("").to_string()
    }
}
