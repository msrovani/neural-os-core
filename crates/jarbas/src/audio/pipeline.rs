//! AudioPipelineAgent — barge-in a partir do MIC_CAPTURE_RING.
//! TTS é responsabilidade de JarbasVoiceAgent (HERMES_RESPONSE → synthesize_tts).
//! Sprint Sound: sem rota LLM_RESPONSE duplicada / formant paralelo.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::audio::vad::{VAD, VadTransition};
use core::sync::atomic::{AtomicBool, Ordering};

pub static BARGE_IN: AtomicBool = AtomicBool::new(false);

const PIPELINE_MANIFEST: AgentManifest = AgentManifest {
    name: "audio_pipeline",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct AudioPipelineAgent {
    vad: VAD,
    frame_counter: u64,
}

impl AudioPipelineAgent {
    pub fn new() -> Self {
        AudioPipelineAgent {
            vad: VAD::new(500.0, 16000),
            frame_counter: 0,
        }
    }
}

impl Agent for AudioPipelineAgent {
    fn manifest(&self) -> &AgentManifest {
        &PIPELINE_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        self.frame_counter += 1;

        if BARGE_IN.load(Ordering::Relaxed) {
            // Limpa playback para interromper TTS em curso.
            crate::audio::voice::PLAYBACK_RING.clear();
            k_nano::slog_bin!("PIPELINE", "info", "Barge-in: playback limpo — voltando a escutar");
            BARGE_IN.store(false, Ordering::Relaxed);
            // Reativa wake window para escutar imediatamente o próximo comando
            crate::audio::settings::force_wake_open();
        }

        if self.frame_counter % 10 == 0 {
            let mut mic_samples = [0i16; 256];
            let read = crate::audio::voice::MIC_CAPTURE_RING.pop(&mut mic_samples);
            if read > 0 {
                let (_, _, _is_speech, transition) = self.vad.process_frame(&mic_samples[..read]);
                if transition == VadTransition::SpeechStart {
                    // Só barge-in se houver playback ativo.
                    if crate::audio::voice::PLAYBACK_RING.available() > 64 {
                        BARGE_IN.store(true, Ordering::Relaxed);
                    }
                }
            }
        }

        AgentTickResult::Pending
    }
}
