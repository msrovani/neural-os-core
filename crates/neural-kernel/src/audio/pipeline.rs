use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::vad::{VAD, VAD_ACTIVE};
use crate::audio::tts::{FrameProcessor, TTS_FRAME_SAMPLES, AudioFrame};
use crate::serial_println;
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
    llm_receiver: Receiver,
    tts_out: &'static AudioRingBuffer,
    active_frame: Option<FrameProcessor>,
    pending_text: alloc::vec::Vec<alloc::string::String>,
    vad: VAD,
    frame_counter: u64,
}

impl AudioPipelineAgent {
    pub fn new() -> Self {
        AudioPipelineAgent {
            llm_receiver: crate::EVENT_BUS.subscribe("LLM_RESPONSE"),
            tts_out: &crate::audio::voice::AUDIO_RING,
            active_frame: None,
            pending_text: alloc::vec::Vec::new(),
            vad: VAD::new(500.0, 16000),
            frame_counter: 0,
        }
    }
}

impl Agent for AudioPipelineAgent {
    fn manifest(&self) -> &AgentManifest { &PIPELINE_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        self.frame_counter += 1;

        if let Some(ev) = self.llm_receiver.try_receive() {
            if let Ok(text) = core::str::from_utf8(&ev.payload) {
                self.pending_text.push(alloc::string::String::from(text));
                serial_println!("[PIPELINE] LLM response enqueued for TTS");
            }
        }

        if BARGE_IN.load(Ordering::Relaxed) {
            if self.active_frame.is_some() {
                self.active_frame = None;
                serial_println!("[PIPELINE] Barge-in: TTS interrupted by user");
            }
            BARGE_IN.store(false, Ordering::Relaxed);
        }

        if self.active_frame.is_some() {
            if let Some(ref mut fp) = self.active_frame {
                let frame = fp.generate_frame();
                self.tts_out.push(&frame.pcm);
                if fp.is_done() {
                    self.active_frame = None;
                    serial_println!("[PIPELINE] TTS frame stream complete");
                }
            }
        } else if !self.pending_text.is_empty() {
            let text = self.pending_text.remove(0);
            let fp = FrameProcessor::new(&text);
            let total_frames = fp.estimated_frames();
            self.active_frame = Some(fp);
            serial_println!("[PIPELINE] Starting TTS stream: ~{} frames (12.5 Hz)", total_frames);
        }

        if self.frame_counter % 10 == 0 {
            let mic_samples = [0i16; 256];
            let (_, _, is_speech, transition) = self.vad.process_frame(&mic_samples);
            if transition == crate::audio::vad::VadTransition::SpeechStart {
                BARGE_IN.store(true, Ordering::Relaxed);
            }
        }

        AgentTickResult::Pending
    }
}
