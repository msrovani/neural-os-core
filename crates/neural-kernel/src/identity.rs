use alloc::string::String;
use alloc::vec::Vec;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

const TRUSTED_PUBLIC_KEYS: &[[u8; PUBLIC_KEY_LEN]] = &[
    [0u8; 32],
];

pub fn challenge_message(agent: &str, tick: u64) -> Vec<u8> {
    let mut msg = Vec::with_capacity(agent.len() + 8);
    msg.extend_from_slice(agent.as_bytes());
    msg.extend_from_slice(&tick.to_le_bytes());
    msg
}

pub fn verify_signature(public_key: &[u8; PUBLIC_KEY_LEN], message: &[u8], signature: &[u8; SIGNATURE_LEN]) -> bool {
    use ed25519_compact::*;
    let pk = match PublicKey::from_slice(public_key) { Ok(p) => p, _ => return false };
    let sig = match Signature::from_slice(signature) { Ok(s) => s, _ => return false };
    pk.verify(message, &sig).is_ok()
}

pub fn verify_trusted(message: &[u8], signature: &[u8; SIGNATURE_LEN]) -> bool {
    for key in TRUSTED_PUBLIC_KEYS {
        if verify_signature(key, message, signature) { return true; }
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

pub fn legacy_token_to_identity(token: u64) -> [u8; PUBLIC_KEY_LEN] {
    let mut key = [0u8; PUBLIC_KEY_LEN];
    let bytes = token.to_le_bytes();
    key[..8].copy_from_slice(&bytes);
    key
}
