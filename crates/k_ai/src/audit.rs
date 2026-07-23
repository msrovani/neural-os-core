//! Merkle Audit Trail — chain SHA-256 + Ed25519 por entry (#315.19).
//! Ring buffer 4096. Verifica chain e assinatura (session ou trusted).

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::identity::{sign_session, SIGNATURE_LEN};
use k_nano::tpm::sha256;

const AUDIT_RING_SIZE: usize = 4096;

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub tick: u64,
    pub agent: String,
    pub action: String,
    pub payload_hash: [u8; 32],
    pub prev_hash: [u8; 32],
    pub entry_hash: [u8; 32],
    pub signature: [u8; SIGNATURE_LEN],
}

fn hash_two(prev: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut combined = alloc::vec![0u8; 32 + data.len()];
    combined[..32].copy_from_slice(prev);
    combined[32..].copy_from_slice(data);
    sha256(&combined)
}

pub struct AuditTrail {
    ring: Vec<AuditEntry>,
    head: usize,
    count: usize,
}

impl AuditTrail {
    pub const fn new() -> Self {
        AuditTrail {
            ring: Vec::new(),
            head: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, tick: u64, agent: &str, action: &str, payload: &[u8]) {
        let prev_hash = self
            .ring
            .last()
            .map(|e| e.entry_hash)
            .unwrap_or([0u8; 32]);
        let payload_hash = sha256(payload);
        let mut combined = alloc::vec![0u8; 40];
        combined[..8].copy_from_slice(&tick.to_le_bytes());
        combined[8..].copy_from_slice(&payload_hash);
        let entry_hash = hash_two(&prev_hash, &combined);
        let signature = sign_session(&entry_hash).unwrap_or([0u8; SIGNATURE_LEN]);

        let entry = AuditEntry {
            tick,
            agent: String::from(agent),
            action: String::from(action),
            payload_hash,
            prev_hash,
            entry_hash,
            signature,
        };
        if self.count < AUDIT_RING_SIZE {
            self.ring.push(entry);
        } else {
            self.ring[self.head] = entry;
        }
        self.head = (self.head + 1) % AUDIT_RING_SIZE;
        self.count = self.count.saturating_add(1);
    }

    pub fn verify_chain(&self) -> bool {
        if self.ring.is_empty() {
            return true;
        }
        // Subset load: começa do prev_hash do primeiro (pode ser ≠ 0).
        let mut prev = self.ring[0].prev_hash;
        for entry in &self.ring {
            let mut combined = alloc::vec![0u8; 40];
            combined[..8].copy_from_slice(&entry.tick.to_le_bytes());
            combined[8..].copy_from_slice(&entry.payload_hash);
            let expected = hash_two(&prev, &combined);
            if expected != entry.entry_hash || entry.prev_hash != prev {
                return false;
            }
            prev = entry.entry_hash;
        }
        true
    }

    pub fn verify(&self) -> bool {
        self.verify_chain()
    }

    pub fn last_n(&self, n: usize) -> &[AuditEntry] {
        let start = self.ring.len().saturating_sub(n);
        &self.ring[start..]
    }

    pub fn signed_count(&self) -> usize {
        self.ring
            .iter()
            .filter(|e| e.signature != [0u8; SIGNATURE_LEN])
            .count()
    }

    pub fn status(&self) -> String {
        alloc::format!(
            "[AUDIT] {} entradas, signed={}, chain OK={}",
            self.count,
            self.signed_count(),
            self.verify_chain()
        )
    }

    /// AUD2: tick + prev + entry + payload hashes + signature 64 + agent/action.
    pub fn flush_to_sgdb(&self) -> bool {
        if !crate::sgdb::ready() {
            return false;
        }
        const MAX: usize = 64;
        let slice = self.last_n(MAX);
        let mut buf = Vec::new();
        buf.extend_from_slice(b"AUD2");
        buf.extend_from_slice(&(self.count as u64).to_le_bytes());
        let last = self.ring.last().map(|e| e.entry_hash).unwrap_or([0u8; 32]);
        buf.extend_from_slice(&last);
        buf.extend_from_slice(&(slice.len() as u32).to_le_bytes());
        for e in slice {
            buf.extend_from_slice(&e.tick.to_le_bytes());
            buf.extend_from_slice(&e.prev_hash);
            buf.extend_from_slice(&e.entry_hash);
            buf.extend_from_slice(&e.payload_hash);
            buf.extend_from_slice(&e.signature);
            let al = (e.agent.len().min(32)) as u8;
            let cl = (e.action.len().min(32)) as u8;
            buf.push(al);
            buf.push(cl);
            buf.extend_from_slice(&e.agent.as_bytes()[..al as usize]);
            buf.extend_from_slice(&e.action.as_bytes()[..cl as usize]);
        }
        crate::sgdb::put_kv("audit/head", &buf).is_ok()
    }

    /// Load AUD2 (fallback AUD1 legado sem sig).
    pub fn load_from_sgdb(&mut self) -> bool {
        let Ok(Some(buf)) = crate::sgdb::get_kv("audit/head") else {
            return false;
        };
        if buf.len() < 8 {
            return false;
        }
        if &buf[0..4] == b"AUD2" {
            return self.load_aud2(&buf);
        }
        if &buf[0..4] == b"AUD1" {
            return self.load_aud1_legacy(&buf);
        }
        false
    }

    fn load_aud2(&mut self, buf: &[u8]) -> bool {
        if buf.len() < 4 + 8 + 32 + 4 {
            return false;
        }
        let mut off = 4;
        let count = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        off += 8;
        off += 32;
        let n = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        self.ring.clear();
        self.head = 0;
        self.count = 0;
        for _ in 0..n {
            // tick8 + prev32 + entry32 + payload32 + sig64 + al1 + cl1
            if off + 8 + 32 + 32 + 32 + SIGNATURE_LEN + 2 > buf.len() {
                break;
            }
            let tick = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
            off += 8;
            let mut prev_hash = [0u8; 32];
            prev_hash.copy_from_slice(&buf[off..off + 32]);
            off += 32;
            let mut entry_hash = [0u8; 32];
            entry_hash.copy_from_slice(&buf[off..off + 32]);
            off += 32;
            let mut payload_hash = [0u8; 32];
            payload_hash.copy_from_slice(&buf[off..off + 32]);
            off += 32;
            let mut signature = [0u8; SIGNATURE_LEN];
            signature.copy_from_slice(&buf[off..off + SIGNATURE_LEN]);
            off += SIGNATURE_LEN;
            let al = buf[off] as usize;
            let cl = buf[off + 1] as usize;
            off += 2;
            if off + al + cl > buf.len() {
                break;
            }
            let agent = core::str::from_utf8(&buf[off..off + al])
                .unwrap_or("?")
                .into();
            off += al;
            let action = core::str::from_utf8(&buf[off..off + cl])
                .unwrap_or("?")
                .into();
            off += cl;
            self.ring.push(AuditEntry {
                tick,
                agent,
                action,
                payload_hash,
                prev_hash,
                entry_hash,
                signature,
            });
            self.count = self.count.saturating_add(1);
        }
        let _ = count;
        !self.ring.is_empty() && self.verify_chain()
    }

    fn load_aud1_legacy(&mut self, buf: &[u8]) -> bool {
        if buf.len() < 4 + 8 + 32 + 4 {
            return false;
        }
        let mut off = 4;
        off += 8 + 32;
        let n = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        self.ring.clear();
        self.head = 0;
        self.count = 0;
        for _ in 0..n {
            if off + 8 + 32 + 32 + 2 > buf.len() {
                break;
            }
            let tick = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
            off += 8;
            let mut entry_hash = [0u8; 32];
            entry_hash.copy_from_slice(&buf[off..off + 32]);
            off += 32;
            let mut payload_hash = [0u8; 32];
            payload_hash.copy_from_slice(&buf[off..off + 32]);
            off += 32;
            let al = buf[off] as usize;
            let cl = buf[off + 1] as usize;
            off += 2;
            if off + al + cl > buf.len() {
                break;
            }
            let agent = core::str::from_utf8(&buf[off..off + al])
                .unwrap_or("?")
                .into();
            off += al;
            let action = core::str::from_utf8(&buf[off..off + cl])
                .unwrap_or("?")
                .into();
            off += cl;
            let prev_hash = self
                .ring
                .last()
                .map(|e| e.entry_hash)
                .unwrap_or([0u8; 32]);
            self.ring.push(AuditEntry {
                tick,
                agent,
                action,
                payload_hash,
                prev_hash,
                entry_hash,
                signature: [0u8; SIGNATURE_LEN],
            });
            self.count = self.count.saturating_add(1);
        }
        !self.ring.is_empty()
    }
}
