//! UDP Broadcast — descoberta de nos na LAN.
//! Fase A da ADR-0081: transporte UDP para Brain Mesh.
//!
//! Ponte entre k_nano (NoProto + Mesh) e hermes (NETSTACK + smoltcp UDP).
//! k_nano expoe as funcoes de serializacao; hermes fornece o socket.
//!
//! Integracao: NetAgent chama `send_discovery()` e `recv_packet()`.
//!
//! Depende de: smoltcp UDP socket no NETSTACK (hermes).

use crate::net::noproto::{AiosTaskPacket, TaskType, PacketFlags};
use crate::identity;
use alloc::vec::Vec;
use alloc::vec;
use core::mem;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

/// Tamanho do pacote NoProto em bytes (repr(C, packed) = 36 bytes).
pub const PACKET_SIZE: usize = mem::size_of::<AiosTaskPacket>();

/// Cria pacote de discovery para broadcast.
pub fn make_discovery(source_id: u8, clock: u64) -> AiosTaskPacket {
    AiosTaskPacket {
        magic: 0x41494F53,
        clock,
        source_id,
        dest_id: 0xFF,
        task_type: TaskType::Sync,
        priority: 0,
        tensor_len: 0,
        param_len: 0,
        flags: PacketFlags { persist: false, require_ack: false, compressed: false, encrypted: false, _reserved: 0 },
        reserved: [0; 8],
    }
}

/// Cria pacote de heartbeat para broadcast.
pub fn make_heartbeat(source_id: u8, clock: u64) -> AiosTaskPacket {
    AiosTaskPacket {
        magic: 0x41494F53,
        clock,
        source_id,
        dest_id: 0xFF,
        task_type: TaskType::Heartbeat,
        priority: 1,
        tensor_len: 0,
        param_len: 0,
        flags: PacketFlags { persist: false, require_ack: false, compressed: false, encrypted: false, _reserved: 0 },
        reserved: [0; 8],
    }
}

/// Serializa pacote para envio via UDP — zero-copy sobre #[repr(C, packed)].
pub fn serialize(packet: &AiosTaskPacket) -> Vec<u8> {
    let size = mem::size_of::<AiosTaskPacket>();
    let mut buf = vec![0u8; size];
    unsafe {
        core::ptr::copy_nonoverlapping(
            packet as *const AiosTaskPacket as *const u8,
            buf.as_mut_ptr(),
            size,
        );
    }
    buf
}

/// Tenta parsear buffer UDP recebido como pacote NoProto.
pub fn parse(data: &[u8]) -> Option<AiosTaskPacket> {
    if data.len() < mem::size_of::<AiosTaskPacket>() {
        return None;
    }
    // Leitura direta do buffer via repr(C, packed) — zero-copy
    let packet = unsafe {
        core::ptr::read_unaligned(data.as_ptr() as *const AiosTaskPacket)
    };
    // Valida magic number
    if packet.magic != 0x41494F53 {
        return None;
    }
    Some(packet)
}

// ── Step 6: Ed25519 signing / verification ──────────────────────────
// ADR-0081 tier cripto (crates/k_nano/src/crypto.rs):
// - `sign_packet_authentic`: SEMPRE Ed25519 (64B) — caminho de controle/
//   TOFU (heartbeat, ROLE, PK\0/CAP). Raro (~1.1s no heartbeat).
// - `sign_packet_tiered` (= `sign_packet`): DADOS — Tier Relativized anexa
//   tag HMAC-SHA256 32B (key = SEGMENT_KEY); Tier Full usa Ed25519.

/// Assina o payload serializado do pacote NoProto com a chave de sessão.
/// Retorna o pacote original + assinatura de 64 bytes concatenada.
/// Caminho "authentic" (Ed25519): heartbeat/ROLE/TOFU — sempre assimétrica,
/// prova de posse da chave de sessão.
///
/// Uso: `let signed = sign_packet_authentic(&serialize(&pkt));`
pub fn sign_packet_authentic(serialized: &[u8]) -> Option<Vec<u8>> {
    let sig = identity::sign_session(serialized)?;
    let mut out = Vec::with_capacity(serialized.len() + identity::SIGNATURE_LEN);
    out.extend_from_slice(serialized);
    out.extend_from_slice(&sig);
    Some(out)
}

/// Assina conforme o tier cripto do mesh (ADR-0081):
/// - Tier Relativized (`crypto_tier() == Relativized`): anexa tag
///   HMAC-SHA256 de 32B (key = SEGMENT_KEY) — ~1.3µs/pacote @1.2KB.
/// - Tier Full: Ed25519 (idêntico ao caminho authentic).
pub fn sign_packet_tiered(serialized: &[u8]) -> Option<Vec<u8>> {
    if let Some(key) = crate::net::mesh::segment_key() {
        let tag = crate::crypto::hmac_sha256(&key, serialized);
        let mut out = Vec::with_capacity(serialized.len() + crate::crypto::HMAC_TAG_LEN);
        out.extend_from_slice(serialized);
        out.extend_from_slice(&tag);
        Some(out)
    } else {
        sign_packet_authentic(serialized)
    }
}

/// Alias do caminho tiered — usado pelos call sites de DADOS (matmul,
/// experts, FL, skills, CRDT, knowledge). Heartbeat/ROLE usam
/// `sign_packet_authentic`.
pub fn sign_packet(serialized: &[u8]) -> Option<Vec<u8>> {
    sign_packet_tiered(serialized)
}

/// Verifica a assinatura Ed25519 no final de um pacote recebido.
/// O pacote deve ter: [NoProto header] + [64-byte signature].
/// Retorna o payload sem assinatura se a verificação passar.
pub fn verify_packet<'a>(data: &'a [u8], pk: &[u8; identity::PUBLIC_KEY_LEN]) -> Option<&'a [u8]> {
    if data.len() < mem::size_of::<AiosTaskPacket>() + identity::SIGNATURE_LEN {
        return None;
    }
    let sig_offset = data.len() - identity::SIGNATURE_LEN;
    let pkt_data = &data[..sig_offset];
    let sig_bytes = &data[sig_offset..];
    let mut sig = [0u8; identity::SIGNATURE_LEN];
    sig.copy_from_slice(sig_bytes);
    if identity::verify_signature(pk, pkt_data, &sig) {
        Some(pkt_data)
    } else {
        None
    }
}

/// Verifica um pacote de DADOS conforme o tier cripto atual:
/// - Tier Relativized: tag HMAC-SHA256 de 32B no final (key = SEGMENT_KEY),
///   comparada com `ct_eq` (constant-time).
/// - Tier Full: delega ao `verify_packet` (Ed25519, comportamento atual).
/// Heartbeat/ROLE/TOFU devem SEMPRE usar `verify_packet` (Ed25519).
pub fn verify_packet_tiered<'a>(data: &'a [u8], pk: &[u8; identity::PUBLIC_KEY_LEN]) -> Option<&'a [u8]> {
    if let Some(key) = crate::net::mesh::segment_key() {
        if data.len() < mem::size_of::<AiosTaskPacket>() + crate::crypto::HMAC_TAG_LEN {
            return None;
        }
        let tag_offset = data.len() - crate::crypto::HMAC_TAG_LEN;
        let pkt_data = &data[..tag_offset];
        let tag = &data[tag_offset..];
        let expect = crate::crypto::hmac_sha256(&key, pkt_data);
        if crate::crypto::ct_eq(tag, &expect) {
            Some(pkt_data)
        } else {
            None
        }
    } else {
        verify_packet(data, pk)
    }
}

// ── Tier F (ADR-0081): seams AEAD ponto-a-ponto (X25519 + ChaCha20-Poly1305) ──
// Os DADOS direcionados (MR\0, EDR\0) no Tier Full são SELADOS (confidencialidade
// + autenticação) em vez de apenas assinados. Broadcasts (dest_id = 0xFF) não
// têm receptor único para derivar chave → permanecem assinados (fail-closed).

/// Seam TX do Tier F: sela com AEAD quando ponto-a-ponto (dest != 0xFF) no
/// Tier Full e a pk do destino é conhecida; senão cai no caminho assinado
/// (`sign_packet_tiered` — HMAC Relativized / Ed25519 Full).
pub fn seal_packet_tiered(serialized: &[u8], dest_id: u8) -> Option<Vec<u8>> {
    if dest_id != 0xFF && crate::net::mesh::crypto_tier() == crate::net::mesh::CryptoTier::Full {
        if let Some(pk) = crate::net::mesh::peer_public_key(dest_id) {
            if let Some(sealed) = crate::crypto::aead_seal(serialized, &pk) {
                return Some(sealed);
            }
        }
    }
    sign_packet_tiered(serialized)
}

/// Seam RX do Tier F: se `flags.encrypted` → abre com AEAD (devolve
/// header ‖ plaintext); senão → `verify_packet_tiered`. `pk` = pk Ed25519
/// vinculada ao source no TOFU (usada só no caminho assinado).
pub fn verify_or_open_tiered(
    data: &[u8],
    pk: &[u8; identity::PUBLIC_KEY_LEN],
) -> Option<Vec<u8>> {
    let pkt = parse(data)?;
    if pkt.flags.encrypted {
        crate::crypto::aead_open(data, pk)
    } else {
        verify_packet_tiered(data, pk).map(|v| v.to_vec())
    }
}

// ── Transporte P2P R0 (ADR-0081 Fase A) ──────────────────────────────────
// Porta 42069, broadcast 255.255.255.255, NIC real (e1000/VirtIO/RTL8139).
// Movido do bin (SESSION_234): o transporte mesh agora vive em k_nano — o bin
// só chama `mesh::p2p_tick()` e consome pacotes via EVENT_BUS ("P2P_PACKET").

/// Contadores de RX/TX do transporte P2P (independentes do smoltcp do bin).
static NET_TX_COUNT: AtomicU64 = AtomicU64::new(0);
static NET_RX_COUNT: AtomicU64 = AtomicU64::new(0);

/// Total de frames TX do transporte P2P (k_nano).
pub fn k_nano_tx_count() -> u64 { NET_TX_COUNT.load(Ordering::Relaxed) }

/// Total de frames RX do transporte P2P (k_nano).
pub fn k_nano_rx_count() -> u64 { NET_RX_COUNT.load(Ordering::Relaxed) }

/// Checksum IP (RFC 1071) — mesmo algoritmo do bin netstack.
fn ip_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in data.chunks(2) {
        let word = u16::from_be_bytes([chunk[0], *chunk.get(1).unwrap_or(&0)]);
        sum = sum.wrapping_add(word as u32);
    }
    while sum >> 16 != 0 { sum = (sum & 0xFFFF) + (sum >> 16); }
    !(sum as u16)
}

/// Envia via NIC real: VirtIO → E1000 → RTL8139 (gate canônico e1000).
/// Sem wifi/slip/I225 — esses paths continuam no smoltcp do bin.
unsafe fn nic_send_k(data: Vec<u8>) {
    if let Some(ref mut nic) = *crate::nic_globals::VIRTIO_DEV.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::nic_globals::E1000.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::nic_globals::RTL8139.lock() {
        nic.send(&data); return;
    }
}

/// Recebe do NIC real: VirtIO → E1000 → RTL8139.
unsafe fn nic_recv_k() -> Option<Vec<u8>> {
    if let Some(ref mut nic) = *crate::nic_globals::VIRTIO_DEV.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::nic_globals::E1000.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::nic_globals::RTL8139.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    None
}

/// Monta frame Ethernet + IP + UDP com destino broadcast (FF:FF:FF:FF:FF:FF → 255.255.255.255).
/// Lê (sip, smac) do `nic_globals::NET_CONFIG` (sync via `set_nic_config` pelo bin).
pub fn build_udp_broadcast_frame(payload: &[u8], port: u16) -> Option<Vec<u8>> {
    let (sip, smac) = {
        let cfg = crate::nic_globals::NET_CONFIG.lock();
        let sip = if cfg.ip != [0; 4] { cfg.ip } else { [10, 0, 2, 15] };
        (sip, cfg.mac)
    };
    if smac == [0; 6] || payload.is_empty() {
        return None;
    }
    let src_port: u16 = 42069;
    let udp_len = (8 + payload.len()) as u16;
    let mut udp = Vec::with_capacity(udp_len as usize);
    udp.extend_from_slice(&src_port.to_be_bytes());
    udp.extend_from_slice(&port.to_be_bytes());
    udp.extend_from_slice(&udp_len.to_be_bytes());
    udp.extend_from_slice(&[0x00, 0x00]);
    udp.extend_from_slice(payload);

    let dst: [u8; 4] = [255, 255, 255, 255];
    let total_len = (20 + udp.len()) as u16;
    let mut ip = [0u8; 20];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&total_len.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17; // UDP
    ip[12..16].copy_from_slice(&sip);
    ip[16..20].copy_from_slice(&dst);
    let cs = ip_checksum(&ip);
    ip[10..12].copy_from_slice(&cs.to_be_bytes());

    let dmac = [0xFF; 6]; // broadcast Ethernet
    let mut frame = Vec::with_capacity(14 + 20 + udp.len());
    frame.extend_from_slice(&dmac);
    frame.extend_from_slice(&smac);
    frame.extend_from_slice(&[0x08, 0x00]);
    frame.extend_from_slice(&ip);
    frame.extend_from_slice(&udp);
    Some(frame)
}

/// Envia payload UDP para 255.255.255.255:port (broadcast mesh P2P).
pub fn udp_broadcast_send(payload: &[u8], port: u16) -> bool {
    let Some(frame) = build_udp_broadcast_frame(payload, port) else {
        return false;
    };
    unsafe { nic_send_k(frame) };
    NET_TX_COUNT.fetch_add(1, Ordering::Relaxed);
    true
}

// ─── Unicast P2P (Phase 1 reliability) ──────────────────────────────────────
// Mesmo formato Ethernet+IP+UDP do broadcast, mas com destino MAC específico
// (não FF:FF:FF:FF:FF:FF). Usado por send_fragmented_unicast /
// recv_fragmented_unicast para entregas direcionadas ponto-a-ponto.

/// Monta frame Ethernet + IP + UDP com destino MAC específico (unicast).
/// Lê (sip, smac) do `nic_globals::NET_CONFIG` (sync via `set_nic_config`).
pub fn build_udp_unicast_frame(payload: &[u8], dest_mac: [u8; 6], port: u16) -> Option<Vec<u8>> {
    let (sip, smac) = {
        let cfg = crate::nic_globals::NET_CONFIG.lock();
        let sip = if cfg.ip != [0; 4] { cfg.ip } else { [10, 0, 2, 15] };
        (sip, cfg.mac)
    };
    if smac == [0; 6] || payload.is_empty() {
        return None;
    }
    let src_port: u16 = 42069;
    let udp_len = (8 + payload.len()) as u16;
    let mut udp = Vec::with_capacity(udp_len as usize);
    udp.extend_from_slice(&src_port.to_be_bytes());
    udp.extend_from_slice(&port.to_be_bytes());
    udp.extend_from_slice(&udp_len.to_be_bytes());
    udp.extend_from_slice(&[0x00, 0x00]);
    udp.extend_from_slice(payload);

    let dst: [u8; 4] = [255, 255, 255, 255];
    let total_len = (20 + udp.len()) as u16;
    let mut ip = [0u8; 20];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&total_len.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17; // UDP
    ip[12..16].copy_from_slice(&sip);
    ip[16..20].copy_from_slice(&dst);
    let cs = ip_checksum(&ip);
    ip[10..12].copy_from_slice(&cs.to_be_bytes());

    let mut frame = Vec::with_capacity(14 + 20 + udp.len());
    frame.extend_from_slice(&dest_mac);
    frame.extend_from_slice(&smac);
    frame.extend_from_slice(&[0x08, 0x00]);
    frame.extend_from_slice(&ip);
    frame.extend_from_slice(&udp);
    Some(frame)
}

/// Envia payload UDP unicast para dest_mac:port (não broadcast).
pub fn send_unicast(payload: &[u8], dest_mac: [u8; 6], port: u16) -> bool {
    let Some(frame) = build_udp_unicast_frame(payload, dest_mac, port) else {
        return false;
    };
    unsafe { nic_send_k(frame) };
    NET_TX_COUNT.fetch_add(1, Ordering::Relaxed);
    true
}

// ── Test-only RX injection (host) ───────────────────────────────────────────
// Host tests não têm NIC (`nic_recv_k` → None sempre): sem este ponto de
// injeção, `recv_fragmented`/`recv_fragmented_unicast` são intestáveis no
// host — a remontagem FRAG só era provada em QEMU (LOG AGENTES step 3).
// Compilado FORA de builds não-test (#[cfg(test)]): o binário bare-metal
// fica idêntico e nenhum comportamento de produção muda.
#[cfg(test)]
static TEST_RX_INJECT: Mutex<Vec<(Vec<u8>, [u8; 6])>> = Mutex::new(Vec::new());

/// Pop FIFO de um pacote injetado — só existe em builds de teste.
#[cfg(test)]
fn test_rx_inject_pop() -> Option<(Vec<u8>, [u8; 6])> {
    let mut q = TEST_RX_INJECT.lock();
    if q.is_empty() { None } else { Some(q.remove(0)) }
}

/// Recebe um payload UDP (dst_port == port) do RX do NIC, filtrando por
/// destino MAC != broadcast (unicast). Não bloqueia.
/// Retorna (payload, src_mac) para permitir ACK direto.
pub fn recv_unicast_with_mac(port: u16) -> Option<(Vec<u8>, [u8; 6])> {
    // Test-only: host tests injetam pacotes aqui (host não tem NIC).
    #[cfg(test)]
    if let Some(inj) = test_rx_inject_pop() {
        return Some(inj);
    }
    // Drena até achar um pacote UDP unicast para nossa porta (ou esvazia o RX).
    for _ in 0..16 {
        let pkt = unsafe { nic_recv_k()? };
        NET_RX_COUNT.fetch_add(1, Ordering::Relaxed);
        if pkt.len() < 14 + 20 + 8 {
            continue;
        }
        // Filtra broadcast Ethernet (FF:FF:FF:FF:FF:FF).
        if pkt[0] == 0xFF && pkt[1] == 0xFF && pkt[2] == 0xFF
            && pkt[3] == 0xFF && pkt[4] == 0xFF && pkt[5] == 0xFF {
            continue;
        }
        let src_mac = [pkt[6], pkt[7], pkt[8], pkt[9], pkt[10], pkt[11]];
        if pkt[12] != 0x08 || pkt[13] != 0x00 {
            continue;
        }
        let ihl = (pkt[14] & 0x0f) as usize * 4;
        if ihl < 20 || pkt.len() < 14 + ihl + 8 {
            continue;
        }
        if pkt[14 + 9] != 17 {
            continue; // não-UDP
        }
        let udp = 14 + ihl;
        let dport = u16::from_be_bytes([pkt[udp + 2], pkt[udp + 3]]);
        if dport != port {
            continue;
        }
        let ulen = u16::from_be_bytes([pkt[udp + 4], pkt[udp + 5]]) as usize;
        if ulen < 8 || pkt.len() < udp + ulen {
            continue;
        }
        return Some((pkt[udp + 8..udp + ulen].to_vec(), src_mac));
    }
    None
}

/// Recebe um payload UDP (dst_port == port) do RX do NIC.
/// Retorna (payload, src_mac) para popular cache ARP.
pub fn udp_broadcast_recv_with_mac(port: u16) -> Option<(Vec<u8>, [u8; 6])> {
    // Test-only: host tests injetam pacotes aqui (host não tem NIC).
    #[cfg(test)]
    if let Some(inj) = test_rx_inject_pop() {
        return Some(inj);
    }
    // Drena até achar um pacote UDP para nossa porta (ou esvazia o RX).
    for _ in 0..16 {
        let pkt = unsafe { nic_recv_k()? };
        NET_RX_COUNT.fetch_add(1, Ordering::Relaxed);
        if pkt.len() < 14 + 20 + 8 {
            continue;
        }
        if pkt[12] != 0x08 || pkt[13] != 0x00 {
            continue;
        }
        let ihl = (pkt[14] & 0x0f) as usize * 4;
        if ihl < 20 || pkt.len() < 14 + ihl + 8 {
            continue;
        }
        if pkt[14 + 9] != 17 {
            continue; // não-UDP
        }
        let udp = 14 + ihl;
        let dport = u16::from_be_bytes([pkt[udp + 2], pkt[udp + 3]]);
        if dport != port {
            continue;
        }
        let ulen = u16::from_be_bytes([pkt[udp + 4], pkt[udp + 5]]) as usize;
        if ulen < 8 || pkt.len() < udp + ulen {
            continue;
        }
        let src_mac = [pkt[6], pkt[7], pkt[8], pkt[9], pkt[10], pkt[11]];
        return Some((pkt[udp + 8..udp + ulen].to_vec(), src_mac));
    }
    None
}

/// Recebe um payload UDP (dst_port == port) do RX do NIC.
/// Versão compatível que retorna só o payload.
pub fn udp_broadcast_recv(port: u16) -> Option<Vec<u8>> {
    udp_broadcast_recv_with_mac(port).map(|(p, _)| p)
}

// ─── Fragmentação MTU (ADR-0081, SESSION_237) ──────────────────────────────
// Payloads > MTU Ethernet não cabem num frame UDP único. `send_fragmented`
// divide o blob (JÁ assinado pelo chamador — compute assina e depois chama)
// em fragmentos "FRAG\0"; o receptor reassembla ANTES do verify_packet, então
// a verificação continua válida no payload completo. O caminho ≤1200B
// (heartbeat/ROLE/skills) é inalterado — compatibilidade total.

/// Cabeçalho do fragmento: "FRAG\0" (5B) + frag_id u32 LE + total_frags u32 LE
/// + frag_idx u32 LE + total_len u32 LE = 21 bytes.
const FRAG_HEADER_SIZE: usize = 5 + 16;
/// Tamanho máximo de dados por fragmento (fragmento total ≤ 1021B → frame ≤ 1063B).
const FRAG_MAX_CHUNK: usize = 1000;
/// Payloads ≤ 1200B seguem o caminho direto (frame ≤ 1242B, sem fragmentar).
const FRAG_DIRECT_MAX: usize = 1200;
/// Máximo de fragmentos por mensagem (bitmask [u8; 8] = 64 bits).
const FRAG_MAX_PARTS: u32 = 64;
/// Teto de payload fragmentável = FRAG_MAX_PARTS × FRAG_MAX_CHUNK (64.000B).
/// Acima disso o TX emitiria total_frags > 64, que TODO receptor dropa no
/// guard de header — mensagem indeliverável (SESSION_354: sucesso falso é
/// pior que falha honesta).
const FRAG_MAX_PAYLOAD: usize = (FRAG_MAX_PARTS as usize) * FRAG_MAX_CHUNK;

/// Contador global de frag_id (único por boot — suficiente em broadcast LAN).
static FRAG_ID: AtomicU32 = AtomicU32::new(1);

/// Envia payload; fragmenta se > 1200B. O payload deve ser o blob JÁ assinado
/// (NoProto+payload+assinatura) — a fragmentação é ANTES do wire, o reassembly
/// é DEPOIS do wire e ANTES do verify_packet no receptor.
pub fn send_fragmented(payload: &[u8], port: u16) -> bool {
    // Guard honesto no TX: payload > 64.000B emitiria total_frags > 64, que
    // todo receptor dropa — retornar true seria mentira de sucesso.
    if payload.len() > FRAG_MAX_PAYLOAD {
        crate::slog_nano!(
            "P2P", "warn",
            "frag TX REJECT reason=oversize len={} limit={}",
            payload.len(), FRAG_MAX_PAYLOAD
        );
        return false;
    }
    if payload.len() <= FRAG_DIRECT_MAX {
        return udp_broadcast_send(payload, port);
    }
    let id = FRAG_ID.fetch_add(1, Ordering::Relaxed);
    let total_frags = ((payload.len() + FRAG_MAX_CHUNK - 1) / FRAG_MAX_CHUNK) as u32;
    let total_len = payload.len() as u32;
    let mut ok = true;
    let mut off = 0usize;
    for idx in 0..total_frags {
        let end = core::cmp::min(off + FRAG_MAX_CHUNK, payload.len());
        let chunk = &payload[off..end];
        off = end;
        let mut frag = Vec::with_capacity(FRAG_HEADER_SIZE + chunk.len());
        frag.extend_from_slice(b"FRAG\0");
        frag.extend_from_slice(&id.to_le_bytes());
        frag.extend_from_slice(&total_frags.to_le_bytes());
        frag.extend_from_slice(&idx.to_le_bytes());
        frag.extend_from_slice(&total_len.to_le_bytes());
        frag.extend_from_slice(chunk);
        if !udp_broadcast_send(&frag, port) {
            ok = false;
        }
    }
    // "info" → TRACE mudo (ADR-0092); gate FRAG precisa de "ok" na serial.
    crate::slog_nano!("P2P", "ok", "frag TX id={} partes={} len={}", id, total_frags, payload.len());
    ok
}

/// Estado de reassembly de um payload fragmentado (2 slots simultâneos bastam).
struct FragReassembly {
    id: u32,
    total_frags: u32,
    received: u32,
    total_len: usize,
    /// bitmask de fragmentos recebidos (64 bits — FRAG_MAX_PARTS).
    seen: [u8; 8],
    /// pedaços por índice (fora de ordem ok — concatenação por índice).
    chunks: Vec<Vec<u8>>,
    /// TIMER_TICKS da última atualização (timeout simples).
    last_tick: u64,
}

/// Tabela de reassembly — 16 slots. Slot livre = None; se todos ocupados por
/// outros ids, o mais antigo é descartado (timeout simples por tick).
static REASSEMBLY: Mutex<[Option<FragReassembly>; 16]> = Mutex::new([const { None }; 16]);

/// Recebe um payload UDP: fragmentos "FRAG\0" são reassemblados; qualquer
/// outro pacote (≤1200B, caminho compatível) retorna direto. Não bloqueia —
/// retorna None quando o RX esvazia e nenhum reassembly completou.
/// Phase 2: popula cache MAC (peer_set_mac) ao receber pacotes.
pub fn recv_fragmented(port: u16) -> Option<Vec<u8>> {
    let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    // Timeout simples: descarta slots parados há >2000 ticks (Phase 1 mesh).
    {
        let mut table = REASSEMBLY.lock();
        for slot in table.iter_mut() {
            if let Some(rs) = slot {
                if now.wrapping_sub(rs.last_tick) > 2000 {
                    *slot = None;
                }
            }
        }
    }
    loop {
        let Some((pkt, src_mac)) = udp_broadcast_recv_with_mac(port) else { return None; };
        // Popula cache MAC do remetente.
        let sender_id = if pkt.len() >= crate::net::noproto::PACKET_HEADER_SIZE {
            if let Some(p) = crate::net::udp_broadcast::parse(&pkt) {
                p.source_id
            } else { 0 }
        } else { 0 };
        if sender_id != 0 {
            crate::net::mesh::peer_set_mac(sender_id, src_mac);
        }
        if !pkt.starts_with(b"FRAG\0") {
            // Payload normal (compatibilidade) — retorna direto.
            return Some(pkt);
        }
        if pkt.len() < FRAG_HEADER_SIZE {
            continue; // fragmento malformado — descarta
        }
        let id = u32::from_le_bytes([pkt[5], pkt[6], pkt[7], pkt[8]]);
        let total_frags = u32::from_le_bytes([pkt[9], pkt[10], pkt[11], pkt[12]]);
        let idx = u32::from_le_bytes([pkt[13], pkt[14], pkt[15], pkt[16]]);
        let total_len = u32::from_le_bytes([pkt[17], pkt[18], pkt[19], pkt[20]]) as usize;
        let max_legit_len = (FRAG_MAX_PARTS as usize) * FRAG_MAX_CHUNK;
        if total_frags == 0 || total_frags > FRAG_MAX_PARTS || idx >= total_frags || total_len == 0 || total_len > max_legit_len {
            continue; // cabeçalho inválido — descarta
        }
        // AIOS: RAM baixa → DEGRADED (drop FRAG grande), nunca OOM/halt no reassembly.
        if !crate::memory::can_afford_frag(total_len) {
            crate::net::mesh::note_frag_drop_pressure();
            // Throttle: 1 slog / ~200 ticks (evita flood serial nos 1G).
            static LAST_DROP_LOG: core::sync::atomic::AtomicU64 =
                core::sync::atomic::AtomicU64::new(0);
            let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
            let last = LAST_DROP_LOG.load(Ordering::Relaxed);
            if last == 0 || now.wrapping_sub(last) >= 200 {
                LAST_DROP_LOG.store(now, Ordering::Relaxed);
                crate::slog_nano!(
                    "P2P", "warn",
                    "frag DROP pressure len={} budget={} RAM={}MB (DEGRADED)",
                    total_len,
                    crate::memory::frag_reassembly_budget_bytes(
                        crate::memory::TOTAL_RAM_MB.load(Ordering::Relaxed)
                    ),
                    crate::memory::TOTAL_RAM_MB.load(Ordering::Relaxed)
                );
            }
            continue;
        }
        let chunk = &pkt[FRAG_HEADER_SIZE..];
        // M10 (onda 4): chunk maior que o máximo = header mentiroso — drop.
        if chunk.is_empty() || chunk.len() > FRAG_MAX_CHUNK {
            continue;
        }
        let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        let mut table = REASSEMBLY.lock();
        // Slot do id; senão um livre; senão o mais antigo (reuso).
        let slot_pos = {
            let mut match_pos: Option<usize> = None;
            let mut free_pos: Option<usize> = None;
            let mut oldest_pos: Option<usize> = None;
            let mut oldest_tick = u64::MAX;
            for (i, slot) in table.iter().enumerate() {
                match slot {
                    Some(rs) if rs.id == id => match_pos = Some(i),
                    Some(rs) => {
                        if rs.last_tick < oldest_tick {
                            oldest_tick = rs.last_tick;
                            oldest_pos = Some(i);
                        }
                    }
                    None => {
                        if free_pos.is_none() {
                            free_pos = Some(i);
                        }
                    }
                }
            }
            match_pos.or(free_pos).or(oldest_pos)
        }?;
        if table[slot_pos].as_ref().map_or(true, |rs| rs.id != id) {
            // Slot livre ou reutilizado — inicia reassembly deste id.
            table[slot_pos] = Some(FragReassembly {
                id,
                total_frags,
                received: 0,
                total_len,
                seen: [0u8; 8],
                chunks: Vec::new(),
                last_tick: now,
            });
        }
        let byte = (idx / 8) as usize;
        let bit = 1u8 << (idx % 8);
        let rs = table[slot_pos].as_mut().unwrap();
        if (rs.seen[byte] & bit) != 0 {
            continue; // fragmento duplicado — ignora
        }
        rs.seen[byte] |= bit;
        if rs.chunks.len() <= idx as usize {
            rs.chunks.resize(idx as usize + 1, Vec::new());
        }
        // try_reserve: sob pressão, drop em vez de OOM handler (AIOS DEGRADED).
        {
            let dest = &mut rs.chunks[idx as usize];
            if dest.try_reserve(chunk.len()).is_err() {
                crate::net::mesh::note_frag_drop_other();
                crate::slog_nano!(
                    "P2P", "warn",
                    "frag DROP alloc pressure idx={} len={} (DEGRADED)",
                    idx, chunk.len()
                );
                table[slot_pos] = None;
                continue;
            }
            dest.clear();
            dest.extend_from_slice(chunk);
        }
        rs.received += 1;
        rs.last_tick = now;

        // Completo? Concatena por índice e libera o slot.
        if rs.received == rs.total_frags {
            let complete_id = rs.id;
            let complete_parts = rs.total_frags;
            let complete_len = rs.total_len;
            let mut out = Vec::with_capacity(complete_len);
            for c in &rs.chunks {
                out.extend_from_slice(c);
            }
            table[slot_pos] = None;
            drop(table);
            // M10: o total do header tem que bater com o que foi reassemblado —
            // divergência = header mentiroso / truncamento silencioso.
            if out.len() != complete_len {
                crate::slog_nano!(
                    "P2P", "warn",
                    "frag RX id={} DROP len_mismatch out={} esperado={}",
                    complete_id, out.len(), complete_len
                );
                continue;
            }
            crate::slog_nano!(
                "P2P", "ok",
                "frag RX id={} partes={} len={}", complete_id, complete_parts, complete_len
            );
            return Some(out);
        }
        // Ainda incompleto — continua drenando.
    }
}

// ─── Fragmentação unicast (Phase 1 reliability) ─────────────────────────────
// Mesmo formato "FRAG\0" do broadcast, mas entrega direcionada ponto-a-ponto
// via send_unicast/recv_unicast. Compartilha a tabela REASSEMBLY (16 slots)
// — unicast e broadcast usam frag_id global único por boot.
//
// ACK seletivo (Phase 2): cada fragmento "FRAG\0" exige ACK "FRACK\0".
// send_fragmented_unicast faz stop-and-wait por fragmento (timeout 50 ticks).
// recv_fragmented_unicast envia ACK automático após inserir fragmento.

/// Header do ACK de fragmento: "FRACK\0" (6B) + frag_id u32 LE + idx u32 LE = 14 bytes.
const FRACK_HEADER_SIZE: usize = 6 + 8;
/// Timeout por ACK em ticks. O PIT roda a ~18.2Hz (~55ms/tick), NÃO 100Hz:
/// 9 ticks ≈ 500ms. O valor antigo (50) dava ~2.75s por tentativa — com 4
/// tentativas × N fragmentos o TX bloqueava por minutos.
const FRACK_TIMEOUT_TICKS: u64 = 9;
/// Max retransmissões por fragmento.
const FRACK_MAX_RETRIES: u8 = 3;

/// Monta o payload do ACK de fragmento (wire format): "FRACK\0" (6B)
/// + frag_id u32 LE + idx u32 LE = 14 bytes (FRACK_HEADER_SIZE).
fn build_frag_ack(frag_id: u32, idx: u32) -> Vec<u8> {
    let mut ack = Vec::with_capacity(FRACK_HEADER_SIZE);
    ack.extend_from_slice(b"FRACK\0");
    ack.extend_from_slice(&frag_id.to_le_bytes());
    ack.extend_from_slice(&idx.to_le_bytes());
    ack
}

/// Envia ACK de fragmento via unicast.
fn send_frack(dest_mac: [u8; 6], port: u16, frag_id: u32, idx: u32) -> bool {
    send_unicast(&build_frag_ack(frag_id, idx), dest_mac, port)
}

/// Stash de pacotes não-FRACK consumidos no ACK-wait do
/// `send_fragmented_unicast` (M10 onda 4): antes eram silenciosamente
/// descartados. `recv_fragmented_unicast` drena a stash antes do NIC.
static UCAST_STASH: Mutex<Vec<(Vec<u8>, [u8; 6])>> = Mutex::new(Vec::new());

/// Envia payload unicast; fragmenta se > 1200B. O payload deve ser o blob JÁ
/// assinado (NoProto+payload+assinatura) — a fragmentação é ANTES do wire, o
/// reassembly é DEPOIS do wire e ANTES do verify_packet no receptor.
/// Phase 2: stop-and-wait com ACK seletivo por fragmento.
pub fn send_fragmented_unicast(payload: &[u8], dest_mac: [u8; 6], port: u16) -> bool {
    // Guard honesto no TX (mesmo contrato do send_fragmented): > 64.000B é
    // indeliverável — recusa antes de qualquer frame/ACK-wait.
    if payload.len() > FRAG_MAX_PAYLOAD {
        crate::slog_nano!(
            "P2P", "warn",
            "frag-unicast TX REJECT reason=oversize len={} limit={}",
            payload.len(), FRAG_MAX_PAYLOAD
        );
        return false;
    }
    if payload.len() <= FRAG_DIRECT_MAX {
        return send_unicast(payload, dest_mac, port);
    }
    let id = FRAG_ID.fetch_add(1, Ordering::Relaxed);
    let total_frags = ((payload.len() + FRAG_MAX_CHUNK - 1) / FRAG_MAX_CHUNK) as u32;
    let total_len = payload.len() as u32;
    let mut ok = true;
    let mut off = 0usize;
    for idx in 0..total_frags {
        let end = core::cmp::min(off + FRAG_MAX_CHUNK, payload.len());
        let chunk = &payload[off..end];
        off = end;
        let mut frag = Vec::with_capacity(FRAG_HEADER_SIZE + chunk.len());
        frag.extend_from_slice(b"FRAG\0");
        frag.extend_from_slice(&id.to_le_bytes());
        frag.extend_from_slice(&total_frags.to_le_bytes());
        frag.extend_from_slice(&idx.to_le_bytes());
        frag.extend_from_slice(&total_len.to_le_bytes());
        frag.extend_from_slice(chunk);
        
        // Stop-and-wait: envia fragmento e espera ACK.
        let mut retries = 0;
        let mut acked = false;
        while retries <= FRACK_MAX_RETRIES && !acked {
            if send_unicast(&frag, dest_mac, port) {
                // Espera ACK por FRACK_TIMEOUT_TICKS.
                let start = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
                loop {
                    let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
                    if now.wrapping_sub(start) >= FRACK_TIMEOUT_TICKS {
                        break; // timeout → retransmite
                    }
                    if let Some((rx, src_mac)) = recv_unicast_with_mac(port) {
                        if rx.len() >= FRACK_HEADER_SIZE && &rx[0..6] == b"FRACK\0" {
                            let ack_id = u32::from_le_bytes([rx[6], rx[7], rx[8], rx[9]]);
                            let ack_idx = u32::from_le_bytes([rx[10], rx[11], rx[12], rx[13]]);
                            if ack_id == id && ack_idx == idx {
                                acked = true;
                                break;
                            }
                        } else {
                            // M10: pacote não-FRACK chegou no meio do ACK-wait —
                            // faz stash p/ recv_fragmented_unicast, não descarta.
                            let mut st = UCAST_STASH.lock();
                            if st.len() < 8 {
                                st.push((rx, src_mac));
                            } else {
                                crate::net::mesh::note_frag_drop_other();
                            }
                        }
                    }
                    // ponytail: dorme até a próxima IRQ (timer/NIC) em vez de
                    // queimar o core; se IF=0 não dá pra dormir → spin.
                    if x86_64::instructions::interrupts::are_enabled() {
                        x86_64::instructions::hlt();
                    } else {
                        core::hint::spin_loop();
                    }
                }
            }
            if !acked {
                retries += 1;
                crate::slog_nano!("P2P", "warn", "frag-unicast retry id={} idx={} attempt={}", id, idx, retries);
            }
        }
        if !acked {
            ok = false;
            crate::slog_nano!("P2P", "error", "frag-unicast FAILED id={} idx={} after {} retries", id, idx, FRACK_MAX_RETRIES);
        }
    }
    crate::slog_nano!("P2P", "info", "frag-unicast TX id={} partes={} len={} ok={}", id, total_frags, payload.len(), ok);
    ok
}

/// Recebe um payload UDP unicast: fragmentos "FRAG\0" são reassemblados;
/// qualquer outro pacote (≤1200B, caminho compatível) retorna direto.
/// Não bloqueia — retorna None quando o RX esvazia e nenhum reassembly completou.
/// Phase 2: envia ACK "FRACK\0" automático após inserir cada fragmento.
pub fn recv_fragmented_unicast(port: u16) -> Option<Vec<u8>> {
    let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    // Timeout simples: descarta slots parados há >2000 ticks (Phase 1 mesh).
    {
        let mut table = REASSEMBLY.lock();
        for slot in table.iter_mut() {
            if let Some(rs) = slot {
                if now.wrapping_sub(rs.last_tick) > 2000 {
                    *slot = None;
                }
            }
        }
    }
    loop {
        // M10: drena a stash (não-FRACK do ACK-wait) antes do NIC.
        let (pkt, src_mac) = {
            let mut st = UCAST_STASH.lock();
            if !st.is_empty() {
                st.remove(0)
            } else {
                drop(st);
                let Some(rx) = recv_unicast_with_mac(port) else { return None; };
                rx
            }
        };
        if !pkt.starts_with(b"FRAG\0") {
            // Payload normal (compatibilidade) — retorna direto.
            return Some(pkt);
        }
        if pkt.len() < FRAG_HEADER_SIZE {
            continue; // fragmento malformado — descarta
        }
        let id = u32::from_le_bytes([pkt[5], pkt[6], pkt[7], pkt[8]]);
        let total_frags = u32::from_le_bytes([pkt[9], pkt[10], pkt[11], pkt[12]]);
        let idx = u32::from_le_bytes([pkt[13], pkt[14], pkt[15], pkt[16]]);
        let total_len = u32::from_le_bytes([pkt[17], pkt[18], pkt[19], pkt[20]]) as usize;
        let max_legit_len = (FRAG_MAX_PARTS as usize) * FRAG_MAX_CHUNK;
        if total_frags == 0 || total_frags > FRAG_MAX_PARTS || idx >= total_frags || total_len == 0 || total_len > max_legit_len {
            continue; // cabeçalho inválido — descarta
        }
        if !crate::memory::can_afford_frag(total_len) {
            crate::net::mesh::note_frag_drop_pressure();
            crate::slog_nano!(
                "P2P", "warn",
                "frag DROP pressure (ucast) len={} RAM={}MB (DEGRADED)",
                total_len,
                crate::memory::TOTAL_RAM_MB.load(Ordering::Relaxed)
            );
            continue;
        }
        let chunk = &pkt[FRAG_HEADER_SIZE..];
        // M10: chunk maior que o máximo = header mentiroso — drop.
        if chunk.is_empty() || chunk.len() > FRAG_MAX_CHUNK {
            continue;
        }
        let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        let mut table = REASSEMBLY.lock();
        // Slot do id; senão um livre; senão o mais antigo (reuso).
        let slot_pos = {
            let mut match_pos: Option<usize> = None;
            let mut free_pos: Option<usize> = None;
            let mut oldest_pos: Option<usize> = None;
            let mut oldest_tick = u64::MAX;
            for (i, slot) in table.iter().enumerate() {
                match slot {
                    Some(rs) if rs.id == id => match_pos = Some(i),
                    Some(rs) => {
                        if rs.last_tick < oldest_tick {
                            oldest_tick = rs.last_tick;
                            oldest_pos = Some(i);
                        }
                    }
                    None => {
                        if free_pos.is_none() {
                            free_pos = Some(i);
                        }
                    }
                }
            }
            match_pos.or(free_pos).or(oldest_pos)
        }?;
        if table[slot_pos].as_ref().map_or(true, |rs| rs.id != id) {
            // Slot livre ou reutilizado — inicia reassembly deste id.
            table[slot_pos] = Some(FragReassembly {
                id,
                total_frags,
                received: 0,
                total_len,
                seen: [0u8; 8],
                chunks: Vec::new(),
                last_tick: now,
            });
        }
        let byte = (idx / 8) as usize;
        let bit = 1u8 << (idx % 8);
        let rs = table[slot_pos].as_mut().unwrap();
        if (rs.seen[byte] & bit) != 0 {
            // Fragmento duplicado — envia ACK mesmo assim (idempotente).
            drop(table);
            let _ = send_frack(src_mac, port, id, idx);
            continue;
        }
        rs.seen[byte] |= bit;
        if rs.chunks.len() <= idx as usize {
            rs.chunks.resize(idx as usize + 1, Vec::new());
        }
        // try_reserve: sob pressão, drop em vez de OOM handler (AIOS DEGRADED).
        {
            let dest = &mut rs.chunks[idx as usize];
            if dest.try_reserve(chunk.len()).is_err() {
                crate::net::mesh::note_frag_drop_other();
                crate::slog_nano!(
                    "P2P", "warn",
                    "frag DROP alloc pressure idx={} len={} (DEGRADED)",
                    idx, chunk.len()
                );
                table[slot_pos] = None;
                continue;
            }
            dest.clear();
            dest.extend_from_slice(chunk);
        }
        rs.received += 1;
        rs.last_tick = now;
        
        // Envia ACK automático para o remetente.
        drop(table);
        let _ = send_frack(src_mac, port, id, idx);
        
        // Re-trava para verificar completude.
        let mut table = REASSEMBLY.lock();
        let Some(rs) = table[slot_pos].as_mut() else { continue; };
        
        // Completo? Concatena por índice e libera o slot.
        if rs.received == rs.total_frags {
            let complete_id = rs.id;
            let complete_parts = rs.total_frags;
            let complete_len = rs.total_len;
            let mut out = Vec::with_capacity(complete_len);
            for c in &rs.chunks {
                out.extend_from_slice(c);
            }
            table[slot_pos] = None;
            drop(table);
            // M10: o total do header tem que bater com o reassemblado.
            if out.len() != complete_len {
                crate::slog_nano!(
                    "P2P", "warn",
                    "frag-unicast RX id={} DROP len_mismatch out={} esperado={}",
                    complete_id, out.len(), complete_len
                );
                continue;
            }
            crate::slog_nano!(
                "P2P", "info",
                "frag-unicast RX id={} partes={} len={}", complete_id, complete_parts, complete_len
            );
            return Some(out);
        }
        // Ainda incompleto — continua drenando.
    }
}

// ─── Testes host — protocolo FRAG/FRACK (LOG AGENTES step 3) ────────────────
// Propriedades: remontagem (in/out-of-order), truncamento, duplicação, perda,
// boundary 64.000, formato FRACK, chunking TX, prioridade da stash M10.
//
// Como dirigir no host (padrão p2p_sim: statics dirigidos direto — o PIT não
// corre no host, TIMER_TICKS é um AtomicUsize que o teste store()a):
// - TX: `nic_send_k` é no-op (sem NIC) — a evidência é o contador NET_TX_COUNT.
// - RX: pacotes injetados em TEST_RX_INJECT (seam cfg(test) acima) / UCAST_STASH.
//
// NÃO testável no host: o loop stop-and-wait do `send_fragmented_unicast`
// (espera TIMER_TICKS avançar — PIT IRQ — e executa `hlt`, instrução
// privilegiada em user-mode host → crash do processo de teste). Esse caminho
// continua provado em QEMU (SESSION_242).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::interrupts::TIMER_TICKS;
    use alloc::vec::Vec;
    use core::sync::atomic::Ordering;

    /// Serializa testes que mutam statics compartilhados (SESSION_346):
    /// REASSEMBLY, UCAST_STASH, TEST_RX_INJECT, TIMER_TICKS, NET_CONFIG.
    static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());

    const TEST_PORT: u16 = 42069;
    const PEER_MAC: [u8; 6] = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0x01];

    /// Reset completo do estado FRAG entre testes (statics compartilhados).
    /// TOTAL_RAM_MB: o default no host é 512 → budget de reassembly = 1200B
    /// (nó frugal DEGRADED — produção correta, mas droparia os payloads de
    /// teste >1200B no guard `can_afford_frag`). 0 = "RAM desconhecida" →
    /// budget pleno 64.000 (ramo `ram_mb == 0` de `frag_reassembly_budget_bytes`).
    /// Nenhum outro teste host lê TOTAL_RAM_MB (memory tests = allocator only).
    fn reset_frag_state() {
        for slot in REASSEMBLY.lock().iter_mut() {
            *slot = None;
        }
        UCAST_STASH.lock().clear();
        TEST_RX_INJECT.lock().clear();
        TIMER_TICKS.store(0, Ordering::Relaxed);
        crate::memory::TOTAL_RAM_MB.store(0, Ordering::Relaxed);
    }

    /// MAC/IP p/ build_udp_*_frame (nic_send_k é no-op no host — o frame é
    /// descartado; sem MAC não-zero o build do frame recusa).
    fn setup_nic_config() {
        crate::nic_globals::set_nic_config([0x02, 0x00, 0x00, 0x00, 0x00, 0x01], [10, 0, 2, 15]);
    }

    /// Monta um fragmento no wire format: "FRAG\0" + id/total/idx/len u32 LE + chunk.
    fn make_frag(id: u32, total_frags: u32, idx: u32, total_len: u32, chunk: &[u8]) -> Vec<u8> {
        let mut f = Vec::with_capacity(FRAG_HEADER_SIZE + chunk.len());
        f.extend_from_slice(b"FRAG\0");
        f.extend_from_slice(&id.to_le_bytes());
        f.extend_from_slice(&total_frags.to_le_bytes());
        f.extend_from_slice(&idx.to_le_bytes());
        f.extend_from_slice(&total_len.to_le_bytes());
        f.extend_from_slice(chunk);
        f
    }

    /// Payload determinístico de n bytes (padrão reconhecível, não-zeros).
    fn pattern(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    /// Injeta um pacote UDP (payload, não frame) no RX — consumido por
    /// recv_fragmented (broadcast) e recv_fragmented_unicast (via
    /// recv_unicast_with_mac, após a stash).
    fn inject(pkt: Vec<u8>) {
        TEST_RX_INJECT.lock().push((pkt, PEER_MAC));
    }

    /// Propriedade 1 (remontagem in-order): fragmentos inseridos em ordem →
    /// payload exato reassemblado; mensagem de 1 fragmento completa sozinha.
    #[test]
    fn frag_reassembly_in_order_exact_payload() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        // 2500B = 3 chunks (1000 + 1000 + 500).
        let payload = pattern(2500);
        let id = 0x10_01;
        inject(make_frag(id, 3, 0, 2500, &payload[0..1000]));
        inject(make_frag(id, 3, 1, 2500, &payload[1000..2000]));
        inject(make_frag(id, 3, 2, 2500, &payload[2000..2500]));
        let out = recv_fragmented(TEST_PORT).expect("3 frags em ordem devem reassemblar");
        assert_eq!(out.len(), 2500);
        assert_eq!(out, payload, "payload reassemblado deve ser byte-exato");

        // Mensagem de 1 fragmento (≤ FRAG_MAX_CHUNK) completa imediatamente.
        let small = pattern(800);
        inject(make_frag(0x10_02, 1, 0, 800, &small));
        let out = recv_fragmented(TEST_PORT).expect("1 frag deve completar sozinho");
        assert_eq!(out, small);
    }

    /// Propriedade 2 (remontagem out-of-order): leituras intermediárias
    /// retornam None (nunca parcial); completa só quando o último pedaço
    /// chega — e os anteriores são retidos, não descartados.
    #[test]
    fn frag_reassembly_out_of_order_never_partial() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        let payload = pattern(2500);
        let id = 0x20_01;
        // Fora de ordem: idx 2 e idx 0 — falta idx 1.
        inject(make_frag(id, 3, 2, 2500, &payload[2000..2500]));
        inject(make_frag(id, 3, 0, 2500, &payload[0..1000]));
        assert!(recv_fragmented(TEST_PORT).is_none(), "2/3 frags: nunca parcial");

        // Último pedaço → completa com o payload exato.
        inject(make_frag(id, 3, 1, 2500, &payload[1000..2000]));
        let out = recv_fragmented(TEST_PORT).expect("último frag completa a remontagem");
        assert_eq!(out, payload);
    }

    /// Propriedade 3 (truncamento): cabeçalhos mentirosos são rejeitados.
    /// ⚠️ O RX dropa em silêncio hoje — os guards de header/chunk fazem
    /// `continue` sem slog; o drop só é observável pela ausência de
    /// reassembly (documentado aqui, conforme pedido).
    #[test]
    fn frag_rejects_oversized_chunk_and_bad_totals() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        // chunk.len() > FRAG_MAX_CHUNK (1000): header promete total_len=1500
        // num fragmento único — guard M10 dropa antes de criar slot.
        inject(make_frag(0x30_01, 1, 0, 1500, &pattern(1500)));
        assert!(recv_fragmented(TEST_PORT).is_none(), "chunk > FRAG_MAX_CHUNK deve ser dropado");

        // chunk vazio (header sem payload) — mesmo guard do M10.
        inject(make_frag(0x30_02, 2, 0, 5, &[]));
        assert!(recv_fragmented(TEST_PORT).is_none(), "chunk vazio deve ser dropado");

        // total_len == 0: header inválido.
        inject(make_frag(0x30_03, 1, 0, 0, b"x"));
        assert!(recv_fragmented(TEST_PORT).is_none(), "total_len=0 deve ser dropado");

        // total_frags == 0 e idx >= total_frags: mesmos guards de header.
        inject(make_frag(0x30_04, 0, 0, 10, b"x"));
        assert!(recv_fragmented(TEST_PORT).is_none(), "total_frags=0 deve ser dropado");
        inject(make_frag(0x30_05, 2, 2, 10, b"x"));
        assert!(recv_fragmented(TEST_PORT).is_none(), "idx >= total_frags deve ser dropado");

        // Nenhum drop acima pode ter envenenado a tabela: mensagem válida
        // seguinte reassembla normalmente.
        let ok = pattern(100);
        inject(make_frag(0x30_06, 1, 0, 100, &ok));
        assert_eq!(recv_fragmented(TEST_PORT), Some(ok));
    }

    /// Propriedade 4 (duplicação): o mesmo idx inserido 2× é idempotente —
    /// bitmask não conta 2× e o payload não corrompe. Se o duplicado fosse
    /// contado, a conclusão dispararia cedo (received==total_frags com 2
    /// chunks) → len_mismatch → drop → None em vez do payload exato.
    /// Caminho unicast de propósito: cobre também o branch de dup que
    /// re-envia FRACK (idempotente).
    #[test]
    fn frag_duplicate_is_idempotent() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        let payload = pattern(2500);
        let id = 0x40_01;
        inject(make_frag(id, 3, 0, 2500, &payload[0..1000]));
        inject(make_frag(id, 3, 0, 2500, &payload[0..1000])); // duplicado
        inject(make_frag(id, 3, 1, 2500, &payload[1000..2000]));
        inject(make_frag(id, 3, 2, 2500, &payload[2000..2500]));
        let out =
            recv_fragmented_unicast(TEST_PORT).expect("dup não deve corromper a remontagem");
        assert_eq!(out, payload, "payload byte-exato apesar do duplicado");
    }

    /// Propriedade 5 (perda): fragmento que nunca chega → remontagem fica
    /// incompleta (None) e o slot não vaza — mensagem NOVA reassembla
    /// normalmente; o slot velho expira por timeout (>2000 ticks) e não
    /// completa tarde (o fragmento atrasado inicia slot novo 1/3 → None).
    #[test]
    fn frag_loss_incomplete_slot_reusable_and_expiry() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        let payload = pattern(2500);
        let lost_id = 0x50_01;
        // idx 2 nunca chega (perda).
        inject(make_frag(lost_id, 3, 0, 2500, &payload[0..1000]));
        inject(make_frag(lost_id, 3, 1, 2500, &payload[1000..2000]));
        assert!(recv_fragmented(TEST_PORT).is_none(), "com perda: nunca completa");

        // Slot não vaza: mensagem nova (outro id) reassembla normalmente.
        let fresh = pattern(600);
        inject(make_frag(0x50_02, 1, 0, 600, &fresh));
        assert_eq!(recv_fragmented(TEST_PORT), Some(fresh));

        // Expiry: avança TIMER_TICKS > 2000 (PIT não corre no host — o teste
        // dirige o static direto, padrão p2p_sim). O sweep de entrada descarta
        // o slot velho; o fragmento que faltava NÃO completa a mensagem antiga.
        // Se o slot tivesse sobrevivido, retornaria Some(payload antigo).
        TIMER_TICKS.store(2001, Ordering::Relaxed);
        inject(make_frag(lost_id, 3, 2, 2500, &payload[2000..2500]));
        assert!(
            recv_fragmented(TEST_PORT).is_none(),
            "slot expirado não deve completar tarde"
        );
        TIMER_TICKS.store(0, Ordering::Relaxed);
    }

    /// Propriedade 6 (boundary): exatamente 64.000B (64 chunks × 1000) é
    /// aceito — teto = FRAG_MAX_PARTS × FRAG_MAX_CHUNK = max_legit_len e
    /// também o budget de reassembly no host (RAM=0 → 64.000); 64.001 é
    /// rejeitado (o RX dropa em silêncio — guard de header sem slog).
    #[test]
    fn frag_boundary_64000_accepted_64001_rejected() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        let payload = pattern(64_000);
        let id = 0x60_01;
        for i in 0..64u32 {
            let off = (i as usize) * 1000;
            inject(make_frag(id, 64, i, 64_000, &payload[off..off + 1000]));
        }
        let out = recv_fragmented(TEST_PORT).expect("64.000B = 64 chunks deve reassemblar");
        assert_eq!(out.len(), 64_000);
        assert_eq!(out, payload);

        // 64.001B: total_frags=65 > FRAG_MAX_PARTS(64) E total_len > 64.000 —
        // os dois guards rejeitam. O TX agora recusa esse tamanho no entry
        // (guard honesto — ver send_fragmented_oversize_rejected_no_frames),
        // então esse header só chega ao RX de um par desatualizado/malicioso.
        inject(make_frag(0x60_02, 65, 0, 64_001, &pattern(1000)));
        assert!(recv_fragmented(TEST_PORT).is_none(), "64.001B deve ser rejeitado");
    }

    /// Propriedade 7 (formato FRACK): build_frag_ack produz o wire format de
    /// 14B ("FRACK\0" + frag_id u32 LE + idx u32 LE) — round-trip parse igual
    /// ao feito no ACK-wait do send_fragmented_unicast (rx[6..10]/rx[10..14]).
    #[test]
    fn frack_ack_wire_format_roundtrip() {
        assert_eq!(FRACK_HEADER_SIZE, 14, "FRACK = 6B magic + 8B campos");
        let (fid, idx) = (0xDEAD_BEEFu32, 42u32);
        let ack = build_frag_ack(fid, idx);
        assert_eq!(ack.len(), 14);
        assert_eq!(&ack[0..6], b"FRACK\0");
        let parsed_id = u32::from_le_bytes([ack[6], ack[7], ack[8], ack[9]]);
        let parsed_idx = u32::from_le_bytes([ack[10], ack[11], ack[12], ack[13]]);
        assert_eq!(parsed_id, fid);
        assert_eq!(parsed_idx, idx);
    }

    /// Propriedade TX (chunking do send_fragmented): ≤1200B vai direto (1
    /// frame); acima, ceil(len/1000) fragmentos. Evidência = delta de
    /// NET_TX_COUNT (nic_send_k é no-op no host — frame descartado).
    #[test]
    fn send_fragmented_frame_counts_match_chunking() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        let base = k_nano_tx_count();
        assert!(send_fragmented(&pattern(100), TEST_PORT), "payload pequeno: caminho direto");
        assert_eq!(k_nano_tx_count() - base, 1, "≤1200B = 1 frame (sem fragmentar)");

        let base = k_nano_tx_count();
        assert!(send_fragmented(&pattern(1200), TEST_PORT));
        assert_eq!(k_nano_tx_count() - base, 1, "1200B = limite direto (≤ FRAG_DIRECT_MAX)");

        let base = k_nano_tx_count();
        assert!(send_fragmented(&pattern(1201), TEST_PORT));
        assert_eq!(k_nano_tx_count() - base, 2, "1201B = 2 fragmentos");

        let base = k_nano_tx_count();
        assert!(send_fragmented(&pattern(2500), TEST_PORT));
        assert_eq!(k_nano_tx_count() - base, 3, "2500B = ceil(2500/1000) = 3 fragmentos");
    }

    /// Propriedade TX (guard honesto de oversize): payload > 64.000B
    /// (FRAG_MAX_PARTS × FRAG_MAX_CHUNK) é recusado com `false` ANTES de
    /// emitir qualquer frame (NET_TX_COUNT delta 0) — tanto no broadcast
    /// quanto no unicast. Antes do guard, o TX emitia total_frags > 64 que
    /// todo receptor dropa (true = mentira de sucesso, classe SESSION_354).
    /// O ramo unicast é seguro no host: o guard dispara antes do loop
    /// stop-and-wait (que executa `hlt` — não testável em user-mode).
    #[test]
    fn send_fragmented_oversize_rejected_no_frames() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        // 64.001B = 1 byte acima do teto — o caso mínimo indeliverável.
        let base = k_nano_tx_count();
        assert!(!send_fragmented(&pattern(64_001), TEST_PORT), "oversize broadcast deve recusar");
        assert_eq!(k_nano_tx_count() - base, 0, "nenhum frame pode ser emitido no oversize");

        let base = k_nano_tx_count();
        assert!(
            !send_fragmented_unicast(&pattern(64_001), PEER_MAC, TEST_PORT),
            "oversize unicast deve recusar"
        );
        assert_eq!(k_nano_tx_count() - base, 0, "nenhum frame unicast no oversize");

        // Bem acima do teto também recusa (não é só o caso limítrofe).
        let base = k_nano_tx_count();
        assert!(!send_fragmented(&pattern(100_000), TEST_PORT));
        assert_eq!(k_nano_tx_count() - base, 0);
    }

    /// Propriedade TX (boundary): exatamente 64.000B ainda é entregável —
    /// 64 fragmentos, todos emitidos, retorno true. O caminho de 1 frame
    /// (≤ FRAG_DIRECT_MAX) permanece inalterado.
    #[test]
    fn send_fragmented_boundary_64000_still_delivers() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        // Caminho de 1 frame inalterado (≤1200B → direto, sem fragmentar).
        let base = k_nano_tx_count();
        assert!(send_fragmented(&pattern(1200), TEST_PORT));
        assert_eq!(k_nano_tx_count() - base, 1, "≤1200B continua 1 frame");

        // Boundary: 64.000B = 64 chunks exatos → 64 frames, true.
        let base = k_nano_tx_count();
        assert!(send_fragmented(&pattern(64_000), TEST_PORT), "64.000B é entregável");
        assert_eq!(
            k_nano_tx_count() - base, 64,
            "64.000B = ceil(64000/1000) = 64 fragmentos emitidos"
        );
    }

    /// Propriedade M10 (stash): recv_fragmented_unicast drena UCAST_STASH
    /// (pacotes não-FRACK capturados durante o ACK-wait do TX) ANTES do NIC.
    #[test]
    fn unicast_stash_drained_before_nic() {
        let _g = TEST_LOCK.lock();
        reset_frag_state();
        setup_nic_config();

        // Stash: pacote não-FRACK (o tipo que o ACK-wait stashou).
        UCAST_STASH.lock().push((b"PLAINTEXT-DATA".to_vec(), PEER_MAC));
        // Fila de injeção: mensagem FRAG completa de 1 fragmento.
        let frag_payload = pattern(300);
        inject(make_frag(0x90_01, 1, 0, 300, &frag_payload));

        // 1ª chamada: stash primeiro — retorna o pacote não-FRACK.
        assert_eq!(
            recv_fragmented_unicast(TEST_PORT),
            Some(b"PLAINTEXT-DATA".to_vec()),
            "stash (não-FRACK) deve ser drenada antes do NIC"
        );
        // 2ª chamada: agora o FRAG (recv_unicast_with_mac → injeção).
        assert_eq!(recv_fragmented_unicast(TEST_PORT), Some(frag_payload));
    }
}
