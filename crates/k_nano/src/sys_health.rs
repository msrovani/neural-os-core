//! SYS_HEALTH — veredito GO/NO-GO/UNKNOWN unificado dos subsistemas da máquina
//! (net / storage / gpu) para a IA (AIOS, premissa máxima ADR-0088).
//!
//! Complementa o AUDIO_HEALTH do jarbas: mesmo padrão de veredito, JSON
//! compacto (padrão MESH_HEALTH) e `EscalationState` compartilhado — a política
//! de escala ao LLM (janela de persistência + dedupe por incidente, anti-loop
//! SESSION_410) é UMA, definida aqui, e consumida por hermes (áudio + sistema).
//!
//! Formato: `{"net":"GO","storage":"GO","gpu":"UNKNOWN","reasons":["NET_LINK_DOWN"]}`
//!
//! Regras (puras, host-testáveis) — herdadas do audio_health do jarbas:
//! - GO só com evidência POSITIVA (contador/flag andando). Ausência de falha ≠ GO.
//! - NO_GO nunca mascarado (erro ativo ou evidência negativa direta).
//! - UNKNOWN = sem evidência nesta janela (não é falha; não conta no streak).

use core::sync::atomic::{AtomicU64, Ordering};

// ── Veredito de MÁQUINA (consolidação) — SESSION_417 ──────────────────────────
// Consolidação em um veredito único da máquina: worst-of entre os JSONs
// (SYS_HEALTH + AUDIO_HEALTH) com razões agregadas — consumido pelo LLM num
// prompt só e propagado via mesh para o Master agregar a frota.

/// Conjunto de vereditos de um domínio de saúde (ex.: sys = net/storage/gpu,
/// audio = playback/capture/voice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainVerdicts {
    /// Pares (nome do campo, veredito) — ordem fixa do produtor.
    pub fields: alloc::vec::Vec<(&'static str, Verdict)>,
    /// Razões NO_GO agregadas (constantes estáveis, sem duplicatas).
    pub reasons: alloc::vec::Vec<&'static str>,
}

impl DomainVerdicts {
    /// Pior veredito do domínio: NO_GO > UNKNOWN > GO (UNKNOWN contamina:
    /// sem evidência não se afirma saúde).
    pub fn worst(&self) -> Verdict {
        let mut w = Verdict::Go;
        for (_, v) in &self.fields {
            w = match (w, *v) {
                (Verdict::NoGo, _) | (_, Verdict::NoGo) => Verdict::NoGo,
                (Verdict::Unknown, _) | (_, Verdict::Unknown) => Verdict::Unknown,
                _ => Verdict::Go,
            };
        }
        w
    }
}

/// Veredito consolidado de uma máquina (ou da frota agregada no Master).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineHealth {
    /// ID do nó (0 = agregado de frota no Master).
    pub node: u8,
    /// Domínios consolidados ("sys", "audio", …).
    pub domains: alloc::vec::Vec<(&'static str, DomainVerdicts)>,
}

impl MachineHealth {
    /// Pior veredito da máquina: NO_GO > UNKNOWN > GO.
    pub fn overall(&self) -> Verdict {
        let mut w = Verdict::Go;
        for (_, d) in &self.domains {
            w = match (w, d.worst()) {
                (Verdict::NoGo, _) | (_, Verdict::NoGo) => Verdict::NoGo,
                (Verdict::Unknown, _) | (_, Verdict::Unknown) => Verdict::Unknown,
                _ => Verdict::Go,
            };
        }
        w
    }

    /// Razões globais dedupe-preservando ordem.
    pub fn all_reasons(&self) -> alloc::vec::Vec<&'static str> {
        let mut seen: alloc::vec::Vec<&'static str> = alloc::vec::Vec::new();
        for (_, d) in &self.domains {
            for r in &d.reasons {
                if !seen.contains(r) {
                    seen.push(r);
                }
            }
        }
        seen
    }
}

/// Consolidação worst-of de uma máquina (razões dedupe em `all_reasons`).
pub fn machine_verdict(
    node: u8,
    domains: alloc::vec::Vec<(&'static str, DomainVerdicts)>,
) -> MachineHealth {
    MachineHealth { node, domains }
}

/// Agregação de FROTA no Master: worst-of entre nós, razões agregadas com
/// prefixo `node=N:` para o LLM saber de quem vem (dedupe por (razão, nó)).
pub fn fleet_worst(nodes: &[MachineHealth]) -> MachineHealth {
    let mut fields: alloc::vec::Vec<(&'static str, Verdict)> = alloc::vec::Vec::new();
    let mut reasons: alloc::vec::Vec<&'static str> = alloc::vec::Vec::new();
    // Campos: união dos nomes de campo do primeiro domínio de mesmo nome.
    // Simplificação honesta: domínios com nomes iguais têm campos iguais
    // (contrato dos produtores); agregamos por posição do domínio.
    let n_dom = nodes.iter().map(|m| m.domains.len()).max().unwrap_or(0);
    for di in 0..n_dom {
        let first_fields: alloc::vec::Vec<&'static str> = nodes
            .iter()
            .filter_map(|m| m.domains.get(di))
            .next()
            .map(|d| d.1.fields.iter().map(|(n, _)| *n).collect())
            .unwrap_or_default();
        for fname in first_fields {
            if fields.iter().any(|(n, _)| *n == fname) {
                continue;
            }
            // worst-of do campo entre todos os nós que o têm.
            let mut w = Verdict::Go;
            for m in nodes {
                if let Some((_, d)) = m.domains.get(di) {
                    if let Some((_, v)) = d.fields.iter().find(|(n, _)| *n == fname) {
                        w = match (w, *v) {
                            (Verdict::NoGo, _) | (_, Verdict::NoGo) => Verdict::NoGo,
                            (Verdict::Unknown, _) | (_, Verdict::Unknown) => Verdict::Unknown,
                            _ => Verdict::Go,
                        };
                    }
                }
            }
            fields.push((fname, w));
        }
    }
    for m in nodes {
        for r in m.all_reasons() {
            if !reasons.contains(&r) {
                reasons.push(r);
            }
        }
    }
    MachineHealth { node: 0, domains: alloc::vec![("fleet", DomainVerdicts { fields, reasons })] }
}

/// JSON do veredito de máquina (padrão MESH_HEALTH — parse no_std no Master).
/// `{"node":15,"overall":"NO_GO","sys":{"net":"NO_GO","storage":"GO","gpu":"UNKNOWN"},"reasons":["NET_LINK_DOWN"]}`
pub fn machine_verdict_json(m: &MachineHealth) -> alloc::string::String {
    let mut s = alloc::string::String::from("{\"node\":");
    s.push_str(alloc::format!("{}", m.node).as_str());
    s.push_str(",\"overall\":\"");
    s.push_str(m.overall().label());
    s.push_str("\",");
    for (i, (name, d)) in m.domains.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(name);
        s.push_str("\":{");
        for (j, (field, v)) in d.fields.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            s.push('"');
            s.push_str(field);
            s.push_str("\":\"");
            s.push_str(v.label());
            s.push('"');
        }
        s.push('}');
    }
    s.push_str(",\"reasons\":[");
    let reasons = m.all_reasons();
    for (i, r) in reasons.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(r);
        s.push('"');
    }
    s.push_str("]}");
    s
}

/// Parse mínimo do JSON de máquina (contrato fixo do `machine_verdict_json`).
/// Retorna (node, overall, [(domínio, campo, veredito)], razões).
pub fn parse_machine_json(
    json: &str,
) -> Option<(u8, Verdict, alloc::vec::Vec<(&'static str, &'static str, Verdict)>, alloc::vec::Vec<alloc::string::String>)> {
    let get_field = |key: &str| -> Option<alloc::string::String> {
        let needle = alloc::format!("\"{}\":\"", key);
        let start = json.find(&needle)? + needle.len();
        let rest = &json[start..];
        let end = rest.find('"')?;
        Some(alloc::string::String::from(&rest[..end]))
    };
    // node é número no JSON: `"node":15` (sem aspas).
    let node: u8 = {
        let needle = "\"node\":";
        let start = json.find(needle)? + needle.len();
        let rest = &json[start..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[..end].parse().ok()?
    };
    let overall_label = get_field("overall")?;
    let overall = Verdict::from_label(&overall_label);
    // Domínios: pares "nome":{...} — procura cada domínio conhecido.
    let mut triplets = alloc::vec::Vec::new();
    for dom in ["sys", "audio"] {
        let dkey = alloc::format!("\"{}\":{{", dom);
        let start = json.find(&dkey)? + dkey.len();
        let rest = &json[start..];
        let end = rest.find('}')?;
        let body = &rest[..end];
        for pair in body.split(',') {
            let mut it = pair.splitn(2, ':');
            let field = it.next()?.trim().trim_matches('"');
            let val = it.next()?.trim().trim_matches('"');
            // Campos são estáticos conhecidos (contrato dos produtores).
            const FIELDS: [&str; 6] = ["net", "storage", "gpu", "playback", "capture", "voice"];
            if let Some(f) = FIELDS.iter().find(|f| **f == field) {
                triplets.push((*f, dom, Verdict::from_label(val)));
            }
        }
    }
    // Razões: array de strings conhecidas (mesma tabela do parse_own_json).
    let mut reasons = alloc::vec::Vec::new();
    if let Some(start) = json.find("\"reasons\":[") {
        let rest = &json[start + "\"reasons\":[".len()..];
        let end = rest.find(']').unwrap_or(0);
        for item in rest[..end].split(',') {
            let item = item.trim().trim_matches('"');
            if !item.is_empty() {
                reasons.push(alloc::string::String::from(item));
            }
        }
    }
    Some((node, overall, triplets, reasons))
}

/// Tópico no EventBus.
pub const TOPIC_SYS_HEALTH: &str = "SYS_HEALTH";

/// Veredito por subsistema (único — compartilhado com audio_health do jarbas).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Evidência positiva de funcionamento.
    Go,
    /// Erro ativo ou evidência negativa direta.
    NoGo,
    /// Sem evidência na janela (não é falha; não conta no streak de escala).
    Unknown,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Go => "GO",
            Verdict::NoGo => "NO_GO",
            Verdict::Unknown => "UNKNOWN",
        }
    }

    pub fn from_label(s: &str) -> Self {
        match s {
            "GO" => Verdict::Go,
            "NO_GO" => Verdict::NoGo,
            _ => Verdict::Unknown,
        }
    }
}

// ── Política de escala (fonte única — hermes::audio_health reusa) ───────────

/// Janela de persistência: NO_GO consecutivos antes de escalar ao LLM.
/// Amostras são 1×/s; 30 = ~30 s de defeito contínuo antes de acordar o LLM.
pub const NO_GO_PERSIST_MIN: u32 = 30;

/// Estado acumulado de um subsistema na janela de persistência.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EscalationState {
    /// NO_GO consecutivos desde a última observação não-NO_GO.
    pub no_go_streak: u32,
    /// Razão que abriu o streak (primeira razão NO_GO da janela). `None` = OK.
    pub streak_reason: Option<&'static str>,
    /// Já escalou neste incidente (dedupe por razão).
    pub escalated_for: Option<&'static str>,
}

impl Default for EscalationState {
    fn default() -> Self {
        EscalationState {
            no_go_streak: 0,
            streak_reason: None,
            escalated_for: None,
        }
    }
}

impl EscalationState {
    /// Observa um veredito e decide se deve escalar ao LLM.
    ///
    /// Retorna `Some(reason)` uma única vez por incidente:
    /// - streak >= `NO_GO_PERSIST_MIN` **e** `escalated_for != razão atual`.
    /// - GO fecha o incidente (reabre com a mesma razão depois).
    /// - UNKNOWN não conta (não é falha) e não zera o streak.
    /// - Troca de razão reinicia a janela (defeito novo = janela própria).
    pub fn observe(&mut self, verdict: Verdict, reason: Option<&'static str>) -> Option<&'static str> {
        match verdict {
            Verdict::Go => {
                *self = EscalationState::default();
                None
            }
            Verdict::Unknown => None,
            Verdict::NoGo => {
                let reason = reason.unwrap_or("UNKNOWN_REASON");
                if self.streak_reason != Some(reason) {
                    self.no_go_streak = 0;
                    self.streak_reason = Some(reason);
                }
                self.no_go_streak = self.no_go_streak.saturating_add(1);
                if self.no_go_streak >= NO_GO_PERSIST_MIN && self.escalated_for != Some(reason) {
                    self.escalated_for = Some(reason);
                    self.no_go_streak = 0;
                    Some(reason)
                } else {
                    None
                }
            }
        }
    }
}

// ── Entradas (snapshot dos produtores) ──────────────────────────────────────

/// Inputs medidos de net/storage/gpu (o produtor preenche a partir dos
/// contadores reais — nunca de flag derivada sem fonte).
#[derive(Debug, Clone, Copy, Default)]
pub struct SysHealthInputs {
    /// Link físico medido no boot (`env::net_link_ok`).
    pub net_link: bool,
    /// Pacotes RX desde o último snapshot (smoltcp stack).
    pub net_rx_delta: u64,
    /// Flag SLIP degradado (SLIP não é Net gate — honesto).
    pub net_slip_degraded: bool,
    /// BOOT.LOG já gravou em FAT ao menos 1× (persistência provada).
    pub storage_persisted: bool,
    /// Flushes com sucesso desde o último snapshot.
    pub storage_flush_ok_delta: u64,
    /// Flushes falhos desde o último snapshot.
    pub storage_flush_fail_delta: u64,
    /// Estado do compute backend (canário vector_add golden).
    pub gpu_backend: GpuBackend,
    /// Compute executa desde o último snapshot (work real submetido).
    pub gpu_compute_delta: u64,
}

/// Estado do GPU backend (espelho de k_hal::gpu::compute_abi::BackendState —
/// copiado aqui para manter o modelo puro sem depender de k_hal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuBackend {
    /// Detectado; compute ainda não validado (sem evidência → UNKNOWN).
    #[default]
    Probed,
    /// Bring-up em curso.
    BringingUp,
    /// Canário vector_add golden passou.
    Ready,
    /// Falha honesta — usar CPU (NO_GO de compute acelerado).
    Quarantine,
    /// Sem GPU / CPU-only (estado esperado em QEMU — não é falha).
    CpuOnly,
}

impl GpuBackend {
    pub fn from_backend_state(b: u8) -> Self {
        // ordem dos variantes em compute_abi.rs: Probed/BringingUp/Ready/Quarantine/CpuOnly
        match b {
            0 => GpuBackend::Probed,
            1 => GpuBackend::BringingUp,
            2 => GpuBackend::Ready,
            3 => GpuBackend::Quarantine,
            _ => GpuBackend::CpuOnly,
        }
    }
}

// ── Razões estáveis (a IA aprende com elas) ─────────────────────────────────
pub const R_NET_LINK_DOWN: &str = "NET_LINK_DOWN";
pub const R_NET_SLIP_DEGRADED: &str = "NET_SLIP_DEGRADED";
pub const R_STORAGE_NO_PERSIST: &str = "STORAGE_NO_PERSIST";
pub const R_STORAGE_FLUSH_FAIL: &str = "STORAGE_FLUSH_FAIL";
pub const R_GPU_QUARANTINE: &str = "GPU_COMPUTE_QUARANTINE";

/// Espelho da razão de capture do audio_health (const local — k_nano não
/// depende de jarbas; mesmo contrato de string).
pub const R_CAPTURE_LPIB_STALE_F: &str = "SD_CAPTURE_LPIB_STALE";

/// Vereditos por subsistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysHealthVerdicts {
    pub net: Verdict,
    pub storage: Verdict,
    pub gpu: Verdict,
}

/// Regra pura: contadores medidos → vereditos + razões.
pub fn sys_health(inputs: SysHealthInputs) -> (SysHealthVerdicts, alloc::vec::Vec<&'static str>) {
    let mut reasons = alloc::vec::Vec::new();

    // ── Net ─────────────────────────────────────────────────────────────────
    let net = if inputs.net_link {
        if inputs.net_rx_delta == 0 {
            // Link up mas nenhum pacote: pode ser rede quieta — UNKNOWN (sem
            // evidência de tráfego), não NO_GO de HW.
            Verdict::Unknown
        } else {
            Verdict::Go
        }
    } else if inputs.net_slip_degraded {
        reasons.push(R_NET_SLIP_DEGRADED);
        Verdict::NoGo
    } else {
        reasons.push(R_NET_LINK_DOWN);
        Verdict::NoGo
    };

    // ── Storage (persistência BOOT.LOG / dados) ─────────────────────────────
    let storage = if inputs.storage_flush_fail_delta > 0 {
        reasons.push(R_STORAGE_FLUSH_FAIL);
        Verdict::NoGo
    } else if inputs.storage_persisted {
        Verdict::Go
    } else {
        // Nunca persistiu: no QEMU-loader (sem disco) é esperado; honesto é
        // UNKNOWN até a primeira evidência (a IA decide com dado, não com ausência).
        Verdict::Unknown
    };

    // ── GPU (compute acelerado) ─────────────────────────────────────────────
    let gpu = match inputs.gpu_backend {
        GpuBackend::Ready => Verdict::Go,
        GpuBackend::Quarantine => {
            reasons.push(R_GPU_QUARANTINE);
            Verdict::NoGo
        }
        // CpuOnly é estado legítimo (QEMU/VBox); Probed/BringingUp = sem evidência.
        _ => Verdict::Unknown,
    };

    (SysHealthVerdicts { net, storage, gpu }, reasons)
}

/// Serializa o snapshot (padrão MESH_HEALTH JSON compacto).
pub fn to_json(v: &SysHealthVerdicts, reasons: &[&str]) -> alloc::string::String {
    let mut s = alloc::string::String::from("{\"net\":\"");
    s.push_str(v.net.label());
    s.push_str("\",\"storage\":\"");
    s.push_str(v.storage.label());
    s.push_str("\",\"gpu\":\"");
    s.push_str(v.gpu.label());
    s.push_str("\",\"reasons\":[");
    for (i, r) in reasons.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(r);
        s.push('"');
    }
    s.push_str("]}");
    s
}

// ── Estado do publicador ────────────────────────────────────────────────────
static LAST_NET_RX: AtomicU64 = AtomicU64::new(0);
static LAST_FLUSH_OK: AtomicU64 = AtomicU64::new(0);
static LAST_FLUSH_FAIL: AtomicU64 = AtomicU64::new(0);
static LAST_GPU_COMPUTE: AtomicU64 = AtomicU64::new(0);
static LAST_PUBLISH_TICK: AtomicU64 = AtomicU64::new(0);

/// Último JSON publicado (HUD/consumidores leem sem re-parse do bus).
pub static LAST_SYS_HEALTH_JSON: spin::Mutex<Option<alloc::string::String>> = spin::Mutex::new(None);

/// Contadores de flush do boot_logger (o produtor de storage incrementa —
/// escrita 1 linha, leitura lock-free daqui e do HUD).
pub static STORAGE_FLUSH_OK: AtomicU64 = AtomicU64::new(0);
pub static STORAGE_FLUSH_FAIL: AtomicU64 = AtomicU64::new(0);

/// Período de publicação: 1 s (igual AUDIO_HEALTH — 100 ticks @100 Hz).
pub const PUBLISH_PERIOD_TICKS: u64 = 100;

/// Publica SYS_HEALTH no EventBus se passou o período. Chamado pelo agente
/// produtor (hermes::sys_health::SysHealthAgent tick — R2 decide, R0 guarda
/// só o modelo puro). Sem alocação fora do publish (1×/s).
pub fn publish_sys_health(mut inputs: SysHealthInputs, tick: u64) {
    let last = LAST_PUBLISH_TICK.load(Ordering::Relaxed);
    if last != 0 && tick.saturating_sub(last) < PUBLISH_PERIOD_TICKS {
        return;
    }
    LAST_PUBLISH_TICK.store(tick, Ordering::Relaxed);

    inputs.net_rx_delta = hermes_netstack_rx()
        .saturating_sub(LAST_NET_RX.swap(hermes_netstack_rx(), Ordering::Relaxed));
    inputs.storage_flush_ok_delta = STORAGE_FLUSH_OK.load(Ordering::Relaxed)
        .saturating_sub(LAST_FLUSH_OK.swap(STORAGE_FLUSH_OK.load(Ordering::Relaxed), Ordering::Relaxed));
    inputs.storage_flush_fail_delta = STORAGE_FLUSH_FAIL.load(Ordering::Relaxed)
        .saturating_sub(LAST_FLUSH_FAIL.swap(STORAGE_FLUSH_FAIL.load(Ordering::Relaxed), Ordering::Relaxed));
    inputs.gpu_compute_delta = gpu_compute_count()
        .saturating_sub(LAST_GPU_COMPUTE.swap(gpu_compute_count(), Ordering::Relaxed));

    let (verdicts, reasons) = sys_health(inputs);
    let json = to_json(&verdicts, &reasons);

    let _ = crate::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_SYS_HEALTH),
        payload: json.clone().into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    *LAST_SYS_HEALTH_JSON.lock() = Some(json);

    // slog ok/warn (padrão ADR-0092): saúde visível no dmesg quando degrada.
    let all_ok = verdicts.net != Verdict::NoGo
        && verdicts.storage != Verdict::NoGo
        && verdicts.gpu != Verdict::NoGo;
    let sev = if all_ok { "ok" } else { "warn" };
    crate::slog_nano!(
        "SysHealth",
        sev,
        "SYS_HEALTH net={} storage={} gpu={} reasons={}",
        verdicts.net.label(),
        verdicts.storage.label(),
        verdicts.gpu.label(),
        reasons.len()
    );
}

// ── Fontes de evidência (R0 — leituras lock-free dos statics reais) ─────────

/// RX da netstack (hermes::netstack::net_rx_count) — injetado via seam para
/// não criar dependência k_nano → hermes (seria ciclo). O produtor hermes
/// registra a função na init do agente.
static NET_RX_SOURCE: spin::Mutex<Option<fn() -> u64>> = spin::Mutex::new(None);

/// Registra a fonte de RX (chamado 1× pelo agente produtor no hermes).
pub fn register_net_rx_source(f: fn() -> u64) {
    *NET_RX_SOURCE.lock() = Some(f);
}

/// Contador de compute GPU executado (o backend incrementa via
/// `note_gpu_compute`; hook que não cria dependência k_hal → k_nano reversa).
pub static GPU_COMPUTE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Chamado pelo backend de GPU quando um trabalho de compute completa.
pub fn note_gpu_compute(n: u64) {
    GPU_COMPUTE_COUNT.fetch_add(n.max(1), Ordering::Relaxed);
}

fn hermes_netstack_rx() -> u64 {
    NET_RX_SOURCE.lock().map(|f| f()).unwrap_or(0)
}

fn gpu_compute_count() -> u64 {
    GPU_COMPUTE_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tudo_com_evidencia_e_go() {
        let inp = SysHealthInputs {
            net_link: true,
            net_rx_delta: 10,
            storage_persisted: true,
            storage_flush_ok_delta: 1,
            gpu_backend: GpuBackend::Ready,
            gpu_compute_delta: 3,
            ..Default::default()
        };
        let (v, r) = sys_health(inp);
        assert_eq!(v.net, Verdict::Go);
        assert_eq!(v.storage, Verdict::Go);
        assert_eq!(v.gpu, Verdict::Go);
        assert!(r.is_empty());
    }

    #[test]
    fn net_sem_link_e_no_go_slip_e_razao_propria() {
        let inp = SysHealthInputs { net_link: false, net_slip_degraded: true, ..Default::default() };
        let (v, r) = sys_health(inp);
        assert_eq!(v.net, Verdict::NoGo);
        assert!(r.contains(&R_NET_SLIP_DEGRADED));
        assert!(!r.contains(&R_NET_LINK_DOWN));
    }

    #[test]
    fn link_up_sem_trafego_e_unknown() {
        let inp = SysHealthInputs { net_link: true, net_rx_delta: 0, ..Default::default() };
        let (v, _) = sys_health(inp);
        assert_eq!(v.net, Verdict::Unknown);
    }

    #[test]
    fn storage_flush_fail_e_no_go() {
        let inp = SysHealthInputs {
            storage_persisted: true,
            storage_flush_fail_delta: 2,
            ..Default::default()
        };
        let (v, r) = sys_health(inp);
        assert_eq!(v.storage, Verdict::NoGo);
        assert!(r.contains(&R_STORAGE_FLUSH_FAIL));
    }

    #[test]
    fn gpu_quarantine_e_no_go_cpu_only_e_unknown() {
        let inp = SysHealthInputs { gpu_backend: GpuBackend::Quarantine, ..Default::default() };
        let (v, r) = sys_health(inp);
        assert_eq!(v.gpu, Verdict::NoGo);
        assert!(r.contains(&R_GPU_QUARANTINE));

        let inp = SysHealthInputs { gpu_backend: GpuBackend::CpuOnly, ..Default::default() };
        let (v, _) = sys_health(inp);
        assert_eq!(v.gpu, Verdict::Unknown);
    }

    #[test]
    fn json_formato_mesh_health() {
        let v = SysHealthVerdicts { net: Verdict::Go, storage: Verdict::NoGo, gpu: Verdict::Unknown };
        let j = to_json(&v, &[R_STORAGE_FLUSH_FAIL]);
        assert_eq!(
            j.as_str(),
            "{\"net\":\"GO\",\"storage\":\"NO_GO\",\"gpu\":\"UNKNOWN\",\"reasons\":[\"STORAGE_FLUSH_FAIL\"]}"
        );
    }

    // ── SESSION_417: veredito de máquina + frota ──────────────────────────

    fn dom_sys(net: Verdict, storage: Verdict, gpu: Verdict) -> (&'static str, DomainVerdicts) {
        let mut reasons = alloc::vec::Vec::new();
        if net == Verdict::NoGo {
            reasons.push(R_NET_LINK_DOWN);
        }
        if storage == Verdict::NoGo {
            reasons.push(R_STORAGE_FLUSH_FAIL);
        }
        (
            "sys",
            DomainVerdicts {
                fields: alloc::vec![("net", net), ("storage", storage), ("gpu", gpu)],
                reasons,
            },
        )
    }

    fn dom_audio(playback: Verdict, capture: Verdict) -> (&'static str, DomainVerdicts) {
        (
            "audio",
            DomainVerdicts {
                fields: alloc::vec![("playback", playback), ("capture", capture)],
                reasons: if capture == Verdict::NoGo { alloc::vec![R_CAPTURE_LPIB_STALE_F] } else { alloc::vec![] },
            },
        )
    }

    #[test]
    fn machine_overall_e_worst_of() {
        // GO + UNKNOWN = UNKNOWN (sem evidência não afirma saúde).
        let m = machine_verdict(15, alloc::vec![dom_sys(Verdict::Go, Verdict::Go, Verdict::Unknown), dom_audio(Verdict::Go, Verdict::Go)]);
        assert_eq!(m.overall(), Verdict::Unknown);
        // Um NO_GO domina tudo.
        let m = machine_verdict(15, alloc::vec![dom_sys(Verdict::Go, Verdict::NoGo, Verdict::Unknown), dom_audio(Verdict::Go, Verdict::Go)]);
        assert_eq!(m.overall(), Verdict::NoGo);
        assert_eq!(m.all_reasons(), alloc::vec![R_STORAGE_FLUSH_FAIL]);
    }

    #[test]
    fn machine_json_roundtrip() {
        let m = machine_verdict(15, alloc::vec![dom_sys(Verdict::NoGo, Verdict::Go, Verdict::Unknown), dom_audio(Verdict::Go, Verdict::Unknown)]);
        let j = machine_verdict_json(&m);
        assert!(j.contains("\"node\":15"));
        assert!(
            j.contains("\"overall\":\"NO_GO\""),
            "json={}",
            j
        );
        let (node, overall, triplets, reasons) = parse_machine_json(&j).unwrap();
        assert_eq!(node, 15);
        assert_eq!(overall, Verdict::NoGo);
        assert!(triplets.contains(&("net", "sys", Verdict::NoGo)));
        assert!(triplets.contains(&("capture", "audio", Verdict::Unknown)));
        assert_eq!(reasons, alloc::vec![alloc::string::String::from(R_NET_LINK_DOWN)]);
    }

    #[test]
    fn fleet_worst_propaga_pior_veredito_e_razoes() {
        let a = machine_verdict(3, alloc::vec![dom_sys(Verdict::Go, Verdict::Go, Verdict::Go), dom_audio(Verdict::Go, Verdict::Go)]);
        let b = machine_verdict(7, alloc::vec![dom_sys(Verdict::NoGo, Verdict::Go, Verdict::Unknown), dom_audio(Verdict::Go, Verdict::Go)]);
        let f = fleet_worst(&[a, b]);
        assert_eq!(f.overall(), Verdict::NoGo);
        // net = NO_GO do nó 7; storage = GO; gpu = UNKNOWN (nó 7).
        let (_, d) = f.domains.first().unwrap();
        let net = d.fields.iter().find(|(n, _)| *n == "net").unwrap().1;
        let gpu = d.fields.iter().find(|(n, _)| *n == "gpu").unwrap().1;
        assert_eq!(net, Verdict::NoGo);
        assert_eq!(gpu, Verdict::Unknown);
        assert_eq!(f.all_reasons(), alloc::vec![R_NET_LINK_DOWN]);
    }

    #[test]
    fn escalation_state_politica_unica() {
        // Janela + dedupe + reabertura (mesma política do audio_health).
        let mut st = EscalationState::default();
        for _ in 0..NO_GO_PERSIST_MIN - 1 {
            assert!(st.observe(Verdict::NoGo, Some(R_NET_LINK_DOWN)).is_none());
        }
        assert_eq!(st.observe(Verdict::NoGo, Some(R_NET_LINK_DOWN)), Some(R_NET_LINK_DOWN));
        for _ in 0..NO_GO_PERSIST_MIN * 2 {
            assert!(st.observe(Verdict::NoGo, Some(R_NET_LINK_DOWN)).is_none());
        }
        assert!(st.observe(Verdict::Go, None).is_none());
        for _ in 0..NO_GO_PERSIST_MIN - 1 {
            assert_eq!(st.observe(Verdict::NoGo, Some(R_NET_LINK_DOWN)), None);
        }
        assert_eq!(st.observe(Verdict::NoGo, Some(R_NET_LINK_DOWN)), Some(R_NET_LINK_DOWN));
    }
}
