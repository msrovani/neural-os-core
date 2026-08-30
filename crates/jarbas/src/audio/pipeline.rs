//! AudioPipelineAgent — barge-in a partir do MIC_CAPTURE_RING.
//! TTS é responsabilidade de JarbasVoiceAgent (HERMES_RESPONSE → synthesize_tts).
//! Sprint Sound: sem rota LLM_RESPONSE duplicada / formant paralelo.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
// VAD compartilhado via voice agent — não duplicar
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
    frame_counter: u64,
}

impl AudioPipelineAgent {
    pub fn new() -> Self {
        AudioPipelineAgent {
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
            // Barge-in: detecta se há playback ativo e usuário começou a falar
            // (VAD real roda no voice agent — aqui só checamos o ring)
            if crate::audio::voice::PLAYBACK_RING.available() > 64 {
                // Se há áudio no mic (não só silêncio), ativa barge-in
                let mut mic_samples = [0i16; 256];
                let read = crate::audio::voice::MIC_CAPTURE_RING.pop(&mut mic_samples);
                if read > 0 {
                    let has_voice = mic_samples[..read].iter().any(|s| s.unsigned_abs() > 500);
                    if has_voice {
                        BARGE_IN.store(true, Ordering::Relaxed);
                    }
                }
            }
        }

        AgentTickResult::Pending
    }
}
