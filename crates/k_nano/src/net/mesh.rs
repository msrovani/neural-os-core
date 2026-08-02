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

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use alloc::string::String;
use alloc::vec::Vec;
use event_bus::{Event, CapabilityToken};
use crate::net::noproto::{AiosTaskPacket, TaskType, PacketFlags};
use crate::identity::PUBLIC_KEY_LEN;

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

        // Find node with highest capacity score.
        // Tie-break (SESSION_234): score igual → menor node_id vence — eleição
        // determinística entre instâncias idênticas (antes: ambas Master).
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(n) = node {
                if n.online {
                    let score = n.capabilities.capacity_score();
                    let beats = match best_node {
                        None => {
                            // Peer só vence o local se score maior, ou score
                            // igual com node_id menor (unicidade por IP).
                            score > local_score
                                || (score == local_score
                                    && n.capabilities.node_id < self.local_capabilities.node_id)
                        }
                        Some(bi) => {
                            if let Some(b) = self.nodes[bi].as_ref() {
                                score > b.capabilities.capacity_score()
                                    || (score == b.capabilities.capacity_score()
                                        && n.capabilities.node_id < b.capabilities.node_id)
                            } else {
                                false
                            }
                        }
                    };
                    if beats {
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

        // Check if we should be Master: Master = nenhum peer venceu o local.
        if best_node.is_none() {
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

    /// Aplica papel atribuído pelo Master (propagação via ROLE\0{node}\0{role}).
    /// SESSION_235: receptor filtra pelo node_id e aplica no MESH_ENGINE.
    pub fn set_role(&self, role: NodeRole) {
        self.local_role.store(role as u8, Ordering::Release);
        crate::slog_nano!("P2P", "info", "role aplicado: {:?}", role);
    }

    /// Assign roles to nodes based on capabilities
    ///
    /// Only the Master node performs role assignment.
    /// Simple heuristics: highest RAM → Memory, AVX2/AVX-512 → Compute.
    /// SESSION_235: envia ROLE\0{node_id}\0{role_u8} via broadcast para cada
    /// nó conhecido (transporte não tem unicast — receptor filtra pelo node_id).
    fn assign_roles(&self) {
        if !self.is_master.load(Ordering::Acquire) {
            return;
        }

        // Throttle ~110 ticks (espelha o heartbeat): check_election roda a cada
        // tick via mesh_tick + a cada heartbeat RX — sem throttle, spam de ROLE.
        static LAST_ASSIGN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        let last = LAST_ASSIGN.load(Ordering::Relaxed);
        if last != 0 && now.wrapping_sub(last) < 110 {
            return;
        }
        LAST_ASSIGN.store(now, Ordering::Relaxed);

        let mut memory_node: Option<usize> = None;
        let mut compute_nodes: Vec<usize> = Vec::new();

        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(n) = node {
                if n.online {
                    // Capacidades anunciadas no heartbeat (CAP\0) do peer.
                    let caps = peer_caps(n.capabilities.node_id[0]).unwrap_or(0);
                    // Memory ← maior RAM OU bit memory anunciado (CAP_MEMORY).
                    if (caps & CAP_MEMORY) != 0
                        || memory_node.is_none()
                        || n.capabilities.ram_gb
                            > self.nodes[memory_node.unwrap()].as_ref().unwrap().capabilities.ram_gb
                    {
                        memory_node = Some(i);
                    }
                    // Compute ← SIMD AVX2/AVX-512 OU bit compute (CAP_COMPUTE).
                    if (caps & CAP_COMPUTE) != 0
                        || n.capabilities.simd == SimdWeight::Avx512
                        || n.capabilities.simd == SimdWeight::Avx2
                    {
                        compute_nodes.push(i);
                    }
                }
            }
        }

        // Envia papel para cada nó conhecido (broadcast — filtro no receptor).
        for (i, node) in self.nodes.iter().enumerate() {
            let Some(n) = node else { continue };
            if !n.online {
                continue;
            }
            // node_id do peer = primeiro byte das capabilities (source_id do
            // heartbeat que o registrou: sender_mac = [source_id, 0,0,0,0,0]).
            let target = n.capabilities.node_id[0];
            let role = if Some(i) == memory_node {
                NodeRole::Memory
            } else if compute_nodes.contains(&i) {
                NodeRole::Compute
            } else {
                NodeRole::Worker
            };
            self.send_role_assign(target, role);
        }
    }

    /// Envia um pacote NoProto Sync com payload "ROLE\0{target}\0{role_u8}"
    /// via broadcast (porta 42069). Receptor filtra pelo target.
    /// Fase A (SESSION_236): assinado — o RX fail-closed dropa não-assinados.
    fn send_role_assign(&self, target_node: u8, role: NodeRole) {
        let pkt = AiosTaskPacket::new(
            0, node_id(), 0xFF, TaskType::Sync, 1, 0, 0, PacketFlags::new(),
        );
        let mut buf = crate::net::udp_broadcast::serialize(&pkt);
        let payload = alloc::format!("ROLE\0{}\0{}", target_node, role as u8).into_bytes();
        buf.extend_from_slice(&payload);
        let Some(signed) = crate::net::udp_broadcast::sign_packet(&buf) else {
            crate::slog_nano!("P2P", "warn", "role-assign skip: sessao nao inicializada node={}", target_node);
            return;
        };
        let ok = crate::net::udp_broadcast::udp_broadcast_send(&signed, 42069);
        crate::slog_nano!("P2P", "info", "role-assign node={} role={:?} sent={}", target_node, role, ok);
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

/// Static global mesh engine instance.
/// Inicializado via `mesh::init(caps)` no boot, depois chamado via `mesh_tick()`.
use spin::Mutex;
pub static MESH_ENGINE: Mutex<Option<BrainMeshEngine>> = Mutex::new(None);

/// Inicializa o mesh engine com as capacidades locais.
pub fn init(caps: NodeCapabilities) {
    *MESH_ENGINE.lock() = Some(BrainMeshEngine::new(caps));
}

/// Tick do mesh: chamado pelo NetAgent a cada ciclo do scheduler.
/// Executa heartbeat logico, cleanup de nos mortos, e re-eleicao.
pub fn mesh_tick() {
    if let Some(ref mut engine) = *MESH_ENGINE.lock() {
        engine.step();
    }
}

/// Retorna o papel local no mesh (Master, Worker, etc).
pub fn local_role() -> NodeRole {
    MESH_ENGINE.lock()
        .as_ref()
        .map(|e| e.local_role())
        .unwrap_or(NodeRole::Undecided)
}

/// ID único por instância — usado como source_id nos pacotes NoProto
/// (heartbeat/Sync/offers) e no node_id das NodeCapabilities do mesh.
///
/// SESSION_234: `local_role() as u8` colidia entre instâncias (ambas
/// enviavam o mesmo ID, ex. Undecided=4) → add_or_update_node deduplicava
/// pelo node_id → nodes=1 mesmo com 2 instâncias. Deriva do IP real
/// (10.0.3.2→2, .3→3) para unicidade; fallbacks: MAC byte 5, depois role.
pub fn node_id() -> u8 {
    let cfg = crate::nic_globals::NET_CONFIG.lock();
    if cfg.ip[3] != 0 {
        cfg.ip[3]
    } else if cfg.mac[5] != 0 {
        cfg.mac[5]
    } else {
        local_role() as u8
    }
}

// ─── Fase A de segurança do mesh (ADR-0081, SESSION_236) ───────────────────
// Tabela de peers TOFU: a 1ª assinatura válida de um node_id vincula a pk da
// sessão dele. Seam SKYNET: futuramente a tabela pode ser pré-preenchida por
// TEE attestation sem mudar o mesh — o TX/RX só consulta `peer_*`.

/// Slots de peers: (node_id, public_key, last_clock). Slot livre = None.
/// Tamanho 16 = máximo de nós do mesh (BrainMeshEngine).
static PEER_KEYS: Mutex<[Option<(u8, [u8; PUBLIC_KEY_LEN], u64)>; 16]> = Mutex::new([None; 16]);

/// Chave pública vinculada a um peer. `None` = desconhecido (nunca visto).
fn peer_pk(node_id: u8) -> Option<[u8; PUBLIC_KEY_LEN]> {
    let table = PEER_KEYS.lock();
    for slot in table.iter() {
        if let Some((nid, pk, _)) = slot {
            if *nid == node_id {
                return Some(*pk);
            }
        }
    }
    None
}

/// Vincula node_id → pk (TOFU). Atualiza se já existir; senão, primeiro slot livre.
fn peer_bind(node_id: u8, pk: [u8; PUBLIC_KEY_LEN]) {
    let mut table = PEER_KEYS.lock();
    for slot in table.iter_mut() {
        if let Some((nid, _, _)) = slot {
            if *nid == node_id {
                *slot = Some((node_id, pk, 0));
                return;
            }
        }
    }
    for slot in table.iter_mut() {
        if slot.is_none() {
            *slot = Some((node_id, pk, 0));
            return;
        }
    }
}

/// Último clock aceito de um peer (anti-replay do canal de heartbeat).
fn peer_last_clock(node_id: u8) -> Option<u64> {
    let table = PEER_KEYS.lock();
    for slot in table.iter() {
        if let Some((nid, _, clk)) = slot {
            if *nid == node_id {
                return Some(*clk);
            }
        }
    }
    None
}

/// Atualiza o clock aceito de um peer.
fn peer_update_clock(node_id: u8, clk: u64) {
    let mut table = PEER_KEYS.lock();
    for slot in table.iter_mut() {
        if let Some((nid, _, last)) = slot {
            if *nid == node_id {
                *last = clk;
                return;
            }
        }
    }
}

/// Acesso público à tabela TOFU — usado pelo cortex (Worker verifica a
/// resposta "MR" do Master) e pela futura TEE attestation (SKYNET seam).
pub fn peer_public_key(node_id: u8) -> Option<[u8; PUBLIC_KEY_LEN]> {
    peer_pk(node_id)
}

// Contadores de segurança (diagnóstico).
static SEC_DROPPED_UNSIGNED: AtomicU64 = AtomicU64::new(0);
static SEC_DROPPED_BADSIG: AtomicU64 = AtomicU64::new(0);
static SEC_DROPPED_REPLAY: AtomicU64 = AtomicU64::new(0);

/// (unsigned, badsig, replay) — drops de segurança do mesh para diagnóstico.
pub fn sec_stats() -> (u64, u64, u64) {
    (
        SEC_DROPPED_UNSIGNED.load(Ordering::Relaxed),
        SEC_DROPPED_BADSIG.load(Ordering::Relaxed),
        SEC_DROPPED_REPLAY.load(Ordering::Relaxed),
    )
}

// ─── Fase B: capacidades locais + peers (ADR-0081) ─────────────────────────
// Bitmask anunciado no heartbeat ("CAP\0" após a pk). Definido em k_nano; o
// bin/hermes setam via `set_local_caps` conforme o HW detectado no boot.

/// bit0: LLM (inferência local habilitada)
pub const CAP_LLM: u8 = 1 << 0;
/// bit1: expert RustCoder (geração de código)
pub const CAP_RUSTCODER: u8 = 1 << 1;
/// bit2: expert HwExpert (identificação de HW)
pub const CAP_HWEXPERT: u8 = 1 << 2;
/// bit3: compute (aceita dispatches de matmul distribuído)
pub const CAP_COMPUTE: u8 = 1 << 3;
/// bit4: memory (candidato a Memory node — VFS L0-L7/fact-graph)
pub const CAP_MEMORY: u8 = 1 << 4;
/// bit5: SGDB pronto (store cognitivo operacional)
pub const CAP_SGDB_READY: u8 = 1 << 5;

/// Capacidades locais anunciadas no heartbeat. 0 = não anuncia (default).
static LOCAL_CAPS: AtomicU8 = AtomicU8::new(0);

/// Define as capacidades locais anunciadas no heartbeat (bitmask CAP_*).
pub fn set_local_caps(bits: u8) {
    LOCAL_CAPS.store(bits, Ordering::Release);
    crate::slog_nano!("P2P", "info", "local caps set bits=0x{:02X}", bits);
}

/// Capacidades locais atuais (0 = nenhuma anunciada).
pub fn local_caps() -> u8 {
    LOCAL_CAPS.load(Ordering::Acquire)
}

/// Capacidades anunciadas por peer (via "CAP\0" no heartbeat): (node_id, caps).
static PEER_CAPS: Mutex<[Option<(u8, u8)>; 16]> = Mutex::new([None; 16]);

/// Capacidades anunciadas por um peer. `None` = nunca viu heartbeat com CAP.
pub fn peer_caps(node_id: u8) -> Option<u8> {
    let table = PEER_CAPS.lock();
    for slot in table.iter() {
        if let Some((nid, caps)) = slot {
            if *nid == node_id {
                return Some(*caps);
            }
        }
    }
    None
}

/// Armazena/atualiza as capacidades anunciadas por um peer.
fn peer_set_caps(node_id: u8, caps: u8) {
    let mut table = PEER_CAPS.lock();
    for slot in table.iter_mut() {
        if let Some((nid, _)) = slot {
            if *nid == node_id {
                *slot = Some((node_id, caps));
                return;
            }
        }
    }
    for slot in table.iter_mut() {
        if slot.is_none() {
            *slot = Some((node_id, caps));
            return;
        }
    }
}

// ─── Fase B: chunking de payloads grandes (CHK\0, ADR-0081) ─────────────────
// Payloads > CHUNK_DATA_MAX são fatiados em chunks Sync assinados:
//   "CHK\0" + msg_id u32 LE + seq u16 LE + total u16 LE + [dados do chunk]
// O RX remonta por (source_id, msg_id) e publica o payload original no
// TOPIC_P2P_PACKET (mesmo Event de um pacote pequeno). TOFU/anti-replay
// inalterados — cada chunk passa pelo caminho de verificação normal.

/// Bytes de dados por chunk (≤1100 — folga no MTU UDP/Ethernet 1500).
const CHUNK_DATA_MAX: usize = 1100;
/// Limite total por slot de remontagem (~64KB).
const CHUNK_SLOT_MAX_BYTES: usize = 65536;
/// Número de slots de remontagem (mensagens simultâneas por fonte).
const CHUNK_SLOTS: usize = 4;
/// Timeout de slot incompleto (~5s a 100Hz de TIMER_TICKS).
const CHUNK_TIMEOUT_TICKS: u64 = 500;

/// msg_id global — incrementa a cada `mesh_send_large` (wrap u32 aceito).
static CHUNK_MSG_ID: AtomicU32 = AtomicU32::new(1);

/// Slot de remontagem de uma mensagem chunked (por (source, msg_id)).
struct ChunkSlot {
    source: u8,
    msg_id: u32,
    total: u16,
    last_seen: u64,
    chunks: Vec<Option<Vec<u8>>>,
}

/// Tabela de slots de remontagem (estática, estilo PEER_KEYS).
static CHUNK_SLOTS_TABLE: Mutex<[Option<ChunkSlot>; CHUNK_SLOTS]> = Mutex::new([const { None }; CHUNK_SLOTS]);

/// Envia payload como pacote Sync assinado via broadcast (porta 42069).
/// Mesmo caminho do ROLE\0 — fail-closed sem sessão.
fn send_sync_payload(payload: &[u8]) -> bool {
    let pkt = AiosTaskPacket::new(
        0, node_id(), 0xFF, TaskType::Sync, 1, 0, 0, PacketFlags::new(),
    );
    let mut buf = crate::net::udp_broadcast::serialize(&pkt);
    buf.extend_from_slice(payload);
    let Some(signed) = crate::net::udp_broadcast::sign_packet(&buf) else {
        crate::slog_nano!("P2P", "warn", "send_sync_payload skip: sessao nao inicializada");
        return false;
    };
    crate::net::udp_broadcast::udp_broadcast_send(&signed, 42069)
}

/// Envia payload via mesh P2P. Payloads ≤ CHUNK_DATA_MAX vão direto como
/// pacote Sync assinado (como hoje); maiores são fatiados em chunks "CHK\0"
/// e remontados no RX. Retorna false se qualquer envio falhou.
pub fn mesh_send_large(payload: &[u8]) -> bool {
    if payload.len() <= CHUNK_DATA_MAX {
        return send_sync_payload(payload);
    }
    let msg_id = CHUNK_MSG_ID.fetch_add(1, Ordering::Relaxed);
    let total = ((payload.len() + CHUNK_DATA_MAX - 1) / CHUNK_DATA_MAX) as u16;
    let mut ok = true;
    for (seq, chunk) in payload.chunks(CHUNK_DATA_MAX).enumerate() {
        let mut body = Vec::with_capacity(12 + chunk.len());
        body.extend_from_slice(b"CHK\0");
        body.extend_from_slice(&msg_id.to_le_bytes());
        body.extend_from_slice(&(seq as u16).to_le_bytes());
        body.extend_from_slice(&total.to_le_bytes());
        body.extend_from_slice(chunk);
        let sent = send_sync_payload(&body);
        ok = ok && sent;
        crate::slog_nano!("P2P", "info", "TX chunk msg={} seq={}/{} node={} bytes={} sent={}", msg_id, seq, total, node_id(), chunk.len(), sent);
    }
    ok
}

/// Processa um payload "CHK\0" recebido: insere o chunk no slot
/// (source_id, msg_id) e, quando o último chega, retorna o payload completo
/// (sem prefixo CHK). Descarta slots incompletos velhos (timeout por ticks).
fn chunk_reassemble(sid: u8, payload: &[u8], now: u64) -> Option<Vec<u8>> {
    if payload.len() < 12 || &payload[0..4] != b"CHK\0" {
        return None;
    }
    let msg_id = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
    let seq = u16::from_le_bytes([payload[8], payload[9]]) as usize;
    let total = u16::from_le_bytes([payload[10], payload[11]]) as usize;
    let chunk = &payload[12..];
    // M1 (oracle): cap no total ANTES de alocar o slot — total * CHUNK_DATA_MAX
    // > CHUNK_SLOT_MAX_BYTES (ex. total=65535) alocaria ~1.5MB por slot e
    // permitiria DoS de remontagem por peer TOFU. Validação também cobre o
    // branch de slot existente (msg_id reutilizado com total diferente).
    if total == 0
        || seq >= total
        || chunk.len() > CHUNK_DATA_MAX
        || total.saturating_mul(CHUNK_DATA_MAX) > CHUNK_SLOT_MAX_BYTES
    {
        return None;
    }

    let mut table = CHUNK_SLOTS_TABLE.lock();
    // Sweep: descarta slots incompletos velhos (timeout por ticks).
    for slot in table.iter_mut() {
        if let Some(s) = slot {
            if now.wrapping_sub(s.last_seen) > CHUNK_TIMEOUT_TICKS {
                crate::slog_nano!("P2P", "warn", "chunk slot timeout node={} msg={}", s.source, s.msg_id);
                *slot = None;
            }
        }
    }
    // Acha slot existente para (sid, msg_id) ou um slot livre.
    let mut idx: Option<usize> = None;
    for (i, slot) in table.iter().enumerate() {
        if let Some(s) = slot {
            if s.source == sid && s.msg_id == msg_id {
                idx = Some(i);
                break;
            }
        }
    }
    if idx.is_none() {
        for (i, slot) in table.iter().enumerate() {
            if slot.is_none() {
                idx = Some(i);
                break;
            }
        }
    }
    let i = idx?;
    if table[i].is_none() {
        let mut chunks = alloc::vec![None; total];
        chunks[seq] = Some(chunk.to_vec());
        table[i] = Some(ChunkSlot { source: sid, msg_id, total: total as u16, last_seen: now, chunks });
        return None;
    }
    {
        let slot = table[i].as_mut().unwrap();
        if slot.total != total as u16 {
            // msg_id reutilizado com tamanho diferente — reinicia o slot.
            slot.chunks = alloc::vec![None; total];
            slot.total = total as u16;
        }
        let used: usize = slot.chunks.iter().filter_map(|c| c.as_ref().map(Vec::len)).sum();
        if used + chunk.len() > CHUNK_SLOT_MAX_BYTES {
            crate::slog_nano!("P2P", "warn", "chunk slot overflow node={} msg={}", sid, msg_id);
            table[i] = None;
            return None;
        }
        slot.chunks[seq] = Some(chunk.to_vec());
        slot.last_seen = now;
    }
    if !table[i].as_ref().unwrap().chunks.iter().all(|c| c.is_some()) {
        return None; // ainda faltam chunks
    }
    let slot = table[i].take().unwrap();
    let mut out = Vec::with_capacity(slot.chunks.len() * CHUNK_DATA_MAX);
    for c in slot.chunks.into_iter() {
        if let Some(v) = c {
            out.extend_from_slice(&v);
        }
    }
    crate::slog_nano!("P2P", "info", "chunk remontado node={} msg={} bytes={}", sid, msg_id, out.len());
    Some(out)
}

/// Self-test puro (sem HW): divide payload sintético de ~3000 bytes, remonta
/// via chunk_reassemble e verifica identidade byte a byte.
pub fn chunk_self_test() -> bool {
    let payload: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
    let msg_id = 0xCAFE_BABEu32;
    let total = ((payload.len() + CHUNK_DATA_MAX - 1) / CHUNK_DATA_MAX) as u16;
    let mut ok = false;
    for (seq, chunk) in payload.chunks(CHUNK_DATA_MAX).enumerate() {
        let mut body = Vec::with_capacity(12 + chunk.len());
        body.extend_from_slice(b"CHK\0");
        body.extend_from_slice(&msg_id.to_le_bytes());
        body.extend_from_slice(&(seq as u16).to_le_bytes());
        body.extend_from_slice(&total.to_le_bytes());
        body.extend_from_slice(chunk);
        if let Some(full) = chunk_reassemble(0x7F, &body, 1) {
            ok = full == payload;
        }
    }
    crate::slog_nano!("P2P", "info", "chunk_self_test ok={} bytes={} chunks={}", ok, payload.len(), total);
    ok
}

// ─── Transporte P2P R0 (ADR-0081 Fase A, SESSION_234) ─────────────────────
// Movido do bin: o kernel dono único do RX/TX broadcast (porta 42069).
// Pacotes não-heartbeat (Sync/ModelUpdate/…) são publicados no EVENT_BUS
// (tópico `TOPIC_P2P_PACKET`) para hermes consumir — sem inversão de
// dependência (k_nano não conhece hermes).

/// Tópico do EventBus para pacotes P2P não-heartbeat (payload = NoProto + payload bruto).
pub const TOPIC_P2P_PACKET: &str = "P2P_PACKET";

/// Heartbeat P2P + processamento de descoberta. Usa TIMER_TICKS global
/// (sempre avança, mesmo com o scheduler rate-limited) — heartbeat a cada
/// ~110 ticks do timer (~1.1s a 100Hz). Recebe e alimenta o MESH_ENGINE.
/// Chamado pelo bin a cada tick do scheduler (bei_tick hook).
pub fn p2p_tick(_tick: u64) {
    const P2P_PORT: u16 = 42069;
    // Lazy init do MESH_ENGINE (ADR-0081) — nunca inicializado no boot.
    {
        let mut eng = MESH_ENGINE.lock();
        if eng.is_none() {
            // node_id local no MESMO formato dos peers ([node_id(),0,0,0,0,0])
            // — SESSION_234: usar o MAC completo corrompia o tie-break da
            // eleição (comparação lexicográfica [3,0,..] < [0x52,0x54,..]
            // sempre true → todo mundo vira Worker).
            let nid = node_id();
            let local_id = [nid, 0, 0, 0, 0, 0];
            let caps = NodeCapabilities::new(
                local_id, 1, 1000, 1, 0,
                SimdWeight::None, false, false,
            );
            *eng = Some(BrainMeshEngine::new(caps));
            crate::slog_nano!("P2P", "info", "MESH_ENGINE inicializado (ADR-0081) node_id={}", nid);
        }
    }

    // Só após MAC presente (IP opcional — fallback 10.0.2.15 no frame).
    let ready = crate::nic_globals::NET_CONFIG.lock().mac != [0; 6];
    if !ready {
        return;
    }

    let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;

    // Heartbeat a cada ~110 ticks do timer (~1.1s a 100Hz). Usa last-sent
    // tracking (não depende de `now % 110 == 0` exato — o scheduler pode
    // pular o tick múltiplo).
    static LAST_SENT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    let last = LAST_SENT.load(Ordering::Relaxed);
    if now.wrapping_sub(last) >= 110 || last == 0 {
        LAST_SENT.store(now, Ordering::Relaxed);
        // node_id único por instância (último octeto do IP: 10.0.3.2→2, .3→3).
        // SESSION_234: usar local_role() colidia — ambas enviavam o mesmo ID
        // (Undecided=4) → add_or_update_node deduplicava → nodes=1.
        let node_id = node_id();
        let pkt = crate::net::udp_broadcast::make_heartbeat(node_id, now);
        let mut buf = crate::net::udp_broadcast::serialize(&pkt);
        // Fase A (SESSION_236): o heartbeat carrega a pk da sessão ("PK\0"+pk)
        // para o receptor TOFU vincular node_id → chave. Self-consistent: a
        // assinatura é verificada contra essa pk embutida.
        if let Some(pk) = crate::identity::session_public_key() {
            buf.extend_from_slice(b"PK\0");
            buf.extend_from_slice(&pk);
            // Fase B (ADR-0081): anuncia capacidades locais no heartbeat.
            let caps = local_caps();
            if caps != 0 {
                buf.extend_from_slice(b"CAP\0");
                buf.push(caps);
            }
        }
        match crate::net::udp_broadcast::sign_packet(&buf) {
            Some(signed) => {
                let ok = crate::net::udp_broadcast::udp_broadcast_send(&signed, P2P_PORT);
                crate::slog_nano!("P2P", "info", "TX heartbeat node={} t={} sent={}", node_id, now, ok);
            }
            None => {
                // Fail-closed: sem sessão não assina → não envia (peers dropariam).
                crate::slog_nano!("P2P", "warn", "TX heartbeat skip: sessao nao inicializada");
            }
        }
    }

    // Diagnóstico de segurança (throttle ~200 ticks).
    static LAST_SEC_LOG: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    let last_sec = LAST_SEC_LOG.load(Ordering::Relaxed);
    if now.wrapping_sub(last_sec) >= 200 || last_sec == 0 {
        LAST_SEC_LOG.store(now, Ordering::Relaxed);
        let (u, b, r) = sec_stats();
        crate::slog_nano!("P2P", "info", "sec: unsigned={} badsig={} replay={}", u, b, r);
    }

    // Recebe descobertas e alimenta o mesh engine
    // SESSION_237: recv_fragmented reassembla payloads > 1200B (ex: MW/MR de
    // matmul grande) antes do gate de segurança; pacotes ≤1200B (heartbeat/
    // ROLE/skills) retornam direto — caminho inalterado.
    while let Some(rx) = crate::net::udp_broadcast::recv_fragmented(P2P_PORT) {
        // ── Fase A de segurança (SESSION_236): fail-closed + TOFU + anti-replay ──
        let Some(pkt) = crate::net::udp_broadcast::parse(&rx) else {
            SEC_DROPPED_UNSIGNED.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        let sid = pkt.source_id;
        let clk = pkt.clock;
        let tt = pkt.task_type as u8;

        // (a) Todo pacote mesh deve ser assinado — fail-closed, sem exceção.
        //     O TX assina em todos os caminhos (heartbeat, ROLE, skill, matmul).
        if rx.len() < crate::net::noproto::PACKET_HEADER_SIZE + crate::identity::SIGNATURE_LEN {
            SEC_DROPPED_UNSIGNED.fetch_add(1, Ordering::Relaxed);
            crate::slog_nano!("P2P", "warn", "drop: pacote sem assinatura node={}", sid);
            continue;
        }
        // Payload bruto (após header, sem assinatura) — usado só no TOFU.
        let raw_payload = &rx[crate::net::noproto::PACKET_HEADER_SIZE..rx.len() - crate::identity::SIGNATURE_LEN];

        // (c) TOFU: peer conhecido verifica contra a pk vinculada. Desconhecido
        //     só vincula via heartbeat com prefixo "PK\0"+pk (self-consistent:
        //     assinatura verificada contra a pk embutida — prova posse da chave).
        //     Não-heartbeat de desconhecido → drop (sem como validar).
        let data: Vec<u8>;
        match peer_pk(sid) {
            Some(pk) => match crate::net::udp_broadcast::verify_packet(&rx, &pk) {
                Some(valid) => data = valid.to_vec(),
                None => {
                    SEC_DROPPED_BADSIG.fetch_add(1, Ordering::Relaxed);
                    crate::slog_nano!("P2P", "warn", "drop: assinatura invalida node={}", sid);
                    continue;
                }
            },
            None => {
                if tt != 5 {
                    SEC_DROPPED_BADSIG.fetch_add(1, Ordering::Relaxed);
                    crate::slog_nano!("P2P", "warn", "drop: peer desconhecido (sem vinculo) node={} type={}", sid, tt);
                    continue;
                }
                let pk = match raw_payload.strip_prefix(b"PK\0") {
                    Some(p) if p.len() >= PUBLIC_KEY_LEN => {
                        let mut k = [0u8; PUBLIC_KEY_LEN];
                        k.copy_from_slice(&p[..PUBLIC_KEY_LEN]);
                        k
                    }
                    _ => {
                        SEC_DROPPED_BADSIG.fetch_add(1, Ordering::Relaxed);
                        crate::slog_nano!("P2P", "warn", "drop: heartbeat sem PK embutida node={}", sid);
                        continue;
                    }
                };
                match crate::net::udp_broadcast::verify_packet(&rx, &pk) {
                    Some(valid) => {
                        peer_bind(sid, pk);
                        data = valid.to_vec();
                    }
                    None => {
                        SEC_DROPPED_BADSIG.fetch_add(1, Ordering::Relaxed);
                        crate::slog_nano!("P2P", "warn", "drop: assinatura TOFU invalida node={}", sid);
                        continue;
                    }
                }
            }
        }

        // (d) Anti-replay: só o canal de heartbeat tem clock monotônico por
        //     fonte (não-heartbeats usam clock=0 — são autenticados pela
        //     assinatura, não pelo clock). LAN confiável: drop se clk <= last.
        //     ponytail: janela de reordenação (WAN) = trabalho futuro.
        if tt == 5 {
            if let Some(last) = peer_last_clock(sid) {
                if clk <= last {
                    SEC_DROPPED_REPLAY.fetch_add(1, Ordering::Relaxed);
                    crate::slog_nano!("P2P", "warn", "drop: replay/stale node={} clk={} last={}", sid, clk, last);
                    continue;
                }
            }
            peer_update_clock(sid, clk);
        }

        // (e) Capacidades anunciadas no heartbeat ("CAP\0" após a pk). Só após
        //     o anti-replay passar (heartbeat stale não atualiza caps).
        if tt == 5 {
            if let Some(rest) = raw_payload.strip_prefix(b"PK\0") {
                if rest.len() >= PUBLIC_KEY_LEN + 5
                    && &rest[PUBLIC_KEY_LEN..PUBLIC_KEY_LEN + 4] == b"CAP\0"
                {
                    let caps = rest[PUBLIC_KEY_LEN + 4];
                    peer_set_caps(sid, caps);
                    crate::slog_nano!("P2P", "info", "caps node={} bits=0x{:02X}", sid, caps);
                }
            }
        }

        crate::slog_nano!("P2P", "info", "RX source_id={} clock={} type={}", sid, clk, tt);
        // Payload após o header NoProto (já fatiado aqui).
        let payload = if data.len() > crate::net::noproto::PACKET_HEADER_SIZE {
            &data[crate::net::noproto::PACKET_HEADER_SIZE..]
        } else {
            &[][..]
        };
        // SESSION_235: propagação de papéis — ROLE\0{target}\0{role_u8}.
        // Consumido AQUI (antes do publish P2P_PACKET) — não deve vazar
        // para skill_sync (que aplicaria "ROLE" como skill).
        if tt == 3 && payload.starts_with(b"ROLE\0") {
            let mut parts = payload[5..].splitn(2, |&b| b == 0);
            let target = match parts.next().and_then(|s| core::str::from_utf8(s).ok())
                .and_then(|s| s.parse::<u8>().ok())
            {
                Some(t) => t,
                None => continue, // malformado — descarta
            };
            let role_u8 = parts.next().and_then(|s| core::str::from_utf8(s).ok())
                .and_then(|s| s.parse::<u8>().ok())
                .unwrap_or(4); // fallback Undecided
            if target != node_id() {
                continue; // é de outro nó — ignora
            }
            let role = match role_u8 {
                0 => NodeRole::Master,
                1 => NodeRole::Memory,
                2 => NodeRole::Compute,
                3 => NodeRole::Worker,
                _ => NodeRole::Undecided,
            };
            {
                let eng = MESH_ENGINE.lock();
                if let Some(ref engine) = *eng {
                    engine.set_role(role);
                }
            }
            crate::slog_nano!("P2P", "info", "role aplicado node={} role={:?}", target, role);
            continue; // consumido — não publica no EVENT_BUS
        }
        // (f) Chunking (Fase B): payload "CHK\0" — insere no slot de remontagem
        //     e só publica quando a mensagem completa chegar. O chunk isolado
        //     NÃO é publicado no EVENT_BUS (hermes veria lixo).
        if tt != 5 && payload.starts_with(b"CHK\0") {
            match chunk_reassemble(sid, payload, now) {
                Some(full) => {
                    let mut full_pkt = data[..crate::net::noproto::PACKET_HEADER_SIZE].to_vec();
                    full_pkt.extend_from_slice(&full);
                    let _ = crate::EVENT_BUS.publish(Event {
                        id: 0,
                        topic: String::from(TOPIC_P2P_PACKET),
                        payload: full_pkt,
                        token: CapabilityToken::Legacy(1),
                    });
                    crate::slog_nano!("P2P", "info", "chunk publish node={} bytes={}", sid, full.len());
                }
                None => {
                    crate::slog_nano!("P2P", "debug", "chunk RX node={} aguardando demais chunks", sid);
                }
            }
            continue; // chunk consumido — não publica o chunk isolado
        }
        // Não-heartbeat (Sync/ModelUpdate/…): publica no EVENT_BUS —
        // hermes consome via skill_sync::poll_p2p() / skill_marketplace::poll_p2p().
        if tt != 5 {
            let _ = crate::EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(TOPIC_P2P_PACKET),
                payload: data.to_vec(),
                token: CapabilityToken::Legacy(1),
            });
        }
        // Só heartbeats alimentam o mesh engine (eleição/descoberta)
        if tt == 5 {
            let sender_mac = [pkt.source_id, 0, 0, 0, 0, 0];
            let caps = NodeCapabilities::new(
                sender_mac, 1, 1000, 1, 0,
                SimdWeight::None, false, false,
            );
            let mut eng = MESH_ENGINE.lock();
            if let Some(ref mut engine) = *eng {
                engine.add_or_update_node(caps);
                engine.check_election();
                let role = engine.local_role();
                let count = engine.node_count();
                crate::slog_nano!("P2P", "info", "mesh role={:?} nodes={}", role, count);
            }
        }
    }
}
