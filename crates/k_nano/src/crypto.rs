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
//!
//! ## Tier F (ADR-0081): AEAD X25519 + ChaCha20-Poly1305
//! Dados ponto-a-ponto (MR\0, EDR\0) no Tier Full passam a ser **selados**
//! (confidencialidade + autenticação): chave derivada por par de nós via
//! X25519 DH (pk X25519 do peer a partir da pk Ed25519 do TOFU) com KDF
//! `sha256(DH)`. Nonce de 12B = `source_id` u32 BE ‖ `clock` u64 BE (do
//! header NoProto — não vai no wire; reuso impossível: clock monotônico
//! estrito por fonte + anti-replay `clk > last` no RX). Layout wire:
//! `header NoProto (36B) ‖ ct ‖ tag16` — a tag Poly1305 vai anexada no
//! fim do ciphertext pela própria `Aead::encrypt`.
//!
//! Broadcasts (MW\0, ED\0, FD\0, FM\0, CRDT\0, SKILL\0, PROMOTE\0, offer)
//! NÃO têm receptor único → não há pk de destino para derivar chave → seguem
//! o caminho assinado (Ed25519/HMAC) — fail-closed, sem regressão.

use alloc::vec::Vec;
use spin::Mutex;

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

// ══════════════════════════════════════════════════════════════════════════
// Tier F (ADR-0081): AEAD X25519 + ChaCha20-Poly1305 — dados ponto-a-ponto
// ══════════════════════════════════════════════════════════════════════════
// Confidencialidade + autenticação nos DADOS direcionados (MR\0, EDR\0) no
// Tier Full: a chave é derivada POR PAR de nós via X25519 DH entre a sk X25519
// local (derivada da seed da sessão) e a pk X25519 do peer (derivada da pk
// Ed25519 vinculada no TOFU). KDF = sha256(DH). Não há handshake no wire — a
// mesma chave cai dos dois lados (DH simétrico).
//
// Wire layout: `header NoProto (PACKET_HEADER_SIZE B) ‖ ct ‖ tag16` — a tag
// Poly1305 vai anexada no fim do ciphertext pela própria `Aead::encrypt`.
// Nonce de 12B = `source_id` u32 BE ‖ `clock` u64 BE, ambos do header (NÃO vão
// no wire: o receptor reconstrói do header). Reuso impossível: `clock` é
// monotônico estrito por fonte (next_data_clock) e o anti-replay `clk > last`
// roda ANTES do decrypt no RX.
//
// Broadcasts (MW\0, ED\0, FD\0, FM\0, CRDT\0, SKILL\0, PROMOTE\0, offer) não
// têm receptor único → não há pk de destino para derivar chave → permanecem no
// caminho assinado (Ed25519/HMAC) — fail-closed, sem regressão.

/// Tamanho da tag Poly1305 (ChaCha20-Poly1305) anexada ao ciphertext.
pub const AEAD_TAG_LEN: usize = 16;
/// Tamanho do nonce AEAD (12B = source_id u32 BE ‖ clock u64 BE).
pub const AEAD_NONCE_LEN: usize = 12;

/// Cache da sk X25519 local (derivada da seed da sessão UMA vez).
static LOCAL_XSK: Mutex<Option<[u8; 32]>> = Mutex::new(None);

/// Cache de chaves AEAD derivadas por peer: (pk Ed25519 do peer, chave 32B).
/// 16 slots = máximo de nós do mesh (mesmo limite da tabela TOFU).
static AEAD_KEYS: Mutex<[Option<([u8; crate::identity::PUBLIC_KEY_LEN], [u8; 32])>; 16]> =
    Mutex::new([const { None }; 16]);

/// sk X25519 local derivada da seed da sessão (X25519 a partir do keypair
/// Ed25519 da sessão — `from_ed25519` faz o clamp). Cacheada. None = sessão
/// não inicializada (fail-closed).
fn local_x25519_sk() -> Option<[u8; 32]> {
    if let Some(sk) = *LOCAL_XSK.lock() {
        return Some(sk);
    }
    let seed = crate::identity::session_seed()?;
    let kp = ed25519_compact::KeyPair::from_seed(ed25519_compact::Seed::new(seed));
    let xsk = ed25519_compact::x25519::SecretKey::from_ed25519(&kp.sk).ok()?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&xsk[..]); // Deref → &[u8; 32] → &[u8]
    *LOCAL_XSK.lock() = Some(out);
    Some(out)
}

/// Chave AEAD compartilhada com o peer: DH(X25519) entre a sk local e a pk
/// X25519 do peer (derivada da pk Ed25519 do TOFU) + KDF `sha256(DH)`.
/// Cacheada por peer. None = sessão ausente / pk inválida (fail-closed).
pub fn aead_key_for_peer(peer_ed_pk: &[u8; crate::identity::PUBLIC_KEY_LEN]) -> Option<[u8; 32]> {
    {
        let cache = AEAD_KEYS.lock();
        for slot in cache.iter() {
            if let Some((pk, key)) = slot {
                if pk == peer_ed_pk {
                    return Some(*key);
                }
            }
        }
    }
    let xsk = local_x25519_sk()?;
    let xsk_obj = ed25519_compact::x25519::SecretKey::from_slice(&xsk).ok()?;
    let ed_pk = ed25519_compact::PublicKey::from_slice(peer_ed_pk).ok()?;
    let xpk = ed25519_compact::x25519::PublicKey::from_ed25519(&ed_pk).ok()?;
    let dh = xpk.dh(&xsk_obj).ok()?;
    let key = crate::tpm::sha256(&dh[..]); // DHOutput deref → [u8; 32] → [u8]
    let mut cache = AEAD_KEYS.lock();
    for slot in cache.iter_mut() {
        if slot.is_none() {
            *slot = Some((*peer_ed_pk, key));
            break;
        }
    }
    Some(key)
}

/// Nonce AEAD de 12B: `source_id` (u32 BE) ‖ `clock` (u64 BE) — ambos do
/// header NoProto (o receptor reconstrói; não vai no wire).
pub fn aead_nonce(source_id: u8, clock: u64) -> [u8; AEAD_NONCE_LEN] {
    let mut n = [0u8; AEAD_NONCE_LEN];
    n[0..4].copy_from_slice(&(source_id as u32).to_be_bytes());
    n[4..AEAD_NONCE_LEN].copy_from_slice(&clock.to_be_bytes());
    n
}

/// Cifra com ChaCha20-Poly1305: AAD = header NoProto (36/37B). Retorna
/// ct ‖ tag16 (a tag vai anexada pela própria `Aead::encrypt`).
fn aead_encrypt(
    key: &[u8; 32],
    nonce: &[u8; AEAD_NONCE_LEN],
    header: &[u8],
    payload: &[u8],
) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .encrypt(Nonce::from_slice(nonce), Payload { msg: payload, aad: header })
        .ok()
}

/// Decifra e verifica a tag Poly1305. None = tamper / chave errada.
fn aead_decrypt(
    key: &[u8; 32],
    nonce: &[u8; AEAD_NONCE_LEN],
    header: &[u8],
    ct: &[u8],
) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: header })
        .ok()
}

/// Sela um payload de DADOS ponto-a-ponto (ADR-0081 Tier F): re-serializa o
/// header com `flags.encrypted = true`, nonce = source_id‖clock, AAD = header.
/// Layout wire: `header ‖ ct ‖ tag16`. Retorna None se não há chave derivada
/// para o peer (fail-closed → caller cai no caminho assinado).
pub fn aead_seal(
    serialized: &[u8],
    peer_ed_pk: &[u8; crate::identity::PUBLIC_KEY_LEN],
) -> Option<Vec<u8>> {
    let header_len = crate::net::noproto::PACKET_HEADER_SIZE;
    if serialized.len() < header_len {
        return None;
    }
    let pkt = crate::net::udp_broadcast::parse(serialized)?;
    let key = aead_key_for_peer(peer_ed_pk)?;
    let mut sealed = pkt;
    sealed.flags.encrypted = true;
    let header = crate::net::udp_broadcast::serialize(&sealed);
    let payload = &serialized[header_len..];
    let nonce = aead_nonce(pkt.source_id, pkt.clock);
    let ct = aead_encrypt(&key, &nonce, &header, payload)?;
    let mut out = header;
    out.extend_from_slice(&ct);
    Some(out)
}

/// Abre um pacote selado (Tier F): verifica a tag e decifra o payload,
/// retornando `header ‖ plaintext` (mesmo formato do caminho verificado).
/// AAD = header; nonce = source_id‖clock do header. None = tag inválida
/// (não foi selado com a chave do source) ou peer desconhecido (fail-closed).
pub fn aead_open(
    wire: &[u8],
    source_ed_pk: &[u8; crate::identity::PUBLIC_KEY_LEN],
) -> Option<Vec<u8>> {
    let header_len = crate::net::noproto::PACKET_HEADER_SIZE;
    if wire.len() < header_len + AEAD_TAG_LEN {
        return None;
    }
    let pkt = crate::net::udp_broadcast::parse(wire)?;
    if !pkt.flags.encrypted {
        return None; // não é AEAD — não abrir aqui
    }
    let key = aead_key_for_peer(source_ed_pk)?;
    let header = &wire[..header_len];
    let ct = &wire[header_len..];
    let nonce = aead_nonce(pkt.source_id, pkt.clock);
    let pt = aead_decrypt(&key, &nonce, header, ct)?;
    let mut out = Vec::with_capacity(header_len + pt.len());
    out.extend_from_slice(header);
    out.extend_from_slice(&pt);
    Some(out)
}

/// Self-test Tier F: derivação de chave simétrica (DH X25519), roundtrip
/// seal/open, integridade (tamper → None) e unicidade de nonce por clock.
/// Usa chaves FAKE (o hook roda antes de `init_session_identity` — a sessão
/// ainda não existe).
pub fn aead_self_test() -> bool {
    let kp_a = ed25519_compact::KeyPair::from_seed(ed25519_compact::Seed::new([0xAA; 32]));
    let kp_b = ed25519_compact::KeyPair::from_seed(ed25519_compact::Seed::new([0xBB; 32]));
    let xsk_a = match ed25519_compact::x25519::SecretKey::from_ed25519(&kp_a.sk) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let xsk_b = match ed25519_compact::x25519::SecretKey::from_ed25519(&kp_b.sk) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let xpk_a = match ed25519_compact::x25519::PublicKey::from_ed25519(&kp_a.pk) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let xpk_b = match ed25519_compact::x25519::PublicKey::from_ed25519(&kp_b.pk) {
        Ok(p) => p,
        Err(_) => return false,
    };
    // DH simétrico → mesma chave nos dois lados (sem handshake no wire).
    let dh_a = match xpk_b.dh(&xsk_a) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let dh_b = match xpk_a.dh(&xsk_b) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let key_a = crate::tpm::sha256(&dh_a[..]);
    let key_b = crate::tpm::sha256(&dh_b[..]);
    if key_a != key_b {
        return false;
    }
    // Header sintético + payload.
    let mut serialized = crate::net::udp_broadcast::serialize(
        &crate::net::noproto::AiosTaskPacket::new(
            100, 7, 0xFF, crate::net::noproto::TaskType::Inference, 1, 0, 0,
            crate::net::noproto::PacketFlags::new(),
        ),
    );
    serialized.extend_from_slice(b"ADR-0081 Tier F: dados ponto-a-ponto selados");
    let hlen = crate::net::noproto::PACKET_HEADER_SIZE;
    // Roundtrip com nonce derivado do header (source_id=7, clock=100).
    let nonce_1 = aead_nonce(7, 100);
    let ct1 = match aead_encrypt(&key_a, &nonce_1, &serialized[..hlen], &serialized[hlen..]) {
        Some(c) => c,
        None => return false,
    };
    let pt1 = match aead_decrypt(&key_b, &nonce_1, &serialized[..hlen], &ct1) {
        Some(p) => p,
        None => return false,
    };
    if pt1 != serialized[hlen..] {
        return false;
    }
    // Tamper no ct → tag falha → None.
    let mut ct2 = ct1.clone();
    let last = ct2.len() - 1;
    ct2[last] ^= 0x01;
    if aead_decrypt(&key_b, &nonce_1, &serialized[..hlen], &ct2).is_some() {
        return false;
    }
    // Nonce diverge por clock (mesmo source_id) → ciphertext diferente.
    let nonce_2 = aead_nonce(7, 101);
    let ct3 = match aead_encrypt(&key_a, &nonce_2, &serialized[..hlen], &serialized[hlen..]) {
        Some(c) => c,
        None => return false,
    };
    if ct3 == ct1 {
        return false;
    }
    true
}
