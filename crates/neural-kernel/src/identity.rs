//! #176 Ed25519 Cryptographic Identity — CapabilityToken com assinatura.
//!
//! NOTA: A implementacao real depende de `curve25519-dalek` (incompativel com
//! x86_64-unknown-none devido ao backend SIMD). Enquanto isso, o modulo
//! mantem a estrutura do `IdentityToken` e `CapabilityToken` enum, com
//! verificacao via stub (sempre retorna true para tokens Ed25519).
//!
//! Para ativar: `cargo add ed25519-dalek --no-default-features` em Cargo.toml
//! e descomentar o codigo real em `verify_signature()`.

use alloc::string::String;
use alloc::vec::Vec;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

/// Chave publica padrao (stub)
const TRUSTED_PUBLIC_KEYS: &[[u8; PUBLIC_KEY_LEN]] = &[
    [0u8; 32], // placeholder
];

/// Mensagem de desafio para handshake
pub fn challenge_message(agent: &str, tick: u64) -> Vec<u8> {
    let mut msg = Vec::with_capacity(agent.len() + 8);
    msg.extend_from_slice(agent.as_bytes());
    msg.extend_from_slice(&tick.to_le_bytes());
    msg
}

/// Verifica assinatura — STUB: quando ed25519-dalek estiver disponivel,
/// substituir por verificacao real.
pub fn verify_signature(_public_key: &[u8; PUBLIC_KEY_LEN], _message: &[u8], _signature: &[u8; SIGNATURE_LEN]) -> bool {
    // TODO: usar ed25519-dalek::VerifyingKey::verify() quando o crate for compativel
    true
}

/// Verifica assinatura contra chaves confiaveis (stub)
pub fn verify_trusted(_message: &[u8], _signature: &[u8; SIGNATURE_LEN]) -> bool {
    true
}

/// IdentityToken struct mantida para compatibilidade
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

/// Converte token legado para chave (compatibilidade)
pub fn legacy_token_to_identity(token: u64) -> [u8; PUBLIC_KEY_LEN] {
    let mut key = [0u8; PUBLIC_KEY_LEN];
    let bytes = token.to_le_bytes();
    key[..8].copy_from_slice(&bytes);
    key
}
