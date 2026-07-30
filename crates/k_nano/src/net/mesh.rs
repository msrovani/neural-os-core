//! Brain Mesh Engine — autonomous network clustering for distributed inference.
//!
//! ## Status: POC (proof of concept)
//! - Auto-discovery: requires UDP broadcast (e1000 + smoltcp)
//! - Master election: logic complete, transport pending
//! - Heartbeat: timer-based, needs APIC timer integration
//!
//! ## Integration
//! NetAgent::tick() → mesh.tick() → heartbeat → election → role assignment
//! HermesAgent uses mesh.node_role() for compute dispatch decisions
//!
//! ## Datacenter Vision
//! Cada AIOS vira um no em um cluster cognitivo global.
//! Nos se auto-descobrem, elegem mestres, distribuem inferencia.
//! Sem servidor central. Sem configuracao. Zero-touch.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use alloc::vec::Vec;

// ponytail: transport importado mas nao usado ate UDP broadcast estar pronto
// use crate::net::transport::HybridTransport;

/// SIMD capability weights for CapacityScore calculation
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdWeight {
    /// No SIMD
    None = 0,
    /// SSE4.2
    Sse42 = 1,
    /// AVX2
    Avx2 = 2,
    /// AVX-512
    Avx512 = 4,
}

impl SimdWeight {
    /// Get the weight as f32 for calculation
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Sse42 => 1.0,
            Self::Avx2 => 2.0,
            Self::Avx512 => 4.0,
        }
    }
}

/// Node capabilities descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NodeCapabilities {
    /// Node ID (MAC address or UUID)
    pub node_id: [u8; 6],
    /// Number of physical cores
    pub cores: u8,
    /// CPU clock in MHz
    pub clock_mhz: u32,
    /// RAM size in GB
    pub ram_gb: u32,
    /// L3 cache size in MB
    pub l3_cache_mb: u32,
    /// SIMD capability
    pub simd: SimdWeight,
    /// Has 3D V-Cache (AMD Zen 4X)
    pub has_3d_vcache: bool,
    /// Is this node anchored (has jarbas UI active)
    pub is_anchored: bool,
}

impl NodeCapabilities {
    /// Create a new node capabilities descriptor
    #[must_use]
    pub const fn new(
        node_id: [u8; 6],
        cores: u8,
        clock_mhz: u32,
        ram_gb: u32,
        l3_cache_mb: u32,
        simd: SimdWeight,
        has_3d_vcache: bool,
        is_anchored: bool,
    ) -> Self {
        Self {
            node_id,
            cores,
            clock_mhz,
            ram_gb,
            l3_cache_mb,
            simd,
            has_3d_vcache,
            is_anchored,
        }
    }

    /// Calculate CapacityScore
    ///
    /// Formula: (Cores × Clock) + (RAM_GB × SIMD_Weight) + L3_Cache_MB
    #[must_use]
    pub fn capacity_score(&self) -> f32 {
        let compute_score = self.cores as f32 * self.clock_mhz as f32;
        let memory_score = self.ram_gb as f32 * self.simd.as_f32();
        let cache_score = self.l3_cache_mb as f32;

        let mut score = compute_score + memory_score + cache_score;

        // Bonus for 3D V-Cache
        if self.has_3d_vcache {
            score *= 1.5;
        }

        // Bonus for anchored node (has UI)
        if self.is_anchored {
            score *= 1.2;
        }

        score
    }

    /// Get node ID as hex string (for display)
    #[must_use]
    pub fn node_id_hex(&self) -> [u8; 12] {
        let mut hex = [0u8; 12];
        for i in 0..6 {
            let b = self.node_id[i];
            hex[i * 2] = (b >> 4) + if b >> 4 < 10 { 48 } else { 55 };
            hex[i * 2 + 1] = (b & 0x0F) + if b & 0x0F < 10 { 48 } else { 55 };
        }
        hex
    }
}

/// Node role in the Brain Mesh
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    /// Master Node: runs hermes (meta-cognitive supervisor), jarbas (UI), global orchestration
    Master = 0,
    /// Memory Node: handles external memory VFS L0-L7 and fact/graph indexing
    Memory = 1,
    /// Compute Node: receives MoE experts for heavy parallel inference
    Compute = 2,
    /// Worker Node: small logical verification cells or async support tasks
    Worker = 3,
    /// Undecided: role not yet assigned
    Undecided = 4,
}

/// Brain Mesh node entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MeshNode {
    /// Node capabilities
    pub capabilities: NodeCapabilities,
    /// Current role
    pub role: NodeRole,
    /// Last heartbeat timestamp (ms)
    pub last_heartbeat: u64,
    /// Is this node online
    pub online: bool,
}

impl MeshNode {
    /// Create a new mesh node
    #[must_use]
    pub const fn new(capabilities: NodeCapabilities) -> Self {
        Self {
            capabilities,
            role: NodeRole::Undecided,
            last_heartbeat: 0,
            online: true,
        }
    }

    /// Update heartbeat timestamp
    pub fn update_heartbeat(&mut self, timestamp: u64) {
        self.last_heartbeat = timestamp;
    }

    /// Check if node is stale (no heartbeat for > 30 seconds)
    #[must_use]
    pub fn is_stale(&self, current_time: u64) -> bool {
        current_time.saturating_sub(self.last_heartbeat) > 30_000
    }
}

/// Brain Mesh Engine
///
/// Manages autonomous network clustering, discovery, and role assignment.
pub struct BrainMeshEngine {
    /// Local node capabilities
    local_capabilities: NodeCapabilities,
    /// Known nodes in the mesh (max 16 nodes)
    nodes: [Option<MeshNode>; 16],
    /// Number of known nodes
    node_count: AtomicU8,
    /// Local node role
    local_role: AtomicU8,
    /// Is this node the Master
    is_master: AtomicBool,
    /// Tick counter used as simple logical clock
    tick_count: u64,
    /// Discovery active flag
    discovery_active: AtomicBool,
}

impl BrainMeshEngine {
    /// Create a new Brain Mesh Engine
    #[must_use]
    pub const fn new(local_capabilities: NodeCapabilities) -> Self {
        const INIT: Option<MeshNode> = None;
        Self {
            local_capabilities,
            nodes: [INIT; 16],
            node_count: AtomicU8::new(0),
            local_role: AtomicU8::new(NodeRole::Undecided as u8),
            is_master: AtomicBool::new(false),
            tick_count: 0,
            discovery_active: AtomicBool::new(false),
        }
    }

    /// Advance the tick counter.
    /// Call from NetAgent::tick() or APIC timer.
    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
    }

    /// Start auto-discovery (Brain Beaconing).
    ///
    /// ponytail: transport layer (UDP broadcast over e1000) not production-ready.
    /// Currently only sets the flag — real broadcast requires smoltcp UDP socket.
    pub fn start_discovery(&self) {
        self.discovery_active.store(true, Ordering::Release);
        // ponytail: send UDP broadcast beacon here once transport is wired
    }

    /// Stop auto-discovery
    pub fn stop_discovery(&self) {
        self.discovery_active.store(false, Ordering::Release);
    }

    /// Build discovery packet for broadcast.
    ///
    /// ponytail: clock removed (LogicalClock module not restored yet);
    /// packet uses tick_count instead.
    fn build_discovery_packet(&self) -> Vec<u8> {
        // Discovery packet format:
        // [Magic: 4 bytes][Type: 1 byte][Tick: 8 bytes][Capabilities: NodeCapabilities]
        let mut packet = Vec::new();

        // Magic: "BMSH" (Brain Mesh)
        packet.extend_from_slice(&[0x42, 0x4D, 0x53, 0x48]);

        // Type: Discovery (0x01)
        packet.push(0x01);

        // Tick count (simple logical clock)
        packet.extend_from_slice(&self.tick_count.to_le_bytes());

        // Capabilities as raw bytes
        let caps = unsafe {
            let ptr = &self.local_capabilities as *const NodeCapabilities as *const [u8; core::mem::size_of::<NodeCapabilities>()];
            core::ptr::read(ptr)
        };
        packet.extend_from_slice(&caps);

        packet
    }

    /// Handle received discovery packet
    ///
    /// ponytail: transport pending; sender_mac could come from Ethernet header.
    pub fn handle_discovery(&mut self, packet: &[u8], _sender_mac: [u8; 6]) {
        if packet.len() < 13 {
            return;
        }

        // Check magic
        if &packet[0..4] != b"BMSH" {
            return;
        }

        // Check type
        if packet[4] != 0x01 {
            return;
        }

        // Extract tick count (reserved for clock sync when LogicalClock is restored)
        // let remote_tick = u64::from_le_bytes([packet[5], packet[6], packet[7], packet[8], packet[9], packet[10], packet[11], packet[12]]);

        // Extract capabilities
        let caps_offset = 13;
        if packet.len() < caps_offset + core::mem::size_of::<NodeCapabilities>() {
            return;
        }

        let caps = unsafe {
            let ptr = &packet[caps_offset] as *const u8 as *const NodeCapabilities;
            ptr.read()
        };

        // Add or update node
        self.add_or_update_node(caps);

        // Trigger re-election if needed
        self.check_election();
    }

    /// Add or update a node in the mesh
    pub fn add_or_update_node(&mut self, capabilities: NodeCapabilities) {
        let current_time = self.tick_count;

        // Check if node already exists
        for node in &mut self.nodes {
            if let Some(n) = node {
                if n.capabilities.node_id == capabilities.node_id {
                    n.capabilities = capabilities;
                    n.update_heartbeat(current_time);
                    return;
                }
            }
        }

        // Add new node if space available
        let count = self.node_count.load(Ordering::Acquire);
        if count < 16 {
            for node in &mut self.nodes {
                if node.is_none() {
                    *node = Some(MeshNode::new(capabilities));
                    self.node_count.fetch_add(1, Ordering::Release);
                    return;
                }
            }
        }
    }

    /// Check and perform Master election
    pub fn check_election(&mut self) {
        let local_score = self.local_capabilities.capacity_score();
        let mut max_score = local_score;
        let mut best_node: Option<usize> = None;

        // Find node with highest capacity score
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(n) = node {
                if n.online {
                    let score = n.capabilities.capacity_score();
                    if score > max_score {
                        max_score = score;
                        best_node = Some(i);
                    }
                }
            }
        }

        // Anchored node (with jarbas UI) always becomes Master
        if self.local_capabilities.is_anchored {
            self.become_master();
            return;
        }

        // Check if we should be Master
        if best_node.is_none() || max_score <= local_score {
            self.become_master();
        } else {
            self.become_worker();
        }
    }

    /// Become Master node
    fn become_master(&self) {
        self.is_master.store(true, Ordering::Release);
        self.local_role.store(NodeRole::Master as u8, Ordering::Release);

        // Assign roles to other nodes
        self.assign_roles();
    }

    /// Become Worker node
    fn become_worker(&self) {
        self.is_master.store(false, Ordering::Release);
        self.local_role.store(NodeRole::Worker as u8, Ordering::Release);
    }

    /// Assign roles to nodes based on capabilities
    ///
    /// Only the Master node performs role assignment.
    /// Simple heuristics: highest RAM → Memory, AVX2/AVX-512 → Compute.
    fn assign_roles(&self) {
        if !self.is_master.load(Ordering::Acquire) {
            return;
        }

        let mut memory_node: Option<usize> = None;
        let mut compute_nodes: Vec<usize> = Vec::new();

        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(n) = node {
                if n.online {
                    // Highest RAM becomes Memory Node
                    if memory_node.is_none()
                        || n.capabilities.ram_gb
                            > self.nodes[memory_node.unwrap()].as_ref().unwrap().capabilities.ram_gb
                    {
                        memory_node = Some(i);
                    }

                    // High SIMD becomes Compute Node
                    if n.capabilities.simd == SimdWeight::Avx512
                        || n.capabilities.simd == SimdWeight::Avx2
                    {
                        compute_nodes.push(i);
                    }
                }
            }
        }

        // ponytail: in real impl, send role-assignment messages to nodes
        let _ = memory_node;
        let _ = compute_nodes;
    }

    /// Get current timestamp (tick-based)
    fn get_timestamp(&self) -> u64 {
        // ponytail: replace with TSC read when available
        self.tick_count
    }

    /// Get local role
    #[must_use]
    pub fn local_role(&self) -> NodeRole {
        match self.local_role.load(Ordering::Acquire) {
            0 => NodeRole::Master,
            1 => NodeRole::Memory,
            2 => NodeRole::Compute,
            3 => NodeRole::Worker,
            _ => NodeRole::Undecided,
        }
    }

    /// Check if this node is Master
    #[must_use]
    pub fn is_master(&self) -> bool {
        self.is_master.load(Ordering::Acquire)
    }

    /// Get number of known nodes
    #[must_use]
    pub fn node_count(&self) -> u8 {
        self.node_count.load(Ordering::Acquire)
    }

    /// Get reference to the local node capabilities
    #[must_use]
    pub const fn local_capabilities(&self) -> &NodeCapabilities {
        &self.local_capabilities
    }

    /// Iterate over online nodes
    pub fn online_nodes(&self) -> impl Iterator<Item = &MeshNode> {
        self.nodes.iter().filter_map(|n| n.as_ref().filter(|n| n.online))
    }

    /// Clean up stale nodes (no heartbeat for > 30 s)
    pub fn cleanup_stale_nodes(&mut self) {
        let current_time = self.get_timestamp();

        for node in &mut self.nodes {
            if let Some(n) = node {
                if n.is_stale(current_time) {
                    *node = None;
                    self.node_count.fetch_sub(1, Ordering::Release);
                }
            }
        }
    }

    /// Main tick: heartbeat → cleanup → election.
    /// Call once per scheduler tick from NetAgent.
    pub fn step(&mut self) {
        self.tick();
        self.cleanup_stale_nodes();
        self.check_election();
    }
}
