//! Primitivas cripto do mesh P2P (ADR-0081) — Tier Relativizado (HMAC-SHA256).
//!
//! ## Modelo de confiança
//! O mesh autentica pacotes em dois tiers:
//! - **Tier Full (default, fail-closed)**: Ed25519 em todos os caminhos —
//!   assinatura de sessão de 64B, prova de posse de chave. Custo de
//!   verificação ~26-46µs/pacote @1.2KB (aritmética de curva edwards25519).
//! - **Tier Relativizado**: a mesma chave de segmento (`SEGMENT_KEY` em
//!   `net::mesh`) é provisionada em todos os nós do mesmo range/datacenter →
//!   DADOS autenticados por HMAC-SHA256 (tag 32B, ~1.3µs/pacote @1.2KB —
//!   só SHA-256 no buffer, sem curva elíptica). O caminho de CONTROLE
//!   (heartbeat/ROLE/TOFU) continua Ed25519 sempre: é ele que estabelece/
//!   renova confiança e é raro (~1.1s no heartbeat).
//!
//! Fail-closed: sem `SEGMENT_KEY` provisionada = Tier Full = comportamento
//! atual. A tag HMAC é 32B menor que a sig Ed25519 — o mesmo frame cabe mais
//! payload no caminho direto (FRAG_DIRECT_MAX).
//!
//! ## HMAC-SHA256 (RFC 2104 / RFC 4231)
//! Bloco de 64B; ipad 0x36 / opad 0x5c; se a chave > 64B, hash antes.
//! Reusa `crate::tpm::sha256` — sem dependência nova.

use alloc::vec::Vec;

/// Tamanho da tag HMAC-SHA256 (bytes) anexada no Tier Relativizado.
pub const HMAC_TAG_LEN: usize = 32;
/// Tamanho do bloco do SHA-256 (RFC 2104).
const HMAC_BLOCK_LEN: usize = 64;

/// HMAC-SHA256 (RFC 2104/4231). `key` ≤ 64B usada direto; `key` > 64B → hash
/// antes (comportamento padrão HMAC). Retorna a tag de 32B.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; HMAC_BLOCK_LEN];
    if key.len() > HMAC_BLOCK_LEN {
        let h = crate::tpm::sha256(key);
        k[..32].copy_from_slice(&h);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0u8; HMAC_BLOCK_LEN];
    let mut opad = [0u8; HMAC_BLOCK_LEN];
    for i in 0..HMAC_BLOCK_LEN {
        ipad[i] = k[i] ^ 0x36;
        opad[i] = k[i] ^ 0x5c;
    }
    let mut inner = Vec::with_capacity(HMAC_BLOCK_LEN + data.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(data);
    let inner_hash = crate::tpm::sha256(&inner);
    let mut outer = Vec::with_capacity(HMAC_BLOCK_LEN + 32);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    crate::tpm::sha256(&outer)
}

/// Comparação constant-time: XOR acumulado, sem early-exit. Retorna false se
/// os tamanhos diferirem (comprimento não é segredo — o conteúdo é).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Self-test HMAC-SHA256 — vetor RFC 4231 caso 1:
/// key = 0x0b×20, msg = "Hi There" → b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7.
pub fn hmac_self_test() -> bool {
    let key = [0x0bu8; 20];
    let data = b"Hi There";
    let expect: [u8; 32] = [
        0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53,
        0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b, 0xf1, 0x2b,
        0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7,
        0x26, 0xe9, 0x37, 0x6c, 0x2e, 0x32, 0xcf, 0xf7,
    ];
    let got = hmac_sha256(&key, data);
    got == expect
}
