use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::Receiver;
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::settings::AUDIO_VOLUME;
use crate::audio::voice::AUDIO_RING;
use core::sync::atomic::Ordering;

const MIXER_MANIFEST: AgentManifest = AgentManifest {
    name: "audio_mixer",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct AudioMixerAgent {
    tts_receiver: Receiver,
    out_ring: &'static AudioRingBuffer,
}

impl AudioMixerAgent {
    pub fn new() -> Self {
        AudioMixerAgent {
            tts_receiver: k_nano::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_OUT),
            out_ring: &AUDIO_RING,
        }
    }
}

impl Agent for AudioMixerAgent {
    fn manifest(&self) -> &AgentManifest { &MIXER_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(ev) = self.tts_receiver.try_receive() {
            let vol = AUDIO_VOLUME.load(Ordering::Relaxed) as f32 / 100.0;
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(
                    ev.payload.as_ptr() as *const i16,
                    ev.payload.len() / 2,
                )
            };
            let mut scaled: alloc::vec::Vec<i16> = alloc::vec::Vec::with_capacity(pcm.len());
            for &s in pcm {
                scaled.push((s as f32 * vol) as i16);
            }
            let written = self.out_ring.push(&scaled);
            k_nano::slog_jarbas!("MIXER", "info", "{} samples -> ring (vol={}%)", written, (vol * 100.0) as u8);
        }
        // Drena o ring buffer para o HDA playback (auto-falante)
        let mut hda_buf = [0i16; 1024];
        let n = self.out_ring.pop(&mut hda_buf);
        if n > 0 {
            crate::audio::hda::write_hda_playback(&hda_buf[..n]);
        }
        AgentTickResult::Pending
    }
}
