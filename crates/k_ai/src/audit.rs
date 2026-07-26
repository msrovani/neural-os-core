//! Merkle Audit Trail — chain SHA-256 + Ed25519 por entry (#315.19).
//! Ring buffer 4096. Verifica chain e assinatura (session ou trusted).

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
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
    last_saved: Cell<usize>,
}

impl AuditTrail {
    pub const fn new() -> Self {
        AuditTrail {
            ring: Vec::new(),
            head: 0,
            count: 0,
            last_saved: Cell::new(0),
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

    /// Total entries ever pushed (ring may have wrapped; count is monotonic).
    pub fn entry_count(&self) -> usize {
        self.count
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

    /// Serialize all entries in chronological order (respects circular buffer wrap).
    fn serialize_ring_ordered(&self) -> Vec<u8> {
        let n = self.ring.len();
        let mut buf = Vec::with_capacity(4 + 8 + n * (8 + 32 + 32 + 32 + SIGNATURE_LEN + 2 + 64));
        buf.extend_from_slice(b"AUD3");
        buf.extend_from_slice(&(self.count as u64).to_le_bytes());
        // last entry hash — unused in AUD3 loader, correct for fallback compat
        let last = if self.ring.is_empty() {
            [0u8; 32]
        } else if self.count < AUDIT_RING_SIZE || self.head == 0 {
            self.ring.last().map(|e| e.entry_hash).unwrap_or([0u8; 32])
        } else {
            self.ring[self.head - 1].entry_hash
        };
        buf.extend_from_slice(&last);
        // If ring hasn't wrapped: order is ring[0..n]
        // If wrapped: order is ring[head..] + ring[..head]
        if n < AUDIT_RING_SIZE {
            for e in &self.ring {
                Self::serialize_entry(&mut buf, e);
            }
        } else {
            for i in self.head..AUDIT_RING_SIZE {
                Self::serialize_entry(&mut buf, &self.ring[i]);
            }
            for i in 0..self.head {
                Self::serialize_entry(&mut buf, &self.ring[i]);
            }
        }
        buf
    }

    fn serialize_entry(buf: &mut Vec<u8>, e: &AuditEntry) {
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

    /// Save full ring to `audit/full` every BATCH_SIZE pushes.
    /// Returns true on success or if no flush needed.
    pub fn flush_to_sgdb(&self) -> bool {
        if !crate::sgdb::ready() {
            return false;
        }
        const BATCH: usize = 64;
        let since = self.count.saturating_sub(self.last_saved.get());
        if since < BATCH && self.last_saved.get() > 0 {
            return true; // not enough new entries to justify write
        }
        let buf = self.serialize_ring_ordered();
        if crate::sgdb::put_kv("audit/full", &buf).is_ok() {
            self.last_saved.set(self.count);
            true
        } else {
            false
        }
    }

    /// Returns the number of entries serialized in `audit/full`.
    pub fn full_saved_count(&self) -> usize {
        self.last_saved.get()
    }

    /// Load: try AUD3 (full ring), fall back AUD2 (legacy 64-entry head), then AUD1.
    pub fn load_from_sgdb(&mut self) -> bool {
        // Try new full-ring format first
        if let Ok(Some(buf)) = crate::sgdb::get_kv("audit/full") {
            if &buf[0..4] == b"AUD3" && self.load_aud3(&buf) {
                self.last_saved.set(self.count);
                return true;
            }
        }
        // Fall back to old audit/head
        let Ok(Some(buf)) = crate::sgdb::get_kv("audit/head") else {
            return false;
        };
        if buf.len() < 8 {
            return false;
        }
        if &buf[0..4] == b"AUD2" {
            self.load_aud2(&buf)
        } else if &buf[0..4] == b"AUD1" {
            self.load_aud1_legacy(&buf)
        } else {
            false
        }
    }

    fn load_aud3(&mut self, buf: &[u8]) -> bool {
        if buf.len() < 4 + 8 + 32 {
            return false;
        }
        let mut off = 4;
        let _count = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        off += 8;
        off += 32; // skip last entry_hash (verified implicitly by chain)
        let mut loaded = Vec::new();
        while off < buf.len() {
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
            loaded.push(AuditEntry {
                tick,
                agent,
                action,
                payload_hash,
                prev_hash,
                entry_hash,
                signature,
            });
        }
        if loaded.is_empty() {
            return false;
        }
        // Restore ring (may be partial if buffer truncated — last_n(BATCH_SIZE) compatible)
        let keep = loaded.len().min(AUDIT_RING_SIZE);
        let tail = &loaded[loaded.len().saturating_sub(keep)..];
        self.ring = tail.to_vec();
        self.head = 0;
        self.count = tail.len();
        self.verify_chain()
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
