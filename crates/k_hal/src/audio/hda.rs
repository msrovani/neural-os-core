//! Intel HDA — facade BE sobre `k_nano::audio::hda` (verdade canônica).
//!
//! SESSION_316/322: o bring-up real (codec verbs, CORB/RIRB, BDL, DMA PMM)
//! vive em k_nano. Esta crate NÃO re-reseta o controller — isso destruía o
//! path de mic/speaker e podia corromper o tick cooperativo do compositor.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

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
        // DriverInit já chamou k_nano::audio::hda::init_hda(). Só anuncia Bound.
        if k_nano::audio::hda::is_ready() {
            crate::audio::register_hda_bound();
            k_nano::slog_hal!("HDA", "ok", "facade Bound — mic/playback via k_nano");
            return AgentTickResult::Done;
        }
        // Fallback: tenta init canônico (sem reimplementar reset local).
        let ok = unsafe { k_nano::audio::hda::init_hda() };
        if ok && k_nano::audio::hda::is_ready() {
            crate::audio::register_hda_bound();
            k_nano::slog_hal!("HDA", "ok", "init via k_nano — Bound");
        } else {
            k_nano::slog_hal!(
                "HDA",
                "warn",
                "sem controller/codec — TTS formant / mic ausente (honesto)"
            );
        }
        AgentTickResult::Done
    }
}

/// Poll captura → EventBus `AUDIO_IN` (path canônico k_nano).
pub fn poll_hda_audio() {
    k_nano::audio::hda::poll_hda_audio();
}

/// Escreve PCM no SD1 (path canônico k_nano).
pub fn write_hda_playback(samples: &[i16]) {
    k_nano::audio::hda::write_hda_playback(samples);
}

/// Pronto para I/O (BAR + streams armados).
pub fn is_ready() -> bool {
    k_nano::audio::hda::is_ready()
}
