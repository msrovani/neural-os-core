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

/// Recebe um payload UDP (dst_port == port) do RX do NIC. Não bloqueia.
pub fn udp_broadcast_recv(port: u16) -> Option<Vec<u8>> {
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
        return Some(pkt[udp + 8..udp + ulen].to_vec());
    }
    None
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

/// Recebe um payload UDP (dst_port == port) do RX do NIC, filtrando por
/// destino MAC != broadcast (unicast). Não bloqueia.
pub fn recv_unicast(port: u16) -> Option<Vec<u8>> {
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
        return Some(pkt[udp + 8..udp + ulen].to_vec());
    }
    None
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

/// Contador global de frag_id (único por boot — suficiente em broadcast LAN).
static FRAG_ID: AtomicU32 = AtomicU32::new(1);

/// Envia payload; fragmenta se > 1200B. O payload deve ser o blob JÁ assinado
/// (NoProto+payload+assinatura) — a fragmentação é ANTES do wire, o reassembly
/// é DEPOIS do wire e ANTES do verify_packet no receptor.
pub fn send_fragmented(payload: &[u8], port: u16) -> bool {
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
    crate::slog_nano!("P2P", "info", "frag TX id={} partes={} len={}", id, total_frags, payload.len());
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
        let pkt = udp_broadcast_recv(port)?;
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
        if total_frags == 0 || total_frags > FRAG_MAX_PARTS || idx >= total_frags || total_len == 0 {
            continue; // cabeçalho inválido — descarta
        }
        let chunk = &pkt[FRAG_HEADER_SIZE..];
        if chunk.is_empty() {
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
        rs.chunks[idx as usize] = chunk.to_vec();
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
            crate::slog_nano!(
                "P2P", "info",
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

/// Envia payload unicast; fragmenta se > 1200B. O payload deve ser o blob JÁ
/// assinado (NoProto+payload+assinatura) — a fragmentação é ANTES do wire, o
/// reassembly é DEPOIS do wire e ANTES do verify_packet no receptor.
pub fn send_fragmented_unicast(payload: &[u8], dest_mac: [u8; 6], port: u16) -> bool {
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
        if !send_unicast(&frag, dest_mac, port) {
            ok = false;
        }
    }
    crate::slog_nano!("P2P", "info", "frag-unicast TX id={} partes={} len={}", id, total_frags, payload.len());
    ok
}

/// Recebe um payload UDP unicast: fragmentos "FRAG\0" são reassemblados;
/// qualquer outro pacote (≤1200B, caminho compatível) retorna direto.
/// Não bloqueia — retorna None quando o RX esvazia e nenhum reassembly completou.
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
        let pkt = recv_unicast(port)?;
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
        if total_frags == 0 || total_frags > FRAG_MAX_PARTS || idx >= total_frags || total_len == 0 {
            continue; // cabeçalho inválido — descarta
        }
        let chunk = &pkt[FRAG_HEADER_SIZE..];
        if chunk.is_empty() {
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
        rs.chunks[idx as usize] = chunk.to_vec();
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
            crate::slog_nano!(
                "P2P", "info",
                "frag-unicast RX id={} partes={} len={}", complete_id, complete_parts, complete_len
            );
            return Some(out);
        }
        // Ainda incompleto — continua drenando.
    }
}
