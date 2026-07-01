//! Kanerva-style Sparse Distributed Memory para o Neural OS.
//! Extende MemoryTree com:
//! - Endereçamento por conteúdo (Hamming distance)
//! - Bayesian online update (prior → posterior)
//! - Distributed write (espalha por múltiplos endereços)

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use crate::memory_tree::{MemoryTree, MemNode, MemTier};
use core::cmp::Ordering;

/// Dimensão do endereço sparse (bits)
pub const ADDR_BITS: usize = 256;

/// Número de slots ativos por leitura
pub const K_NEAREST: usize = 5;

/// Threshold de distância de Hamming para match
pub const HAMMING_THRESHOLD: f32 = 0.3; // 30% dos bits podem diferir

/// Projeta um vetor f32 em um endereço binário sparse (256 bits)
pub fn project_to_address(data: &[f32]) -> [u8; ADDR_BITS / 8] {
    let mut addr = [0u8; ADDR_BITS / 8];
    let mut hash: u64 = 0x9E3779B97F4A7C15u64; // golden ratio
    for &val in data {
        let bits = val.to_bits();
        hash = hash.wrapping_mul(0x9E3779B9u64).wrapping_add(bits as u64);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xFF51AFD7ED558CCDu64);
        hash ^= hash >> 33;
    }
    for i in 0..(ADDR_BITS / 8) {
        let shift = (i as u64 * 8) % 64;
        addr[i] = (hash >> shift) as u8;
    }
    addr
}

/// Projeta uma string em um endereço binário
pub fn project_string(text: &str) -> [u8; ADDR_BITS / 8] {
    let floats: Vec<f32> = text.bytes().map(|b| b as f32 / 255.0).collect();
    project_to_address(&floats)
}

/// Distância de Hamming entre dois endereços (normalizada 0..1)
pub fn hamming_distance(a: &[u8; ADDR_BITS / 8], b: &[u8; ADDR_BITS / 8]) -> f32 {
    let mut diff = 0u32;
    for i in 0..(ADDR_BITS / 8) {
        let xor = a[i] ^ b[i];
        diff += xor.count_ones();
    }
    diff as f32 / ADDR_BITS as f32
}

/// Endereço Kanerva para um node
#[derive(Debug, Clone)]
pub struct KanervaSlot {
    pub node_idx: usize,
    pub address: [u8; ADDR_BITS / 8],
    pub access_count: u64,
    pub last_match: u64,
}

/// Memória Distribuída Kanerva — wrapper sobre MemoryTree
pub struct KanervaMemory {
    pub slots: Vec<KanervaSlot>,
    pub tick: u64,
}

impl KanervaMemory {
    pub fn new() -> Self {
        KanervaMemory { slots: Vec::new(), tick: 0 }
    }

    /// Registra um node da MemoryTree como slot Kanerva
    pub fn register_node(&mut self, idx: usize, tree: &MemoryTree) {
        if let Some(node) = tree.nodes.get(idx) {
            let addr = project_string(&node.summary);
            self.slots.push(KanervaSlot {
                node_idx: idx,
                address: addr,
                access_count: 0,
                last_match: self.tick,
            });
        }
    }

    /// Registra todos os nodes de uma árvore
    pub fn register_all(&mut self, tree: &MemoryTree) {
        for i in 0..tree.nodes.len() {
            if !self.slots.iter().any(|s| s.node_idx == i) {
                self.register_node(i, tree);
            }
        }
    }

    /// Leitura distribuída: encontra K slots mais próximos do query address
    pub fn sparse_read(&self, query_addr: &[u8; ADDR_BITS / 8]) -> Vec<(usize, f32)> {
        let mut matches: Vec<(usize, f32)> = self.slots.iter()
            .map(|s| (s.node_idx, hamming_distance(&s.address, query_addr)))
            .filter(|(_, d)| *d < HAMMING_THRESHOLD)
            .collect();
        matches.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        matches.truncate(K_NEAREST);
        matches
    }

    /// Leitura por conteúdo: string de busca → nodes mais similares
    pub fn query<'a>(&self, text: &str, tree: &'a MemoryTree) -> Vec<(usize, &'a str, f32)> {
        let addr = project_string(text);
        let matches = self.sparse_read(&addr);
        matches.into_iter().filter_map(|(idx, dist)| {
            tree.nodes.get(idx).map(|n| (idx, n.summary.as_str(), dist))
        }).collect()
    }

    /// Escrita distribuída: armazena dados em K slots próximos
    pub fn distributed_write(&mut self, text: &str, data: &[u8],
                              importance: u8, tier: MemTier, ttl: u64,
                              tree: &mut MemoryTree, parent: usize) {
        let addr = project_string(text);

        // Tenta encontrar slots existentes próximos para sobrescrever
        let matches = self.sparse_read(&addr);
        if !matches.is_empty() {
            // Atualiza o node mais próximo
            let (best_idx, _) = matches[0];
            if let Some(node) = tree.nodes.get_mut(best_idx) {
                node.data = data.to_vec();
                node.summary = String::from(text);
                node.importance = importance.max(node.importance);
                node.last_access_tick = self.tick;
                node.access_count += 1;
            }
            // Espalha réplicas parciais nos outros matches
            for (i, &(idx, _)) in matches.iter().enumerate().skip(1) {
                if i >= 3 { break; }
                let chunk_size = data.len() / 3;
                let start = (i - 1) * chunk_size;
                let end = start + chunk_size.min(data.len() - start);
                if let Some(node) = tree.nodes.get_mut(idx) {
                    node.data = data[start..end].to_vec();
                    node.summary = alloc::format!("{}_shard_{}", text, i);
                    node.last_access_tick = self.tick;
                }
            }
        } else {
            // Cria novo node e registra slot
            if let Some(idx) = tree.add(parent, text, data, importance, tier, ttl) {
                self.register_node(idx, tree);
            }
        }
    }

    /// Bayesian online update: ajusta importância baseado em frequência de match
    pub fn bayesian_update(&mut self, tree: &mut MemoryTree) {
        for slot in &self.slots {
            if let Some(node) = tree.nodes.get_mut(slot.node_idx) {
                let prior = node.importance as f32 / 10.0;
                let likelihood = (slot.access_count as f32) / (self.tick.max(1) as f32);
                let posterior = prior * likelihood;
                node.importance = (posterior * 10.0).min(10.0) as u8;
            }
        }
    }

    /// Avança tick e atualiza estatísticas
    pub fn tick_advance(&mut self, tree: &mut MemoryTree) {
        self.tick += 1;
        tree.tick = self.tick;
        if self.tick % 1000 == 0 {
            self.bayesian_update(tree);
        }
    }
}
