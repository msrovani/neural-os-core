//! SYS_HEALTH — produtor + escalador (Hermes → LLM) dos subsistemas de
//! máquina (net / storage / gpu). Complementa o AUDIO_HEALTH do jarbas.
//!
//! Papel do agente (um tick, dois deveres):
//! 1. **Produtor:** monta `SysHealthInputs` de fontes REAIS (env::net_link_ok,
//!    netstack RX, boot_logger flush counters, gpu backend canário) e publica
//!    `SYS_HEALTH` 1×/s (padrão AUDIO_HEALTH/MESH_HEALTH).
//! 2. **Consumidor/escalador:** aplica a política única
//!    (`k_nano::sys_health::EscalationState`) por subsistema e escala NO_GO
//!    persistente ao LLM via USER_INTENT — 1× por incidente (anti-loop
//!    SESSION_410). UNKNOWN nunca escala (estado esperado, não falha).
//!
//! Fontes de evidência (todas lock-free, sem dependência reversa):
//! - net: `k_nano::env::net_link_ok` + `net_rx_count` (registrado via seam
//!   `k_nano::sys_health::register_net_rx_source` — k_nano não pode depender
//!   de hermes).
//! - storage: `boot_logger::FAT_READY` + contadores `STORAGE_FLUSH_OK/FAIL`
//!   incrementados no `flush()`.
//! - gpu: `k_hal::gpu::backend::compute_state()` (canário vector_add golden)
//!   + `note_gpu_compute` incrementado em matmul real.

use core::sync::atomic::{AtomicU64, Ordering};

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

use k_nano::sys_health::{
    publish_sys_health, GpuBackend, SysHealthInputs, SysHealthVerdicts, Verdict,
    NO_GO_PERSIST_MIN, R_GPU_QUARANTINE, R_NET_LINK_DOWN, R_NET_SLIP_DEGRADED,
    R_STORAGE_FLUSH_FAIL, R_STORAGE_NO_PERSIST,
};

/// Tópico publicado por este agente.
pub const TOPIC_SYS_HEALTH: &str = k_nano::sys_health::TOPIC_SYS_HEALTH;

/// SESSION_417: veredito consolidado de máquina (sys + audio), publicado
/// 1×/s e propagado via mesh (prefixo `MCH\0`) para o Master agregar a frota.
pub const TOPIC_MACHINE_HEALTH: &str = "MACHINE_HEALTH";
/// Prefixo do payload mesh do veredito de máquina (padrão `MEM\0`/`ROLE\0`).
pub const MESH_PREFIX_MCH: &[u8] = b"MCH\0";

/// Escala via USER_INTENT (mesmo caminho do runtime_observe/audio_health).
const TOPIC_USER_INTENT: &str = crate::hermes::TOPIC_USER_INTENT;

/// Subsistemas do SYS_HEALTH (ordem = prioridade de impacto na IA: sem net a
/// IA não busca na internet; sem storage não memoriza; GPU = aceleração).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SysSubsystem {
    Net,
    Storage,
    Gpu,
}

impl SysSubsystem {
    fn label(self) -> &'static str {
        match self {
            SysSubsystem::Net => "net",
            SysSubsystem::Storage => "storage",
            SysSubsystem::Gpu => "gpu",
        }
    }
}

/// Extrai a razão relevante ao subsistema (razões são estáveis e mapeadas —
/// razão desconhecida não escala com rótulo fabricado).
fn reason_for(sub: SysSubsystem, reasons: &[&'static str]) -> Option<&'static str> {
    let affects = |r: &str| match sub {
        SysSubsystem::Net => r == R_NET_LINK_DOWN || r == R_NET_SLIP_DEGRADED,
        SysSubsystem::Storage => r == R_STORAGE_FLUSH_FAIL || r == R_STORAGE_NO_PERSIST,
        SysSubsystem::Gpu => r == R_GPU_QUARANTINE,
    };
    reasons.iter().copied().find(|r| affects(r))
}

/// Prompt de diagnóstico autônomo (JSON + razão — IA decide com dado medido).
pub fn escalation_prompt(sub: SysSubsystem, reason: &str, json: &str) -> alloc::string::String {
    alloc::format!(
        "SYS_HEALTH persistente: subsistema de máquina {} em NO_GO (razão: {}). \
Diagnostique a causa raiz e proponha correção. Snapshot: {}",
        sub.label(),
        reason,
        json
    )
}

// ── Contadores lock-free (observabilidade) ──────────────────────────────────
pub static SYS_HEALTH_ESCALATIONS: AtomicU64 = AtomicU64::new(0);
pub static SYS_HEALTH_PUBLISH_TICKS: AtomicU64 = AtomicU64::new(0);

const SYS_MANIFEST: AgentManifest = AgentManifest {
    name: "sys_health_agent",
    kind: AgentKind::System,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: true,
};

/// Produtor + escalador do SYS_HEALTH (EventDriven: tem trabalho a cada
/// período de publicação; between publishes o `has_pending` é temporal).
pub struct SysHealthAgent {
    net: k_nano::sys_health::EscalationState,
    storage: k_nano::sys_health::EscalationState,
    gpu: k_nano::sys_health::EscalationState,
    next_publish_tick: u64,
    rx_source_registered: bool,
    /// SESSION_417: último snapshot de áudio (JSON bruto — parse na consolidação).
    last_audio_json: Option<alloc::string::String>,
    /// Receptor do AUDIO_HEALTH (consumidor 2 do tópico do jarbas).
    audio_rx: Option<event_bus::Receiver>,
    /// Última máquina consolidada (para o TX mesh e testes).
    last_machine: Option<k_nano::sys_health::MachineHealth>,
    /// Último tick de TX mesh (cooldown 10 s — heartbeat já flui 1×/s).
    last_mesh_tx_tick: u64,
    /// Escaladas deste agente (campo testável — global é só observabilidade).
    pub escalations: u64,
}

impl Default for SysHealthAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl SysHealthAgent {
    pub fn new() -> Self {
        SysHealthAgent {
            net: k_nano::sys_health::EscalationState::default(),
            storage: k_nano::sys_health::EscalationState::default(),
            gpu: k_nano::sys_health::EscalationState::default(),
            next_publish_tick: 0,
            rx_source_registered: false,
            last_audio_json: None,
            audio_rx: None,
            last_machine: None,
            last_mesh_tx_tick: 0,
            escalations: 0,
        }
    }

    /// Registra a fonte de RX da netstack (seam k_nano ← hermes, 1×).
    fn ensure_rx_source(&mut self) {
        if !self.rx_source_registered {
            k_nano::sys_health::register_net_rx_source(crate::netstack::net_rx_count);
            self.rx_source_registered = true;
        }
    }

    /// Monta os inputs de fontes reais. Chamado no tick do agente.
    fn gather_inputs(&self) -> SysHealthInputs {
        SysHealthInputs {
            net_link: k_nano::env::net_link_ok(),
            net_slip_degraded: k_nano::env::net_hud_label() == "slip",
            storage_persisted: k_nano::boot_logger::FAT_READY.load(Ordering::Acquire),
            gpu_backend: match k_hal::gpu::backend::compute_state() {
                k_hal::gpu::compute_abi::BackendState::Probed => GpuBackend::Probed,
                k_hal::gpu::compute_abi::BackendState::BringingUp => GpuBackend::BringingUp,
                k_hal::gpu::compute_abi::BackendState::Ready => GpuBackend::Ready,
                k_hal::gpu::compute_abi::BackendState::Quarantine => GpuBackend::Quarantine,
                k_hal::gpu::compute_abi::BackendState::CpuOnly => GpuBackend::CpuOnly,
            },
            // deltas de contadores são preenchidos dentro do publish_sys_health
            // (ele faz o swap atômico das janelas — fonte única de truth).
            ..Default::default()
        }
    }

    /// Aplica a política de escala sobre um snapshot recém-publicado.
    /// (separado do publish para testes — recebe vereditos + razões + JSON)
    fn observe_and_escalate(&mut self, verdicts: &SysHealthVerdicts, reasons: &[&'static str], json: &str) {
        if let Some(reason) = self.net.observe(verdicts.net, reason_for(SysSubsystem::Net, reasons)) {
            self.escalate(SysSubsystem::Net, reason, json);
        }
        if let Some(reason) = self.storage.observe(verdicts.storage, reason_for(SysSubsystem::Storage, reasons)) {
            self.escalate(SysSubsystem::Storage, reason, json);
        }
        if let Some(reason) = self.gpu.observe(verdicts.gpu, reason_for(SysSubsystem::Gpu, reasons)) {
            self.escalate(SysSubsystem::Gpu, reason, json);
        }
    }

    // ── SESSION_417: consolidação de máquina (sys + audio) ─────────────────

    /// Drena AUDIO_HEALTH (lazy subscribe — padrão mesh_health do DisplayAgent)
    /// e guarda o último JSON bruto.
    fn drain_audio_health(&mut self) {
        if self.audio_rx.is_none() {
            self.audio_rx = Some(k_nano::EVENT_BUS.subscribe(crate::audio_health::TOPIC_AUDIO_HEALTH));
        }
        if let Some(rx) = &self.audio_rx {
            while let Some(evt) = rx.try_receive() {
                if let Ok(s) = core::str::from_utf8(&evt.payload) {
                    self.last_audio_json = Some(alloc::string::String::from(s));
                }
            }
        }
    }

    /// Consolida o veredito de máquina: domínio sys (do JSON próprio) + domínio
    /// audio (do último AUDIO_HEALTH consumido, se houver — sem snapshot de
    /// áudio o domínio é omitido, não fabricado).
    fn consolidate_machine(&self, sys_json: &str) -> Option<k_nano::sys_health::MachineHealth> {
        let (v, reasons) = parse_own_json(sys_json)?;
        let mut domains = alloc::vec::Vec::new();
        domains.push((
            "sys",
            k_nano::sys_health::DomainVerdicts {
                fields: alloc::vec![
                    ("net", v.net),
                    ("storage", v.storage),
                    ("gpu", v.gpu),
                ],
                reasons: reasons.clone(),
            },
        ));
        if let Some(audio_json) = &self.last_audio_json {
            if let Some(snap) = crate::audio_health::parse_snapshot(audio_json) {
                let audio_reasons: alloc::vec::Vec<&'static str> = crate::audio_health::reasons_of(audio_json);
                domains.push((
                    "audio",
                    k_nano::sys_health::DomainVerdicts {
                        fields: alloc::vec![
                            ("playback", snap.playback),
                            ("capture", snap.capture),
                        ],
                        reasons: audio_reasons,
                    },
                ));
            }
        }
        let node = k_nano::net::mesh::node_id();
        Some(k_nano::sys_health::machine_verdict(node, domains))
    }

    /// Prompt ÚNICO de máquina para o LLM (substitui os prompts separados
    /// sys/audio quando ambos os domínios existem — um snapshot, um diagnóstico).
    pub fn machine_prompt(m: &k_nano::sys_health::MachineHealth) -> alloc::string::String {
        alloc::format!(
            "MACHINE_HEALTH consolidado do nó {}: worst-of dos domínios = {}. \
Diagnostique a causa raiz do(s) domínio(s) degradado(s) e proponha correção. Snapshot: {}",
            m.node,
            m.overall().label(),
            k_nano::sys_health::machine_verdict_json(m)
        )
    }

    /// TX mesh do veredito (cooldown 10 s; best-effort — heartbeat carrega o
    /// resto). Payload `MCH\0` + JSON de máquina.
    fn mesh_publish(&mut self, m: &k_nano::sys_health::MachineHealth, now: u64) {
        // 10 s = 1000 ticks @100 Hz.
        if now.saturating_sub(self.last_mesh_tx_tick) < 1000 {
            return;
        }
        self.last_mesh_tx_tick = now;
        let json = k_nano::sys_health::machine_verdict_json(m);
        let mut payload = alloc::vec::Vec::with_capacity(MESH_PREFIX_MCH.len() + json.len());
        payload.extend_from_slice(MESH_PREFIX_MCH);
        payload.extend_from_slice(json.as_bytes());
        let ok = k_nano::net::mesh::mesh_send_large(&payload);
        k_nano::slog_hermes!("SysHealth", "info", "mesh TX MCH node={} bytes={} ok={}", m.node, payload.len(), ok);
    }

    fn escalate(&mut self, sub: SysSubsystem, reason: &'static str, json: &str) {
        SYS_HEALTH_ESCALATIONS.fetch_add(1, Ordering::Relaxed);
        self.escalations += 1;
        k_nano::slog_hermes!(
            "SysHealth",
            "ok",
            "escalate LLM {} persist reason={} escalations={}",
            sub.label(),
            reason,
            SYS_HEALTH_ESCALATIONS.load(Ordering::Relaxed)
        );
        let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from(TOPIC_USER_INTENT),
            payload: escalation_prompt(sub, reason, json).into_bytes(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for SysHealthAgent {
    fn manifest(&self) -> &AgentManifest {
        &SYS_MANIFEST
    }

    fn has_pending(&self) -> bool {
        // EventDriven com cadência temporal (publicação 1 Hz).
        self.next_publish_tick != 0 && tick_now() >= self.next_publish_tick
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        self.ensure_rx_source();
        let now = tick_now();
        if self.next_publish_tick == 0 {
            self.next_publish_tick = now + k_nano::sys_health::PUBLISH_PERIOD_TICKS;
        }
        if now < self.next_publish_tick {
            return AgentTickResult::Pending;
        }
        self.next_publish_tick = now + k_nano::sys_health::PUBLISH_PERIOD_TICKS;

        // 1. Publica o snapshot (publish_sys_health completa os deltas e
        //    escreve LAST_SYS_HEALTH_JSON).
        let inputs = self.gather_inputs();
        publish_sys_health(inputs, now);
        SYS_HEALTH_PUBLISH_TICKS.fetch_add(1, Ordering::Relaxed);

        // 2. Observa o que acabou de publicar e escala se due.
        if let Some(json) = k_nano::sys_health::LAST_SYS_HEALTH_JSON.lock().clone() {
            // Re-parse mínimo do JSON próprio (vereditos conhecidos) para
            // alimentar a política — evita segunda estrutura de estado.
            if let Some((verdicts, reasons)) = parse_own_json(&json) {
                self.observe_and_escalate(&verdicts, &reasons, &json);
            }
        }

        // 3. SESSION_417: consolida máquina (sys + audio) e publica MACHINE_HEALTH.
        self.drain_audio_health();
        if let Some(json) = k_nano::sys_health::LAST_SYS_HEALTH_JSON.lock().clone() {
            if let Some(machine) = self.consolidate_machine(&json) {
                let mj = k_nano::sys_health::machine_verdict_json(&machine);
                *LAST_MACHINE_JSON.lock() = Some(mj.clone());
                let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                    id: 0,
                    topic: alloc::string::String::from(TOPIC_MACHINE_HEALTH),
                    payload: mj.into_bytes(),
                    token: event_bus::CapabilityToken::Legacy(1),
                });
                self.last_machine = Some(machine.clone());
                self.mesh_publish(&machine, now);
            }
        }
        AgentTickResult::Pending
    }
}

/// Último veredito de máquina consolidado (HUD/Master lêm sem re-parse do bus).
pub static LAST_MACHINE_JSON: spin::Mutex<Option<alloc::string::String>> = spin::Mutex::new(None);

fn tick_now() -> u64 {
    k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
}

/// Parse mínimo do JSON próprio (formato fixo emitido por to_json).
/// (vereditos + primeira razão conhecida; usado para alimentar a política)
pub fn parse_own_json(json: &str) -> Option<(SysHealthVerdicts, alloc::vec::Vec<&'static str>)> {
    let field = |key: &str| -> Option<Verdict> {
        let needle = alloc::format!("\"{}\":\"", key);
        let start = json.find(&needle)? + needle.len();
        let rest = &json[start..];
        let end = rest.find('"')?;
        Some(Verdict::from_label(&rest[..end]))
    };
    const KNOWN: [&str; 5] = [
        R_NET_LINK_DOWN,
        R_NET_SLIP_DEGRADED,
        R_STORAGE_NO_PERSIST,
        R_STORAGE_FLUSH_FAIL,
        R_GPU_QUARANTINE,
    ];
    let reasons = json
        .find("\"reasons\":[")
        .map(|start| {
            let rest = &json[start + "\"reasons\":[".len()..];
            let end = rest.find(']').unwrap_or(0);
            rest[..end]
                .split(',')
                .filter_map(|item| {
                    let item = item.trim().trim_matches('"');
                    KNOWN.iter().copied().find(|k| *k == item)
                })
                .collect::<alloc::vec::Vec<_>>()
        })
        .unwrap_or_default();
    Some((
        SysHealthVerdicts {
            net: field("net")?,
            storage: field("storage")?,
            gpu: field("gpu")?,
        },
        reasons,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_nano::sys_health::EscalationState;

    const JSON_NET_NOGO: &str =
        "{\"net\":\"NO_GO\",\"storage\":\"GO\",\"gpu\":\"UNKNOWN\",\"reasons\":[\"NET_LINK_DOWN\"]}";
    const JSON_ALL_OK: &str =
        "{\"net\":\"GO\",\"storage\":\"GO\",\"gpu\":\"UNKNOWN\",\"reasons\":[]}";

    #[test]
    fn parse_json_propio() {
        let (v, r) = parse_own_json(JSON_NET_NOGO).unwrap();
        assert_eq!(v.net, Verdict::NoGo);
        assert_eq!(v.storage, Verdict::Go);
        assert_eq!(v.gpu, Verdict::Unknown);
        assert_eq!(r, alloc::vec![R_NET_LINK_DOWN]);
    }

    #[test]
    fn razao_mapeada_por_subsistema() {
        assert_eq!(reason_for(SysSubsystem::Net, &[R_NET_LINK_DOWN]), Some(R_NET_LINK_DOWN));
        // Razão de storage não afeta net — subsistema UNKNOWN sem razão própria.
        assert_eq!(reason_for(SysSubsystem::Net, &[R_STORAGE_FLUSH_FAIL]), None);
        assert_eq!(reason_for(SysSubsystem::Gpu, &[R_NET_LINK_DOWN]), None);
        assert_eq!(reason_for(SysSubsystem::Gpu, &[R_GPU_QUARANTINE]), Some(R_GPU_QUARANTINE));
    }

    #[test]
    fn no_go_persistente_escala_uma_vez_por_incidente() {
        let mut agent = SysHealthAgent::new();
        for _ in 0..NO_GO_PERSIST_MIN {
            let (v, r) = parse_own_json(JSON_NET_NOGO).unwrap();
            agent.observe_and_escalate(&v, &r, JSON_NET_NOGO);
        }
        assert_eq!(agent.escalations, 1);
        // Persiste: sem re-escala (dedupe por incidente).
        for _ in 0..NO_GO_PERSIST_MIN {
            let (v, r) = parse_own_json(JSON_NET_NOGO).unwrap();
            agent.observe_and_escalate(&v, &r, JSON_NET_NOGO);
        }
        assert_eq!(agent.escalations, 1);
        // Recupera (GO fecha incidente) e recai: escala de novo.
        let (v, r) = parse_own_json(JSON_ALL_OK).unwrap();
        agent.observe_and_escalate(&v, &r, JSON_ALL_OK);
        for _ in 0..NO_GO_PERSIST_MIN {
            let (v, r) = parse_own_json(JSON_NET_NOGO).unwrap();
            agent.observe_and_escalate(&v, &r, JSON_NET_NOGO);
        }
        assert_eq!(agent.escalations, 2);
    }

    #[test]
    fn unknown_nunca_escala() {
        let mut agent = SysHealthAgent::new();
        let json = "{\"net\":\"UNKNOWN\",\"storage\":\"UNKNOWN\",\"gpu\":\"UNKNOWN\",\"reasons\":[]}";
        for _ in 0..NO_GO_PERSIST_MIN * 3 {
            let (v, r) = parse_own_json(json).unwrap();
            agent.observe_and_escalate(&v, &r, json);
        }
        assert_eq!(agent.escalations, 0);
    }

    #[test]
    fn storage_sem_persist_e_unknown_no_qemu_loader() {
        // Nunca persistiu: UNKNOWN (a IA decide com evidência, não com ausência).
        let inp = SysHealthInputs { storage_persisted: false, ..Default::default() };
        let (v, r) = k_nano::sys_health::sys_health(inp);
        assert_eq!(v.storage, Verdict::Unknown);
        assert!(!r.contains(&R_STORAGE_NO_PERSIST));
    }

    #[test]
    fn escalation_state_compartilhado_com_audio() {
        // Política única: mesmo tipo, mesma janela, mesma semântica.
        let mut st = EscalationState::default();
        for _ in 0..NO_GO_PERSIST_MIN - 1 {
            st.observe(Verdict::NoGo, Some(R_NET_LINK_DOWN));
        }
        assert_eq!(st.observe(Verdict::NoGo, Some(R_NET_LINK_DOWN)), Some(R_NET_LINK_DOWN));
    }
}
