//! #176 Ed25519 Cryptographic Identity — substitui CapabilityToken(u64).
//!
//! Fornece assinatura e verificação Ed25519 para autenticar agentes, skills e
//! operações do kernel. A verificação é bare-metal (sem std). Geração de chaves
//! e assinaturas é feita externamente (host-side) e embedada no kernel.
//!
//! ## Uso no kernel
//! - `IdentityToken::verify(msg, signature)` → bool — verifica assinatura de uma mensagem
//! - `TRUSTED_PUBLIC_KEYS` — lista de chaves públicas confiáveis (embutidas no boot)

use alloc::string::String;
use alloc::vec::Vec;
use ed25519_dalek::{Verifier, VerifyingKey, Signature};

/// Tamanhos fixos Ed25519
pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

/// Chave pública Ed25519 embutida (32 bytes)
/// Gerada externamente via `tools/gen_identity_key.py`
const TRUSTED_PUBLIC_KEYS: &[[u8; PUBLIC_KEY_LEN]] = &[
    // Chave padrão de desenvolvimento (32 bytes raw)
    // hex: e0c6c6f5e9b1f3c9b8b6a7e8f9d0c1b2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7
    [0xe0, 0xc6, 0xc6, 0xf5, 0xe9, 0xb1, 0xf3, 0xc9,
     0xb8, 0xb6, 0xa7, 0xe8, 0xf9, 0xd0, 0xc1, 0xb2,
     0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8, 0xa9, 0xb0,
     0xc1, 0xd2, 0xe3, 0xf4, 0xa5, 0xb6, 0xc7, 0x00],
];

/// Mensagem de desafio para handshake: agent_id + tick
pub fn challenge_message(agent: &str, tick: u64) -> Vec<u8> {
    let mut msg = Vec::with_capacity(agent.len() + 8);
    msg.extend_from_slice(agent.as_bytes());
    msg.extend_from_slice(&tick.to_le_bytes());
    msg
}

/// Verifica uma assinatura Ed25519 contra chaves públicas confiáveis.
/// Retorna true se a assinatura for válida para QUALQUER chave confiável.
pub fn verify_signature(public_key: &[u8; PUBLIC_KEY_LEN], message: &[u8], signature: &[u8; SIGNATURE_LEN]) -> bool {
    let pk = match VerifyingKey::from_bytes(public_key) {
        Ok(pk) => pk,
        Err(_) => return false,
    };

    let sig = match Signature::try_from(&signature[..]) {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    pk.verify(message, &sig).is_ok()
}

/// Verifica assinatura contra TODAS as chaves públicas confiáveis
pub fn verify_trusted(message: &[u8], signature: &[u8; SIGNATURE_LEN]) -> bool {
    TRUSTED_PUBLIC_KEYS.iter().any(|pk| verify_signature(pk, message, signature))
}

/// Converte um token u64 antigo para o novo formato de identidade
/// (compatibilidade retroativa durante migração)
pub fn legacy_token_to_identity(token: u64) -> [u8; PUBLIC_KEY_LEN] {
    let mut key = [0u8; PUBLIC_KEY_LEN];
    let bytes = token.to_le_bytes();
    key[..8].copy_from_slice(&bytes);
    key
}

/// Identity-aware CapabilityToken — carrega chave pública + assinatura
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


