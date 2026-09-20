//! AudioMixerAgent — volume + drain PLAYBACK_RING → HDA/UAC speaker.
//!
//! SESSION_352: pop fixo 1024/tick @60–120 Hz >> 16 kHz do device → drop.
//! Agora: `min(ring, free_hda, ~20 ms @16 kHz)`.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::Receiver;
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::settings::AUDIO_VOLUME;
use crate::audio::voice::PLAYBACK_RING;
use core::sync::atomic::Ordering;

const MIXER_MANIFEST: AgentManifest = AgentManifest {
    name: "audio_mixer",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// ~20 ms @16 kHz — cap por tick para não matar o scheduler.
const MAX_POP_PER_TICK: usize = 320;

pub struct AudioMixerAgent {
    tts_receiver: Receiver,
    out_ring: &'static AudioRingBuffer,
}

impl AudioMixerAgent {
    pub fn new() -> Self {
        AudioMixerAgent {
            tts_receiver: k_nano::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_OUT),
            out_ring: &PLAYBACK_RING,
        }
    }
}

impl Agent for AudioMixerAgent {
    fn manifest(&self) -> &AgentManifest {
        &MIXER_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Cap eventos AUDIO_OUT por tick — payload Piper 100k aloca/copia e congela UI.
        let mut n_ev = 0u32;
        while n_ev < 4 {
            let Some(ev) = self.tts_receiver.try_receive() else { break; };
            n_ev += 1;
            let vol = AUDIO_VOLUME.load(Ordering::Relaxed) as f32 / 100.0;
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(
                    ev.payload.as_ptr() as *const i16,
                    ev.payload.len() / 2,
                )
            };
            let take = pcm.len().min(4096);
            if take == 0 {
                continue;
            }
            let mut scaled = [0i16; 4096];
            for i in 0..take {
                let v = (pcm[i] as f32 * vol).clamp(-32768.0, 32767.0) as i16;
                scaled[i] = v;
            }
            let written = self.out_ring.push(&scaled[..take]);
            k_nano::slog_bin!(
                "MIXER",
                "trace",
                "{} samples -> playback ring (vol={}%)",
                written,
                (vol * 100.0) as u8
            );
        }

        let hda_free = k_hal::audio::hda::playback_free_mono_samples();
        let ring_avail = self.out_ring.available();
        let want = hda_free
            .min(ring_avail)
            .min(MAX_POP_PER_TICK);
        if want == 0 {
            return AgentTickResult::Pending;
        }
        let mut buf = [0i16; MAX_POP_PER_TICK];
        let n = self.out_ring.pop(&mut buf[..want]);
        if n > 0 {
            crate::display::avatar::process_audio_fft(&buf[..n]);
            k_hal::audio::hda::write_hda_playback(&buf[..n]);
            crate::audio::usb::write_uac_playback(&buf[..n]);
        }
        AgentTickResult::Pending
    }
}
