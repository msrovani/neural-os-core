//! ADR-0081 C2: MoE Expert Distribution across mesh nodes.
//!
//! ## Visao
//! Especialistas do Trinity MoE sao distribuidos entre nos do mesh conforme
//! capacidade (CapacityScore). Nos com mais recursos recebem mais experts.
//!
//! ## Depende de: P2P Transport (Fase A da ADR-0081)
//! - `k_nano::net::mesh::local_role()` — papel do no no mesh
//! - `k_nano::net::transport::HybridTransport` — envio/recebimento futuro
//! - `k_nano::net::udp_broadcast::serialize` — serializacao NoProto
//!
//! ## Fallback local (ativo enquanto P2P nao estiver vivo)
//! Sem P2P, todos os experts rodam localmente — comportamento atual.
//! A funcao `distribute_experts()` retorna `None` (sem distribuicao) e o
//! Trinity MoE router usa o expert local normalmente.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use k_nano::net::mesh::{local_role, NodeRole};
use k_nano::net::noproto::{AiosTaskPacket, TaskType, PacketFlags};
use crate::trinity::ExpertKind;

/// Informacao sobre um expert registrado para distribuicao no mesh.
#[derive(Debug, Clone)]
pub struct ExpertInfo {
    /// Tipo do expert (Generator, RustCoder, etc.)
    pub kind: ExpertKind,
    /// Nome unico do expert
    pub name: String,
    /// Numero estimado de parametros (0 se desconhecido)
    pub param_count: u32,
    /// Tamanho dos pesos em bytes
    pub weight_bytes: usize,
}

impl ExpertInfo {
    pub fn new(kind: ExpertKind, name: &str, param_count: u32, weight_bytes: usize) -> Self {
        Self {
            kind,
            name: String::from(name),
            param_count,
            weight_bytes,
        }
    }
}

/// ID compacto de no no mesh para tabelas de distribuicao.
/// O mesh engine usa MAC 6-byte internamente; aqui usamos u16 para
/// tabelas de capacidade (max 16 nos no mesh).
pub type NodeId = u16;

/// Distribuidor de experts MoE entre nos do mesh P2P.
///
/// Enquanto P2P nao estiver vivo (`local_role() == Undecided`), todos os
/// experts rodam localmente — fallback transparente para o comportamento
/// atual do Trinity MoE router.
///
/// ## Integration
/// - `init()` — chamado no boot com a lista de experts locais
/// - `distribute_experts()` — se P2P ativo, retorna mapa de distribuicao;
///   se nao, retorna `None` (fallback local)
/// - `should_distribute()` — chamado pelo Trinity router antes de rotear
/// - `local_expert_count()` — numero de experts locais (sempre funciona)
pub struct MeshExpertDistributor {
    /// P2P esta ativo? Se false, todos os experts sao locais.
    pub active: bool,
    /// Registro local de experts: (node_name, ExpertInfo).
    pub experts: Vec<(String, ExpertInfo)>,
    /// Capacidades dos nos conhecidos: (node_id, capacity_score).
    pub node_capacities: Vec<(NodeId, f32)>,
    /// Experts remotos anunciados por outros nos: (node_id dono, ExpertInfo).
    /// SESSION_237: preenchido no Master ao receber "ED\0" dos Workers.
    pub remote_experts: Vec<(u8, ExpertInfo)>,
    /// Experts que o assign mandou para ESTE no (Worker preenche ao receber
    /// "EDR\0"; Master preenche ao rodar capacity_weighted_assign).
    pub my_assignment: Vec<(String, ExpertKind)>,
}

impl MeshExpertDistributor {
    /// Inicializa o distribuidor no boot.
    ///
    /// Checa o papel no mesh via `k_nano::net::mesh::local_role()` e ativa
    /// a distribuicao se P2P estiver vivo (qualquer papel exceto Undecided).
    /// Registra os experts fornecidos.
    ///
    /// Seguro chamar multiplas vezes — a partir da segunda, apenas atualiza
    /// o registro de experts e re-checa o mesh.
    pub fn init(experts: Vec<ExpertInfo>) {
        let role = local_role();
        let p2p_active = role != NodeRole::Undecided;

        let named: Vec<(String, ExpertInfo)> = experts
            .into_iter()
            .map(|e| (e.name.clone(), e))
            .collect();

        let mut guard = DISTRIBUTOR.lock();
        let first_init = guard.is_none();

        *guard = Some(MeshExpertDistributor {
            active: p2p_active,
            experts: named,
            node_capacities: Vec::new(),
            remote_experts: Vec::new(),
            my_assignment: Vec::new(),
        });

        if first_init {
            if p2p_active {
                k_nano::slog_cortex!("MESH_DISTRIB", "info",
                    "P2P active (role={:?}), expert distribution enabled", role);
            } else {
                k_nano::slog_cortex!("MESH_DISTRIB", "info",
                    "P2P not active (role=Undecided), local-only fallback");
            }
        }
    }

    /// Re-checa o papel no mesh e atualiza a flag `active`.
    /// Chamar apos eventos de descoberta ou mudanca de papel.
    pub fn refresh() {
        let role = local_role();
        let p2p_active = role != NodeRole::Undecided;
        if let Some(ref mut d) = *DISTRIBUTOR.lock() {
            d.active = p2p_active;
        }
    }

    /// O distribuidor esta ativo? (P2P mesh vivo)
    pub fn is_active() -> bool {
        DISTRIBUTOR
            .lock()
            .as_ref()
            .map(|d| d.active)
            .unwrap_or(false)
    }

    /// O Trinity router deve considerar experts remotos?
    /// Retorna `true` quando P2P esta ativo.
    #[inline]
    pub fn should_distribute() -> bool {
        Self::is_active()
    }

    /// Distribui experts entre os nos do mesh.
    ///
    /// Se P2P estiver ativo:
    /// - Master: roda o assign ponderado por capacidade sobre TODOS os experts
    ///   conhecidos (locais + remotos via "ED\0") e devolve o mapa.
    /// - Worker/Compute/Memory: devolve os experts que o assign mandou para ele
    ///   (campo `my_assignment`, preenchido ao receber "EDR\0" do Master).
    ///
    /// Se P2P nao estiver ativo, retorna `None` — fallback local.
    pub fn distribute_experts() -> Option<Vec<(NodeId, ExpertKind)>> {
        let role = local_role();
        let mut guard = DISTRIBUTOR.lock();
        let dist = match guard.as_mut() {
            Some(d) if d.active => d,
            _ => return None,
        };

        match role {
            NodeRole::Master => {
                // Assign real ponderado por capacidade (todos os experts).
                let assign = capacity_weighted_assign(dist);
                Some(assign.iter().map(|(name, target)| (*target as NodeId, kind_of(dist, name))).collect())
            }
            NodeRole::Worker | NodeRole::Compute | NodeRole::Memory => {
                // Experts que o assign mandou para mim (via "EDR\0").
                if dist.my_assignment.is_empty() {
                    return None; // ainda sem assign — fallback local
                }
                let local = k_nano::net::mesh::node_id() as NodeId;
                Some(dist.my_assignment.iter().map(|(_, k)| (local, *k)).collect())
            }
            NodeRole::Undecided => {
                // Perdeu o mesh enquanto distribuia — desativa fallback.
                dist.active = false;
                None
            }
        }
    }

    /// Numero de experts registrados localmente.
    /// Sempre funciona, independente do estado do P2P.
    pub fn local_expert_count() -> usize {
        DISTRIBUTOR
            .lock()
            .as_ref()
            .map(|d| d.experts.len())
            .unwrap_or(0)
    }

    /// Registra um expert adicional em runtime (ex.: apos `DynamicMoE::try_birth`).
    pub fn register_expert(info: ExpertInfo) {
        let name = info.name.clone();
        if let Some(ref mut d) = *DISTRIBUTOR.lock() {
            // Evita duplicatas pelo nome
            if !d.experts.iter().any(|(n, _)| *n == name) {
                d.experts.push((name, info));
            }
        }
    }

    /// Remove um expert pelo nome.
    pub fn unregister_expert(name: &str) {
        if let Some(ref mut d) = *DISTRIBUTOR.lock() {
            d.experts.retain(|(n, _)| n != name);
        }
    }

    /// Snapshot dos experts locais.
    pub fn local_experts() -> Vec<(String, ExpertInfo)> {
        DISTRIBUTOR
            .lock()
            .as_ref()
            .map(|d| d.experts.clone())
            .unwrap_or_default()
    }

    /// Snapshot das capacidades dos nos.
    pub fn node_capacities() -> Vec<(NodeId, f32)> {
        DISTRIBUTOR
            .lock()
            .as_ref()
            .map(|d| d.node_capacities.clone())
            .unwrap_or_default()
    }

    /// Atualiza a tabela de capacidades dos nos no mesh.
    /// Chamado pelo Master quando recebe heartbeats ou discovery.
    pub fn update_node_capacity(node_id: NodeId, score: f32) {
        if let Some(ref mut d) = *DISTRIBUTOR.lock() {
            d.update_capacity_inner(node_id, score);
        }
    }

    /// Inner (sem lock — chamado com o DISTRIBUTOR já travado).
    fn update_capacity_inner(&mut self, node_id: NodeId, score: f32) {
        for entry in self.node_capacities.iter_mut() {
            if entry.0 == node_id {
                entry.1 = score;
                return;
            }
        }
        self.node_capacities.push((node_id, score));
    }
}

// ─── Distribuição P2P real (ADR-0081 C2, SESSION_237) ──────────────────────
// Protocolo binário (porta 42069, TaskType::Inference — mesmo canal do matmul;
// skill_sync/marketplace gateiam em Sync/ModelUpdate, então não colidem):
//
//   ED\0  (Worker→Master): b"ED\0" | node_id u8 | count u32 LE
//          por expert: name_len u32 LE | name | kind u8 | param_count u32 LE | weight_bytes u32 LE
//   EDR\0 (Master→Worker): b"EDR\0" | dest_node u8 | count u32 LE
//          por expert: name_len u32 LE | name | target_node u8
//
// Assinado (Fase A fail-closed) e fragmentado se > 1200B (FRAG\0).

/// Receiver do EventBus P2P_PACKET (lazy subscribe, padrão poll_mesh_requests).
static DIST_RECV: Mutex<Option<event_bus::Receiver>> = Mutex::new(None);

fn le32(p: &[u8], off: usize) -> Option<u32> {
    let b = p.get(off..off + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn kind_from_u8(v: u8) -> ExpertKind {
    match v {
        0 => ExpertKind::HwIdentify,
        1 => ExpertKind::HwControl,
        2 => ExpertKind::RustCoder,
        3 => ExpertKind::DiskDiag,
        4 => ExpertKind::Security,
        5 => ExpertKind::Generator,
        6 => ExpertKind::SpeechSynth,
        _ => ExpertKind::Unknown,
    }
}

/// Kind de um expert pelo nome (busca em locais e remotos).
fn kind_of(d: &MeshExpertDistributor, name: &str) -> ExpertKind {
    d.experts
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, e)| e.kind)
        .or_else(|| {
            d.remote_experts
                .iter()
                .find(|(_, e)| e.name == name)
                .map(|(_, e)| e.kind)
        })
        .unwrap_or(ExpertKind::Unknown)
}

/// Worker/Compute/Memory: serializa os experts locais como "ED\0", assina e
/// broadcasta para o Master (que roda o assign ponderado). Chamado 1x pelo bin
/// quando o nó é Worker com peer (static flag no bei_tick).
pub fn broadcast_local_experts() -> bool {
    let role = local_role();
    if role == NodeRole::Master || role == NodeRole::Undecided {
        return false;
    }
    let node_id = k_nano::net::mesh::node_id();
    let experts = MeshExpertDistributor::local_experts();
    if experts.is_empty() {
        return false;
    }
    let mut payload = Vec::with_capacity(64);
    payload.extend_from_slice(b"ED\0");
    payload.push(node_id);
    payload.extend_from_slice(&(experts.len() as u32).to_le_bytes());
    for (_, e) in &experts {
        payload.extend_from_slice(&(e.name.len() as u32).to_le_bytes());
        payload.extend_from_slice(e.name.as_bytes());
        payload.push(e.kind as u8);
        payload.extend_from_slice(&e.param_count.to_le_bytes());
        payload.extend_from_slice(&(e.weight_bytes as u32).to_le_bytes());
    }
    let pkt = AiosTaskPacket::new(
        k_nano::net::mesh::next_data_clock(), node_id, 0xFF, TaskType::Inference, 1, 0, 0, PacketFlags::new(),
    );
    let mut buf = k_nano::net::udp_broadcast::serialize(&pkt);
    buf.extend_from_slice(&payload);
    let Some(signed) = k_nano::net::udp_broadcast::sign_packet(&buf) else {
        return false; // fail-closed: sem sessão não assina
    };
    let ok = k_nano::net::udp_broadcast::send_fragmented(&signed, 42069);
    k_nano::slog_cortex!(
        "MESH_DISTRIB", "info",
        "experts enviados node={} n={} sent={}", node_id, experts.len(), ok
    );
    ok
}

/// Parse "ED\0" → (node_id dono, experts remotos).
fn parse_ed(payload: &[u8]) -> Option<(u8, Vec<ExpertInfo>)> {
    if payload.len() < 8 || &payload[0..3] != b"ED\0" {
        return None;
    }
    let node_id = payload[3];
    let count = le32(payload, 4)? as usize;
    let mut off = 8;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let name_len = le32(payload, off)? as usize;
        off += 4;
        let name = core::str::from_utf8(payload.get(off..off.saturating_add(name_len))?).ok()?;
        off += name_len;
        let kind = kind_from_u8(*payload.get(off)?);
        off += 1;
        let param_count = le32(payload, off)?;
        off += 4;
        let weight_bytes = le32(payload, off)? as usize;
        off += 4;
        out.push(ExpertInfo {
            kind,
            name: String::from(name),
            param_count,
            weight_bytes,
        });
    }
    Some((node_id, out))
}

/// Assign ponderado por capacidade: distribui TODOS os experts conhecidos
/// (locais + remotos) entre os nós, greedy pelos mais pesados primeiro, cada
/// um para o nó com menor carga relativa (carga / capacidade). Igualdade de
/// capacidade → balanceamento por peso total. Retorna (name, target_node).
/// Phase 2: usa health metrics (reachable, avg_rtt, p99) para capacity dinâmico.
fn capacity_weighted_assign(d: &MeshExpertDistributor) -> Vec<(String, u8)> {
    let local = k_nano::net::mesh::node_id();
    // Nós candidatos: local + donos de experts remotos.
    let mut nodes: Vec<u8> = alloc::vec![local];
    for (n, _) in &d.remote_experts {
        if !nodes.contains(n) {
            nodes.push(*n);
        }
    }
    if nodes.is_empty() {
        return Vec::new();
    }
    let cap = |n: u8| -> f32 {
        let base = d.node_capacities
            .iter()
            .find(|(id, _)| *id == n as NodeId)
            .map(|(_, s)| *s)
            .unwrap_or(1000.0);
        // Phase 2: ajusta capacidade baseada em health metrics.
        if let Some(h) = k_nano::net::mesh::peer_health(n) {
            if !h.reachable {
                return 0.0; // nó unreachable → capacidade zero
            }
            // Penaliza latência alta: capacity *= 1 / (1 + avg_rtt/1000)
            let avg_rtt_sec = h.avg_rtt_ticks as f32 / 100.0; // ticks → ~ms (100Hz)
            let latency_factor = 1.0 / (1.0 + avg_rtt_sec / 1000.0);
            // Penaliza p99 alto: capacity *= 1 / (1 + p99/2000)
            let p99 = k_nano::net::mesh::peer_p99_rtt(n) as f32 / 100.0;
            let p99_factor = 1.0 / (1.0 + p99 / 2000.0);
            base * latency_factor * p99_factor
        } else {
            base // sem health data → usa base
        }
    };
    // Todos os experts com peso (dedupe por nome — prioridade local).
    let mut experts: Vec<(String, u32)> = Vec::new();
    for (name, e) in &d.experts {
        experts.push((name.clone(), e.weight_bytes as u32));
    }
    for (_, e) in &d.remote_experts {
        if !experts.iter().any(|(n, _)| n == &e.name) {
            experts.push((e.name.clone(), e.weight_bytes as u32));
        }
    }
    experts.sort_by(|a, b| b.1.cmp(&a.1)); // maiores primeiro (greedy)

    let mut load: Vec<(u8, f32)> = nodes.iter().map(|n| (*n, 0.0f32)).collect();
    let mut out: Vec<(String, u8)> = Vec::with_capacity(experts.len());
    for (name, w) in experts {
        let mut best = load[0].0;
        let mut best_rel = f32::MAX;
        for (n, l) in &load {
            let c = cap(*n).max(1.0);
            let rel = l / c;
            if rel < best_rel {
                best_rel = rel;
                best = *n;
            }
        }
        for (n, l) in load.iter_mut() {
            if *n == best {
                *l += w as f32;
            }
        }
        out.push((name, best));
    }
    out
}

/// Monta blob assinado "EDR\0" para um destino (lista de experts assignados).
fn build_edr(dest: u8, list: &[(String, u8)]) -> Option<Vec<u8>> {
    let my_id = k_nano::net::mesh::node_id();
    // ADR-0081 follow-up: clock monotônico único por fonte (anti-replay).
    let pkt = AiosTaskPacket::new(
        k_nano::net::mesh::next_data_clock(), my_id, dest, TaskType::Inference, 1, 0, 0, PacketFlags::new(),
    );
    let mut payload = Vec::with_capacity(16);
    payload.extend_from_slice(b"EDR\0");
    payload.push(dest);
    payload.extend_from_slice(&(list.len() as u32).to_le_bytes());
    for (name, target) in list {
        payload.extend_from_slice(&(name.len() as u32).to_le_bytes());
        payload.extend_from_slice(name.as_bytes());
        payload.push(*target);
    }
    let mut buf = k_nano::net::udp_broadcast::serialize(&pkt);
    buf.extend_from_slice(&payload);
    // ADR-0081 Tier F: EDR\0 é ponto-a-ponto (dest=dest, o Worker alvo) →
    // SEAL com AEAD quando Tier Full + pk do destino conhecida; senão cai em
    // sign_packet_tiered (HMAC Relativized / Ed25519 Full).
    k_nano::net::udp_broadcast::seal_packet_tiered(&buf, dest)
}

/// Worker: aplica "EDR\0" do Master — preenche `my_assignment`.
fn apply_edr(payload: &[u8]) {
    if payload.len() < 6 || &payload[0..4] != b"EDR\0" {
        return;
    }
    let dest = payload[4];
    if dest != k_nano::net::mesh::node_id() {
        return; // para outro nó
    }
    let count = match le32(payload, 5) {
        Some(c) => c as usize,
        None => return,
    };
    let mut off = 9;
    let mut my: Vec<(String, ExpertKind)> = Vec::with_capacity(count);
    {
        let guard = DISTRIBUTOR.lock();
        for _ in 0..count {
            let name_len = match le32(payload, off) {
                Some(l) => l as usize,
                None => return,
            };
            off += 4;
            let name = match core::str::from_utf8(payload.get(off..off.saturating_add(name_len)).unwrap_or(&[])) {
                Ok(n) => n,
                Err(_) => return,
            };
            off += name_len;
            let target = match payload.get(off) {
                Some(t) => *t,
                None => return,
            };
            off += 1;
            if target != dest {
                continue; // expert assignado para outro nó — ignora
            }
            // Kind: lookup nos experts locais (o EDR não carrega kind).
            let kind = guard
                .as_ref()
                .and_then(|d| d.experts.iter().find(|(n, _)| n == name).map(|(_, e)| e.kind))
                .unwrap_or(ExpertKind::Unknown);
            my.push((String::from(name), kind));
        }
    }
    if let Some(ref mut d) = *DISTRIBUTOR.lock() {
        d.my_assignment = my;
    }
    k_nano::slog_cortex!(
        "MESH_DISTRIB", "info",
        "EDR recebido: {} experts assignados para node={}", count, dest
    );
}

/// Master side: drena o EventBus e responde "ED\0" dos Workers com "EDR\0"
/// (assign ponderado por capacidade). Chamado pelo bin a cada tick (bei_tick).
pub fn poll_expert_requests() {
    {
        let mut recv = DIST_RECV.lock();
        if recv.is_none() {
            *recv = Some(k_nano::EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
        }
    }
    loop {
        let evt = DIST_RECV.lock().as_ref().and_then(|r| r.try_receive());
        let Some(evt) = evt else { break };
        if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
            continue;
        }
        let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) else { continue };
        if pkt.task_type != TaskType::Inference {
            continue;
        }
        let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
            &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
        } else {
            &[][..]
        };
        // EDR (resposta do Master) — qualquer nó pode receber; o destino aplica.
        if payload.starts_with(b"EDR\0") {
            apply_edr(payload);
            continue;
        }
        if !payload.starts_with(b"ED\0") {
            continue;
        }
        // Só o Master responde requests de distribuição.
        if local_role() != NodeRole::Master {
            continue;
        }
        let Some((src_node, experts)) = parse_ed(payload) else { continue };
        let local = k_nano::net::mesh::node_id();
        let mut guard = DISTRIBUTOR.lock();
        let Some(d) = guard.as_mut() else { continue };

        // Guarda experts remotos do nó (substitui os anteriores dele).
        d.remote_experts.retain(|(n, _)| *n != src_node);
        for e in experts {
            d.remote_experts.push((src_node, e));
        }
        // Capacidade: proxy honesto = 1000.0 + peso total (KB) dos experts do nó.
        let total_weight: u32 = d
            .remote_experts
            .iter()
            .filter(|(n, _)| *n == src_node)
            .map(|(_, e)| e.weight_bytes as u32)
            .sum();
        d.update_capacity_inner(src_node as NodeId, 1000.0 + total_weight as f32 / 1024.0);

        // Assign ponderado + atualiza my_assignment do Master.
        let assign = capacity_weighted_assign(d);
        d.my_assignment = assign
            .iter()
            .filter(|(_, t)| *t == local)
            .map(|(n, _)| (n.clone(), kind_of(d, n)))
            .collect();

        // Destinos: alvos do assign (≠ local) + donos remotos (≠ local).
        let mut targets: Vec<u8> = Vec::new();
        for (_, t) in &assign {
            if *t != local && !targets.contains(t) {
                targets.push(*t);
            }
        }
        for (n, _) in &d.remote_experts {
            if *n != local && !targets.contains(n) {
                targets.push(*n);
            }
        }
        drop(guard);
        let n_targets = targets.len();
        for target in targets {
            let list: Vec<(String, u8)> = assign
                .iter()
                .filter(|(_, t)| *t == target)
                .cloned()
                .collect();
            if let Some(signed) = build_edr(target, &list) {
                let ok = k_nano::net::udp_broadcast::send_fragmented(&signed, 42069);
                k_nano::slog_cortex!(
                    "MESH_DISTRIB", "info",
                    "EDR assign node={} experts={} sent={}", target, list.len(), ok
                );
            }
        }
        k_nano::slog_cortex!(
            "MESH_DISTRIB", "info",
            "assign completo: {} experts em {} nos", assign.len(), n_targets
        );
    }
}

// Static global state.
static DISTRIBUTOR: Mutex<Option<MeshExpertDistributor>> = Mutex::new(None);

// ─── Self-test ────────────────────────────────────────────────────────────

/// Self-test: verifica fallback local, registro e contagem.
///
/// Retorna `true` se todos os assertions passarem.
/// Projetado para rodar em ambiente de teste (sem mesh), onde
/// `local_role()` retorna `Undecided` → distribuicao inativa.
pub fn self_test() -> bool {
    // Ainda nao init → inativo, count 0, distribute_experts() == None
    if MeshExpertDistributor::is_active() {
        return false;
    }
    if MeshExpertDistributor::local_expert_count() != 0 {
        return false;
    }
    if MeshExpertDistributor::distribute_experts().is_some() {
        return false;
    }

    // Init com lista vazia
    MeshExpertDistributor::init(Vec::new());
    // Active depende do mesh; em ambiente sem mesh (Undecided) → inactive
    // mas local_expert_count deve funcionar
    if MeshExpertDistributor::local_expert_count() != 0 {
        return false;
    }

    // Registra dois experts
    MeshExpertDistributor::register_expert(
        ExpertInfo::new(ExpertKind::Generator, "generator", 0, 1024));
    MeshExpertDistributor::register_expert(
        ExpertInfo::new(ExpertKind::RustCoder, "rust_coder", 0, 2048));

    if MeshExpertDistributor::local_expert_count() != 2 {
        return false;
    }

    // Remove um
    MeshExpertDistributor::unregister_expert("generator");
    if MeshExpertDistributor::local_expert_count() != 1 {
        return false;
    }

    // Update node capacity
    MeshExpertDistributor::update_node_capacity(0, 1000.0);
    MeshExpertDistributor::update_node_capacity(1, 500.0);
    let caps = MeshExpertDistributor::node_capacities();
    if caps.len() != 2 {
        return false;
    }
    // Update existente
    MeshExpertDistributor::update_node_capacity(0, 1500.0);
    let caps = MeshExpertDistributor::node_capacities();
    if caps.len() != 2 {
        return false;
    }
    if caps.iter().any(|(id, _)| *id == 0 && caps.iter().find(|(i, _)| *i == 0).map_or(true, |(_, s)| *s < 1499.0)) {
        // verifica se o score do node 0 foi atualizado
        let score0 = caps.iter().find(|(i, _)| *i == 0).map(|(_, s)| *s).unwrap_or(0.0);
        if (score0 - 1500.0).abs() > 0.01 {
            return false;
        }
    }

    true
}
