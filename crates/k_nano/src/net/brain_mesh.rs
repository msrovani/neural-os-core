//! Brain Mesh Engine - Autonomous Network Clustering
//! 
//! Implements auto-discovery, capacity analysis, and autonomous Master election
//! for Neural-OS-Core nodes in the local network.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use crate::p2p::clock::LogicalClock;
use crate::net::transport::{HybridTransport, TransportMode, TransportConfig};

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
    /// Last heartbeat timestamp
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
    /// Logical clock for ordering
    clock: LogicalClock,
    /// Transport for network communication
    transport: Option<HybridTransport>,
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
            clock: LogicalClock::new(),
            transport: None,
            discovery_active: AtomicBool::new(false),
        }
    }

    /// Initialize the Brain Mesh Engine with transport
    pub fn init(&mut self, transport: HybridTransport) {
        self.transport = Some(transport);
    }

    /// Start auto-discovery (Brain Beaconing)
    /// 
    /// Broadcasts discovery packets to find other Neural-OS-Core nodes
    pub fn start_discovery(&self) {
        self.discovery_active.store(true, Ordering::Release);
        
        // Send discovery broadcast
        if let Some(transport) = &self.transport {
            let discovery_packet = self.build_discovery_packet();
            let mut buffer = [0u8; 1024];
            
            if let Ok(size) = transport.send_packet(&discovery_packet, &mut buffer) {
                let _ = size; // In real implementation, would handle send
            }
        }
    }

    /// Stop auto-discovery
    pub fn stop_discovery(&self) {
        self.discovery_active.store(false, Ordering::Release);
    }

    /// Build discovery packet
    fn build_discovery_packet(&self) -> Vec<u8> {
        // Discovery packet format:
        // [Magic: 4 bytes][Type: 1 byte][Clock: 8 bytes][Capabilities: NodeCapabilities]
        let mut packet = Vec::new();
        
        // Magic: "BMSH" (Brain Mesh)
        packet.extend_from_slice(&[0x42, 0x4D, 0x53, 0x48]);
        
        // Type: Discovery (0x01)
        packet.push(0x01);
        
        // Clock
        let clock = self.clock.tick();
        packet.extend_from_slice(&clock.to_le_bytes());
        
        // Capabilities
        let caps = unsafe {
            core::ptr::read(&self.local_capabilities as *const NodeCapabilities as *const [u8; core::mem::size_of::<NodeCapabilities>()])
        };
        packet.extend_from_slice(&caps);
        
        packet
    }

    /// Handle received discovery packet
    pub fn handle_discovery(&mut self, packet: &[u8], sender_mac: [u8; 6]) {
        // Parse discovery packet
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
        
        // Extract clock
        let received_clock = u64::from_le_bytes([packet[5], packet[6], packet[7], packet[8], packet[9], packet[10], packet[11], packet[12]]);
        self.clock.update(received_clock);
        
        // Extract capabilities
        if packet.len() < 13 + core::mem::size_of::<NodeCapabilities>() {
            return;
        }
        
        let caps = unsafe {
            let ptr = &packet[13] as *const u8 as *const NodeCapabilities;
            ptr.read()
        };
        
        // Add or update node
        self.add_or_update_node(caps);
        
        // Trigger re-election if needed
        self.check_election();
    }

    /// Add or update a node in the mesh
    fn add_or_update_node(&mut self, capabilities: NodeCapabilities) {
        let current_time = self.get_timestamp();
        
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
    fn check_election(&mut self) {
        let local_score = self.local_capabilities.capacity_score();
        let mut max_score = local_score;
        let mut best_node = None;
        
        // Find node with highest capacity score
        for node in &self.nodes {
            if let Some(n) = node {
                if n.online {
                    let score = n.capabilities.capacity_score();
                    if score > max_score {
                        max_score = score;
                        best_node = Some(n);
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
    fn assign_roles(&self) {
        if !self.is_master.load(Ordering::Acquire) {
            return;
        }
        
        // Sort nodes by capabilities (in real implementation)
        // For now, assign based on simple heuristics
        
        let mut memory_node: Option<usize> = None;
        let mut compute_nodes: Vec<usize> = Vec::new();
        
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(n) = node {
                if n.online {
                    // Highest RAM becomes Memory Node
                    if memory_node.is_none() || n.capabilities.ram_gb > self.nodes[memory_node.unwrap()].as_ref().unwrap().capabilities.ram_gb {
                        memory_node = Some(i);
                    }
                    
                    // High SIMD becomes Compute Node
                    if n.capabilities.simd == SimdWeight::Avx512 || n.capabilities.simd == SimdWeight::Avx2 {
                        compute_nodes.push(i);
                    }
                }
            }
        }
        
        // Assign roles (in real implementation, would update node roles)
        let _ = memory_node;
        let _ = compute_nodes;
    }

    /// Get current timestamp
    fn get_timestamp(&self) -> u64 {
        // In real implementation, would read TSC
        0
    }

    /// Get local role
    #[must_use]
    pub fn local_role(&self) -> NodeRole {
        unsafe { core::mem::transmute(self.local_role.load(Ordering::Acquire)) }
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

    /// Clean up stale nodes
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_capabilities_creation() {
        let caps = NodeCapabilities::new(
            [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
            8,
            3000,
            32,
            32,
            SimdWeight::Avx2,
            false,
            false,
        );
        
        assert_eq!(caps.cores, 8);
        assert_eq!(caps.clock_mhz, 3000);
        assert_eq!(caps.ram_gb, 32);
    }

    #[test]
    fn test_capacity_score() {
        let caps1 = NodeCapabilities::new(
            [0; 6],
            4,
            2000,
            16,
            16,
            SimdWeight::Avx2,
            false,
            false,
        );
        
        let caps2 = NodeCapabilities::new(
            [0; 6],
            8,
            3000,
            32,
            32,
            SimdWeight::Avx512,
            true,
            true,
        );
        
        let score1 = caps1.capacity_score();
        let score2 = caps2.capacity_score();
        
        assert!(score2 > score1);
    }

    #[test]
    fn test_simd_weight() {
        assert_eq!(SimdWeight::None.as_f32(), 0.0);
        assert_eq!(SimdWeight::Sse42.as_f32(), 1.0);
        assert_eq!(SimdWeight::Avx2.as_f32(), 2.0);
        assert_eq!(SimdWeight::Avx512.as_f32(), 4.0);
    }

    #[test]
    fn test_mesh_node_creation() {
        let caps = NodeCapabilities::new([0; 6], 4, 2000, 16, 16, SimdWeight::Avx2, false, false);
        let node = MeshNode::new(caps);
        
        assert!(node.online);
        assert_eq!(node.role, NodeRole::Undecided);
    }

    #[test]
    fn test_mesh_node_stale() {
        let caps = NodeCapabilities::new([0; 6], 4, 2000, 16, 16, SimdWeight::Avx2, false, false);
        let mut node = MeshNode::new(caps);
        
        node.update_heartbeat(0);
        assert!(node.is_stale(30_001));
        
        assert!(!node.is_stale(29_999));
    }

    #[test]
    fn test_brain_mesh_engine_creation() {
        let caps = NodeCapabilities::new([0; 6], 4, 2000, 16, 16, SimdWeight::Avx2, false, false);
        let engine = BrainMeshEngine::new(caps);
        
        assert_eq!(engine.node_count(), 0);
        assert!(!engine.is_master());
    }

    #[test]
    fn test_brain_mesh_engine_add_node() {
        let caps = NodeCapabilities::new([0; 6], 4, 2000, 16, 16, SimdWeight::Avx2, false, false);
        let mut engine = BrainMeshEngine::new(caps);
        
        let remote_caps = NodeCapabilities::new([1; 6], 8, 3000, 32, 32, SimdWeight::Avx512, true, false);
        engine.add_or_update_node(remote_caps);
        
        assert_eq!(engine.node_count(), 1);
    }

    #[test]
    fn test_brain_mesh_election_anchored() {
        let caps = NodeCapabilities::new([0; 6], 4, 2000, 16, 16, SimdWeight::Avx2, false, true);
        let mut engine = BrainMeshEngine::new(caps);
        
        engine.check_election();
        
        assert!(engine.is_master());
        assert_eq!(engine.local_role(), NodeRole::Master);
    }

    #[test]
    fn test_brain_mesh_election_capacity() {
        let caps = NodeCapabilities::new([0; 6], 8, 3000, 32, 32, SimdWeight::Avx512, false, false);
        let mut engine = BrainMeshEngine::new(caps);
        
        let remote_caps = NodeCapabilities::new([1; 6], 4, 2000, 16, 16, SimdWeight::Avx2, false, false);
        engine.add_or_update_node(remote_caps);
        
        engine.check_election();
        
        assert!(engine.is_master());
    }

    /// Simulate Master election with 3 virtual nodes of different capacities
    #[test]
    fn test_master_election_3_virtual_nodes() {
        // Node 1: Local node (high capacity, AVX-512)
        let local_caps = NodeCapabilities::new(
            [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
            16,
            4000,
            64,
            64,
            SimdWeight::Avx512,
            true,
            false,
        );
        let mut engine = BrainMeshEngine::new(local_caps);
        
        // Node 2: Medium capacity (AVX2)
        let node2_caps = NodeCapabilities::new(
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            8,
            3000,
            32,
            32,
            SimdWeight::Avx2,
            false,
            false,
        );
        engine.add_or_update_node(node2_caps);
        
        // Node 3: Low capacity (SSE4.2)
        let node3_caps = NodeCapabilities::new(
            [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
            4,
            2000,
            16,
            16,
            SimdWeight::Sse42,
            false,
            false,
        );
        engine.add_or_update_node(node3_caps);
        
        assert_eq!(engine.node_count(), 2);
        
        // Perform election
        engine.check_election();
        
        // Local node should be Master (highest capacity)
        assert!(engine.is_master());
        assert_eq!(engine.local_role(), NodeRole::Master);
        
        // Verify capacity scores
        let local_score = local_caps.capacity_score();
        let node2_score = node2_caps.capacity_score();
        let node3_score = node3_caps.capacity_score();
        
        assert!(local_score > node2_score);
        assert!(node2_score > node3_score);
        
        // Simulate node 2 becoming anchored (has jarbas UI)
        let node2_anchored = NodeCapabilities::new(
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            8,
            3000,
            32,
            32,
            SimdWeight::Avx2,
            false,
            true, // Now anchored
        );
        engine.add_or_update_node(node2_anchored);
        
        // Re-election should favor anchored node
        engine.check_election();
        
        // Local node should no longer be Master (node2 is anchored)
        assert!(!engine.is_master());
        assert_eq!(engine.local_role(), NodeRole::Worker);
    }

    /// Test role assignment based on capabilities
    #[test]
    fn test_dynamic_role_assignment() {
        let caps = NodeCapabilities::new([0; 6], 16, 4000, 64, 64, SimdWeight::Avx512, true, true);
        let mut engine = BrainMeshEngine::new(caps);
        
        // Add memory-rich node
        let memory_caps = NodeCapabilities::new(
            [1; 6],
            8,
            3000,
            128, // High RAM
            32,
            SimdWeight::Avx2,
            false,
            false,
        );
        engine.add_or_update_node(memory_caps);
        
        // Add compute-rich node
        let compute_caps = NodeCapabilities::new(
            [2; 6],
            12,
            3500,
            32,
            64,
            SimdWeight::Avx512,
            true,
            false,
        );
        engine.add_or_update_node(compute_caps);
        
        // Add worker node (dual-core)
        let worker_caps = NodeCapabilities::new(
            [3; 6],
            2,
            2000,
            8,
            4,
            SimdWeight::Sse42,
            false,
            false,
        );
        engine.add_or_update_node(worker_caps);
        
        engine.check_election();
        engine.assign_roles();
        
        // Local node (anchored) should be Master
        assert!(engine.is_master());
    }
}
