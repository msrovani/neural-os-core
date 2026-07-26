//! Merkle Audit Trail (IDEA #315.19).
//! Hash chain de eventos com verificação de integridade.
//! Cada evento contém hash do anterior, formando uma corrente imutável.
//!
//! Chain structure:
//!   entry[0] = { hash=H(data[0]), prev=0 }
//!   entry[N] = { hash=H(data[N]), prev=hash[N-1] }

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::VecDeque;

/// Tamanho máximo do ring buffer de audit trail.
const MAX_ENTRIES: usize = 4096;

/// Um entry no audit trail.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub index: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub data_hash: [u8; 32],  // SHA-256 do payload
    pub prev_hash: [u8; 32],  // SHA-256 do entry anterior
}

/// Audit trail com verificação de integridade.
pub struct MerkleAuditTrail {
    entries: VecDeque<AuditEntry>,
    /// Hash do último entry (cabeça da corrente)
    pub last_hash: [u8; 32],
    /// Contador de entries
    next_index: u64,
}

impl MerkleAuditTrail {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_ENTRIES),
            last_hash: [0u8; 32],
            next_index: 0,
        }
    }

    /// Adiciona um evento ao trail.
    pub fn push(&mut self, event_type: &str, payload: &[u8], tick: u64) {
        let data_hash = simple_hash(payload);
        let prev_hash = self.last_hash;
        
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        
        let entry = AuditEntry {
            index: self.next_index,
            timestamp: tick,
            event_type: String::from(event_type),
            data_hash,
            prev_hash,
        };
        
        // Compute chain hash
        let mut chain_input = Vec::with_capacity(64 + 8 + event_type.len());
        chain_input.extend_from_slice(&data_hash);
        chain_input.extend_from_slice(&prev_hash);
        chain_input.extend_from_slice(tick.to_le_bytes().as_ref());
        self.last_hash = simple_hash(&chain_input);
        
        self.entries.push_back(entry);
        self.next_index += 1;
    }

    /// Verifica a integridade de toda a corrente.
    pub fn verify_chain(&self) -> bool {
        if self.entries.is_empty() {
            return true;
        }
        
        let mut expected_prev = [0u8; 32];
        for entry in &self.entries {
            // Verificar que prev_hash aponta para o entry anterior
            if entry.prev_hash != expected_prev {
                return false;
            }
            // Recomputar hash esperado
            let mut chain_input = Vec::new();
            chain_input.extend_from_slice(&entry.data_hash);
            chain_input.extend_from_slice(&entry.prev_hash);
            chain_input.extend_from_slice(entry.timestamp.to_le_bytes().as_ref());
            expected_prev = simple_hash(&chain_input);
        }
        
        // Verificar que last_hash corresponde ao último entry
        expected_prev == self.last_hash
    }

    /// Número de entries no trail.
    pub fn len(&self) -> usize { self.entries.len() }
    
    /// Retorna fatia dos entries mais recentes.
    pub fn recent(&self, n: usize) -> Vec<&AuditEntry> {
        self.entries.iter().rev().take(n).collect()
    }
}

/// SHA-256 simplificado (FNV-1a 64-bit para evitar dep).
/// Em produção, trocar por SHA-256 real.
fn simple_hash(data: &[u8]) -> [u8; 32] {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&h.to_le_bytes());
    out[8..16].copy_from_slice(&(!h).to_le_bytes());
    // Preenche o resto com rotações
    for i in 2..4 {
        let v = h.rotate_left((i * 16) as u32);
        out[i*8..(i+1)*8].copy_from_slice(&v.to_le_bytes());
    }
    out
}
