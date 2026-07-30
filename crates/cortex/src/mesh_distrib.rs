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
    /// Se P2P estiver ativo, retorna `Some(Vec<(NodeId, ExpertKind)>)` com
    /// o mapa de qual expert vai para qual no. O Master roda o algoritmo
    /// de assign ponderado por capacidade; Workers enviam seus experts
    /// para o Master (ponytail: transporte P2P pendente).
    ///
    /// Se P2P nao estiver ativo, retorna `None` — fallback local.
    /// O Trinity router usa experts locais normalmente.
    ///
    /// ## ponytail
    /// Transporte P2P (UDP broadcast / NoProto serializacao) nao esta wired.
    /// Enquanto isso, retorna assignment local (todos os experts no node 0).
    /// Upgrade: quando `HybridTransport` estiver vivo, Worker serializa experts
    /// via `udp_broadcast::serialize()`, envia para Master, Master roda
    /// `capacity_weighted_assign()`, devolve o mapa via broadcast.
    pub fn distribute_experts() -> Option<Vec<(NodeId, ExpertKind)>> {
        let role = local_role();
        let mut guard = DISTRIBUTOR.lock();
        let dist = match guard.as_mut() {
            Some(d) if d.active => d,
            _ => return None,
        };

        if dist.experts.is_empty() {
            return Some(Vec::new());
        }

        match role {
            NodeRole::Master => {
                // ponytail: gather remote expert lists via P2P transport,
                // then capacity_weighted_assign() sobre todos os nos.
                // Por enquanto, todos os experts ficam no Master (node 0).
                Some(dist.experts.iter().map(|(_, e)| (0u16, e.kind)).collect())
            }
            NodeRole::Worker | NodeRole::Compute | NodeRole::Memory => {
                // ponytail: serializar lista de experts e enviar ao Master.
                // Por enquanto, assignment local.
                Some(dist.experts.iter().map(|(_, e)| (0u16, e.kind)).collect())
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
            // Atualiza se ja existe, senao adiciona
            for entry in d.node_capacities.iter_mut() {
                if entry.0 == node_id {
                    entry.1 = score;
                    return;
                }
            }
            d.node_capacities.push((node_id, score));
        }
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
