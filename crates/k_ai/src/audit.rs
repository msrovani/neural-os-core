//! Merkle Audit Trail — chain SHA-256 + Ed25519 por entry (#315.19).
//! Ring buffer 4096. Verifica chain e assinatura (session ou trusted).

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::identity::{sign_session, verify_trusted, SIGNATURE_LEN};
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

    pub fn verify(&self) -> bool {
        let mut prev = [0u8; 32];
        for entry in &self.ring {
            let mut combined = alloc::vec![0u8; 40];
            combined[..8].copy_from_slice(&entry.tick.to_le_bytes());
            combined[8..].copy_from_slice(&entry.payload_hash);
            let expected = hash_two(&prev, &combined);
            if expected != entry.entry_hash {
                return false;
            }
            if entry.prev_hash != prev {
                return false;
            }
            if entry.signature != [0u8; SIGNATURE_LEN]
                && !verify_trusted(&entry.entry_hash, &entry.signature)
            {
                return false;
            }
            prev = entry.entry_hash;
        }
        true
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
            self.verify()
        )
    }
}
