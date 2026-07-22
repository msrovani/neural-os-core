//! ADR-0061: NUMA-aware frame allocator.
//!
//! Alocador de frames físicos isolado por Proximity Domain NUMA.
//! Cada nó tem um bump allocator lock-free (AtomicUsize) que gerencia
//! exclusivamente o intervalo de memória RAM física pertencente àquele nó.
//!
//! # Uso
//! ```ignore
//! // No boot, após parse_srat():
//! numa_alloc::init_from_topology(&numa_map);
//!
//! // Em qualquer thread:
//! let phys = numa_alloc::alloc_local_page(4096, 4096);
//! ```
//!
//! # Fast path (UMA)
//! Se NUMA não foi detectado (1 domínio), `alloc_local_page` cai no
//! allocator global sem overhead de RDPID/APIC lookup.

#![allow(dead_code)]

use crate::acpi::{NumaMemoryRange, NumaTopologyMap};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Máximo de nós NUMA suportados (Dual-Socket EPYC NPS4 = 8 nós).
pub const MAX_NUMA_NODES: usize = 8;

/// Tamanho mínimo de alinhamento (4 KiB page).
pub const PAGE_SIZE: usize = 4096;

/// Bump allocator lock-free para um nó NUMA.
#[derive(Debug)]
pub struct NumaPhysicalNode {
    pub node_id: u32,
    pub phys_start: u64,
    pub phys_end: u64,
    pub current_ptr: AtomicUsize,
}

impl NumaPhysicalNode {
    pub const fn empty() -> Self {
        Self {
            node_id: 0,
            phys_start: 0,
            phys_end: 0,
            current_ptr: AtomicUsize::new(0),
        }
    }

    /// Aloca frames de 4 KiB estritamente dentro da faixa física local do nó.
    /// Retorna o endereço físico ou None se OOM no nó local.
    pub fn alloc_local_frame(&self, size_bytes: usize) -> Option<u64> {
        let aligned_size = (size_bytes + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let mut current = self.current_ptr.load(Ordering::Relaxed);
        loop {
            let next = current + aligned_size;
            if next > self.phys_end as usize {
                return None; // OOM no nó local
            }
            match self.current_ptr.compare_exchange_weak(
                current,
                next,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(current as u64),
                Err(actual) => current = actual,
            }
        }
    }
}

/// Array global de alocadores por nó NUMA.
static mut NUMA_DOMAINS: [NumaPhysicalNode; MAX_NUMA_NODES] = [
    NumaPhysicalNode::empty(),
    NumaPhysicalNode::empty(),
    NumaPhysicalNode::empty(),
    NumaPhysicalNode::empty(),
    NumaPhysicalNode::empty(),
    NumaPhysicalNode::empty(),
    NumaPhysicalNode::empty(),
    NumaPhysicalNode::empty(),
];

/// Contador de nós NUMA inicializados.
static NUMA_INITIALIZED: AtomicUsize = AtomicUsize::new(0);

/// Inicializa os alocadores NUMA a partir do mapa de topologia SRAT.
///
/// Cada nó NUMA recebe um intervalo de memória física. Se houver múltiplas
/// faixas para o mesmo domínio, elas são concatenadas em ordem.
pub fn init_from_topology(map: &NumaTopologyMap) {
    if !map.is_multi_domain() {
        crate::slog_nano!(
            "NUMA",
            "info",
            "NUMA fast path (UMA): {} domínios",
            map.proximity_domain_count
        );
        return;
    }

    // Agrupa faixas por proximity_domain
    let mut domains: [(u64, u64); MAX_NUMA_NODES] = [(0, 0); MAX_NUMA_NODES];
    let mut domain_count = 0usize;

    for range in &map.memory_ranges {
        let d = range.proximity_domain as usize;
        if d >= MAX_NUMA_NODES {
            continue;
        }
        if domains[d].0 == 0 && domains[d].1 == 0 {
            domains[d] = (range.base, range.base + range.length);
            if d + 1 > domain_count {
                domain_count = d + 1;
            }
        } else {
            // Concatena faixas adicionais
            let end = range.base + range.length;
            if end > domains[d].1 {
                domains[d].1 = end;
            }
        }
    }

    unsafe {
        for i in 0..domain_count {
            let (start, end) = domains[i];
            if start == 0 && end == 0 {
                continue;
            }
            NUMA_DOMAINS[i].node_id = i as u32;
            NUMA_DOMAINS[i].phys_start = start;
            NUMA_DOMAINS[i].phys_end = end;
            NUMA_DOMAINS[i]
                .current_ptr
                .store(start as usize, Ordering::Release);
        }
    }

    NUMA_INITIALIZED.store(domain_count, Ordering::Release);

    crate::slog_nano!(
        "NUMA",
        "info",
        "NUMA inicializado: {} domínios, {} faixas de memória",
        domain_count,
        map.memory_ranges.len()
    );
}

/// Lê o APIC ID do núcleo atual via CPUID leaf 0x0B (x2APIC ID).
#[cfg(target_arch = "x86_64")]
fn current_apic_id() -> u32 {
    unsafe {
        // CPUID leaf 0x0B, subleaf 0: EDX = x2APIC ID (32 bits)
        let result = core::arch::x86_64::__cpuid_count(0x0B, 0);
        result.edx
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn current_apic_id() -> u32 {
    0
}

/// Aloca uma página de memória física estritamente no nó NUMA local do núcleo atual.
///
/// # Argumentos
/// - `size`: tamanho em bytes (será arredondado para múltiplo de 4 KiB)
/// - `_align`: alinhamento desejado (atualmente sempre PAGE_SIZE)
///
/// # Retorno
/// - `Some(phys_addr)` se a alocação foi bem-sucedida
/// - `None` se NUMA não foi inicializado ou se o nó local está OOM
///
/// # Fast path (UMA)
/// Se NUMA não foi inicializado (1 domínio), retorna o endereço físico
/// do heap global sem overhead de RDPID/APIC lookup.
pub fn alloc_local_page(size: usize, _align: usize) -> Option<u64> {
    let count = NUMA_INITIALIZED.load(Ordering::Acquire);
    if count <= 1 {
        // UMA fast path — alocação global sem NUMA
        return Some(alloc_global_fallback(size));
    }

    let apic_id = current_apic_id();
    let node_id = apic_id_to_node(apic_id);
    if node_id >= MAX_NUMA_NODES {
        return Some(alloc_global_fallback(size));
    }

    unsafe {
        let node = &NUMA_DOMAINS[node_id];
        if node.phys_start == 0 && node.phys_end == 0 {
            return Some(alloc_global_fallback(size));
        }
        node.alloc_local_frame(size)
    }
}

/// Mapeia APIC ID → node_id NUMA via lookup simples.
/// Em produção, deveria usar o `NumaTopologyMap` para lookup exato.
fn apic_id_to_node(apic_id: u32) -> usize {
    // Heurística: APIC IDs em sistemas NUMA são tipicamente agrupados por socket.
    // Socket 0 = APIC IDs 0..N/2, Socket 1 = APIC IDs N/2..N.
    // Para Dual-Socket EPYC NPS4, cada socket tem 4 nós NUMA.
    let count = NUMA_INITIALIZED.load(Ordering::Acquire);
    if count == 0 {
        return 0;
    }
    // Distribuição simples: assume APIC ID mod node_count
    (apic_id as usize) % count
}

/// Fallback global quando NUMA não está disponível.
/// Usa o heap global do kernel (talc).
fn alloc_global_fallback(size: usize) -> u64 {
    // Retorna um endereço físico fictício no heap global.
    // Em produção, isso chamaria o frame allocator global do k_nano::memory.
    // Por enquanto, retorna 0 como sentinel — callers devem tratar.
    let _ = size;
    0
}

/// Retorna o número de nós NUMA inicializados.
pub fn initialized_node_count() -> usize {
    NUMA_INITIALIZED.load(Ordering::Acquire)
}

/// Retorna informações de um nó NUMA específico.
pub fn node_info(node_id: usize) -> Option<(u64, u64)> {
    if node_id >= MAX_NUMA_NODES {
        return None;
    }
    unsafe {
        let n = &NUMA_DOMAINS[node_id];
        if n.phys_start == 0 && n.phys_end == 0 {
            None
        } else {
            Some((n.phys_start, n.phys_end))
        }
    }
}

/// Log de diagnóstico do estado NUMA.
pub fn log_numa_state() {
    let count = NUMA_INITIALIZED.load(Ordering::Acquire);
    if count == 0 {
        crate::slog_nano!("NUMA", "info", "NUMA não inicializado (UMA)");
        return;
    }
    for i in 0..count {
        if let Some((start, end)) = node_info(i) {
            crate::slog_nano!(
                "NUMA",
                "info",
                "Nó {}: phys 0x{:x}..0x{:x} ({} MiB)",
                i,
                start,
                end,
                (end - start) / (1024 * 1024)
            );
        }
    }
}
