//! Hub Health — política do painel diagnóstico (F12 / clique no orb / badge).
//!
//! Decisão do mantenedor: o AGENTE é dono da política (abrir/fechar, pill,
//! auto-open 8s pós-HEALTH_ISSUE, pior subsystema) e publica `HUB_HEALTH_STATE`;
//! o compositor (jarbas) SÓ renderiza o snapshot lendo os statics lock-free
//! abaixo. Nenhuma decisão de UI no paint path.
//!
//! Dados (linhas do painel) continuam no fill 2 Hz de
//! `jarbas::display::gauges::refresh_hub_health`, que escreve `set_sample`.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use event_bus::{CapabilityToken, Event, Receiver};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

use k_nano::EVENT_BUS;

pub const TOPIC_HUB_HEALTH_CMD: &str = "HUB_HEALTH_CMD";
pub const TOPIC_HUB_HEALTH_STATE: &str = "HUB_HEALTH_STATE";

// ── Estado lock-free (agente escreve, compositor lê) ────────────────────────
// gen incrementa a cada mudança de estado — o compositor compara e marca
// dirty_panel. Sem locks no caminho do render.
static PANEL_GEN: AtomicU64 = AtomicU64::new(0);
static PANEL_VISIBLE: AtomicBool = AtomicBool::new(false);
/// Pill do header: 0=ok 1=warn 2=fail 3=n/a (código, não cor — a cor é render).
static PANEL_PILL: AtomicU8 = AtomicU8::new(3);
/// Índice da pior linha (0..); 0xFF = nenhuma (todas n/a).
static PANEL_WORST_ROW: AtomicU8 = AtomicU8::new(0xFF);

// ── Amostra (fill 2 Hz no jarbas escreve; agente lê) ────────────────────────
static SAMPLE_PILL: AtomicU8 = AtomicU8::new(3);
static SAMPLE_WORST_ROW: AtomicU8 = AtomicU8::new(0xFF);

/// Escrito pelo fill 2 Hz do snapshot (não é política — é dado amostrado).
pub fn set_sample(pill_sev: u8, worst_row: u8) {
    SAMPLE_PILL.store(pill_sev, Ordering::Relaxed);
    SAMPLE_WORST_ROW.store(worst_row, Ordering::Relaxed);
}

pub fn panel_gen() -> u64 { PANEL_GEN.load(Ordering::Acquire) }
pub fn panel_visible() -> bool { PANEL_VISIBLE.load(Ordering::Acquire) }
pub fn panel_pill() -> u8 { PANEL_PILL.load(Ordering::Relaxed) }
pub fn panel_worst_row() -> u8 { PANEL_WORST_ROW.load(Ordering::Relaxed) }

const HUB_MANIFEST: AgentManifest = AgentManifest {
    name: "hub_health_agent",
    kind: AgentKind::System,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: true,
};

/// Política do painel Hub Health (EventDriven).
///
/// Modelo de decisão:
/// - `HUB_HEALTH_CMD` (`toggle`/`open`/`close`) → F12, clique no orb, badge, Esc.
/// - `HEALTH_ISSUE` → auto-open 8s (depois fecha sozinho).
/// - `MESH_HEALTH`/`TIMER_CAP` → força republicação do estado (dados mudaram).
/// - ~2 Hz: lê a amostra (`set_sample`) e republica se pill/worst mudaram.
/// Estado visível = `PANEL_*` + evento `HUB_HEALTH_STATE` (payload `v<p>l<r>`).
pub struct HubHealthAgent {
    cmd_receiver: Receiver,
    health_receiver: Receiver,
    mesh_receiver: Receiver,
    timer_receiver: Receiver,
    auto_close_us: u64,
    next_sample_us: u64,
}

impl HubHealthAgent {
    pub fn new() -> Self {
        HubHealthAgent {
            cmd_receiver: EVENT_BUS.subscribe(TOPIC_HUB_HEALTH_CMD),
            health_receiver: EVENT_BUS.subscribe("HEALTH_ISSUE"),
            mesh_receiver: EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_MESH_HEALTH),
            timer_receiver: EVENT_BUS.subscribe(k_hal::timer_cap::TOPIC_TIMER_CAP),
            auto_close_us: 0,
            next_sample_us: 0,
        }
    }

    /// Aplica estado e publica só quando muda (evento pequeno, sem spam).
    fn apply(&mut self, visible: bool, pill: u8, worst_row: u8) {
        let vis_changed = PANEL_VISIBLE.swap(visible, Ordering::AcqRel) != visible;
        let pill_changed = PANEL_PILL.swap(pill, Ordering::AcqRel) != pill;
        let row_changed = PANEL_WORST_ROW.swap(worst_row, Ordering::AcqRel) != worst_row;
        if vis_changed || pill_changed || row_changed {
            PANEL_GEN.fetch_add(1, Ordering::AcqRel);
            let payload = alloc::format!("v{}p{}r{}", visible as u8, pill, worst_row);
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from(TOPIC_HUB_HEALTH_STATE),
                payload: payload.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }
    }

    fn refresh_from_sample(&mut self) {
        let visible = PANEL_VISIBLE.load(Ordering::Relaxed);
        self.apply(visible, SAMPLE_PILL.load(Ordering::Relaxed), SAMPLE_WORST_ROW.load(Ordering::Relaxed));
    }

    fn handle_cmd(&mut self, payload: &[u8]) {
        let cmd = core::str::from_utf8(payload).unwrap_or("");
        let sample = (SAMPLE_PILL.load(Ordering::Relaxed), SAMPLE_WORST_ROW.load(Ordering::Relaxed));
        match cmd {
            // Toggle manual cancela o auto-close do HEALTH_ISSUE (controlado pelo usuário).
            "toggle" => {
                self.auto_close_us = 0;
                self.apply(!PANEL_VISIBLE.load(Ordering::Relaxed), sample.0, sample.1);
            }
            "open" => {
                self.auto_close_us = 0;
                self.apply(true, sample.0, sample.1);
            }
            "close" => {
                self.auto_close_us = 0;
                self.apply(false, sample.0, sample.1);
            }
            _ => {}
        }
    }
}

impl Agent for HubHealthAgent {
    fn manifest(&self) -> &AgentManifest { &HUB_MANIFEST }

    fn has_pending(&self) -> bool {
        self.cmd_receiver.has_pending()
            || self.health_receiver.has_pending()
            || self.mesh_receiver.has_pending()
            || self.timer_receiver.has_pending()
            || (self.next_sample_us != 0 && k_nano::tsc::now_us() >= self.next_sample_us)
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(ev) = self.cmd_receiver.try_receive() {
            self.handle_cmd(&ev.payload);
        }
        while let Some(ev) = self.health_receiver.try_receive() {
            let _ = core::str::from_utf8(&ev.payload).unwrap_or("");
            let now = k_nano::tsc::now_us();
            self.auto_close_us = if now == 0 { 0 } else { now + 8_000_000 };
            self.apply(true, SAMPLE_PILL.load(Ordering::Relaxed), SAMPLE_WORST_ROW.load(Ordering::Relaxed));
        }
        while self.mesh_receiver.try_receive().is_some() {}
        while self.timer_receiver.try_receive().is_some() {}

        // Amostragem ~2 Hz: pega pill/worst novos + auto-close expirado.
        let now = k_nano::tsc::now_us();
        if now >= self.next_sample_us {
            self.next_sample_us = if now == 0 { 500_000 } else { now + 500_000 };
            if self.auto_close_us != 0 && now >= self.auto_close_us {
                self.auto_close_us = 0;
                self.apply(false, SAMPLE_PILL.load(Ordering::Relaxed), SAMPLE_WORST_ROW.load(Ordering::Relaxed));
            } else {
                self.refresh_from_sample();
            }
        }
        AgentTickResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_statics_default_na() {
        assert_eq!(SAMPLE_PILL.load(Ordering::Relaxed), 3);
        assert_eq!(SAMPLE_WORST_ROW.load(Ordering::Relaxed), 0xFF);
        assert!(!PANEL_VISIBLE.load(Ordering::Relaxed));
    }

    #[test]
    fn set_sample_updates_statics() {
        set_sample(2, 5);
        assert_eq!(SAMPLE_PILL.load(Ordering::Relaxed), 2);
        assert_eq!(SAMPLE_WORST_ROW.load(Ordering::Relaxed), 5);
        set_sample(3, 0xFF);
    }
}
