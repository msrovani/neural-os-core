//! HDA BE — facade sobre `k_nano::audio::hda` (IRQ IDT 0x30).
//! Oneshot não reseta GCTL se o bring-up do k_nano já rodou (J-04).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

pub use k_nano::audio::hda::{poll_hda_audio, write_hda_playback};

const HDA_MANIFEST: AgentManifest = AgentManifest {
    name: "hda_audio",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

pub struct HdaAudioAgent;

impl HdaAudioAgent {
    pub fn new() -> Self {
        HdaAudioAgent
    }
}

impl Agent for HdaAudioAgent {
    fn manifest(&self) -> &AgentManifest {
        &HDA_MANIFEST
    }
    fn tick(&mut self, _t: u64, _c: u64) -> AgentTickResult {
        if k_nano::audio::hda::is_ready() {
            crate::audio::register_hda_bound();
            k_nano::slog_hal!(
                "HDA",
                "info",
                "ja bound via k_nano (IRQ 0x30) — skip GCTL reset"
            );
            return AgentTickResult::Done;
        }
        if k_nano::audio::hda::init_hda() {
            crate::audio::register_hda_bound();
            k_nano::slog_hal!("HDA", "info", "init via k_nano (agent fallback, mesma instancia)");
        } else {
            k_nano::slog_hal!("HDA", "info", "Nenhum controlador Intel HDA encontrado");
        }
        AgentTickResult::Done
    }
}
