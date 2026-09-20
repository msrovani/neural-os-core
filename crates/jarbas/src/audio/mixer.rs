//! AudioMixerAgent — volume + drain PLAYBACK_RING → HDA/UAC speaker.
//!
//! SESSION_352/368: pop limitado por (1) free frames HDA, (2) Δt×16 kHz (TSC),
//! (3) teto ~20 ms/tick — nunca rajada 1024×tick_hz.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::Receiver;
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::settings::AUDIO_VOLUME;
use crate::audio::voice::PLAYBACK_RING;
use core::sync::atomic::{AtomicU64, Ordering};

const MIXER_MANIFEST: AgentManifest = AgentManifest {
    name: "audio_mixer",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// Taxa do pipeline de voz (mono) — Piper/formant → SD1 via VOICE_DECIM.
pub const VOICE_RATE_HZ: u64 = 16_000;

/// ~20 ms @16 kHz — cap por tick para não matar o scheduler.
const MAX_POP_PER_TICK: usize = 320;

static LAST_POP_US: AtomicU64 = AtomicU64::new(0);
static LAST_DROP_LOG: AtomicU64 = AtomicU64::new(0);

/// Quanto drenar neste tick (puro — testável no host).
///
/// `dt_us==0` → ignora orçamento de relógio (só free/ring/cap).
pub fn compute_mixer_want(
    hda_free: usize,
    ring_avail: usize,
    max_pop: usize,
    dt_us: u64,
    rate_hz: u64,
) -> usize {
    let mut want = hda_free.min(ring_avail).min(max_pop);
    if dt_us > 0 && rate_hz > 0 {
        // samples ≈ dt_us * rate / 1e6; +1 evita starvation em dt curto.
        let clock_budget = ((dt_us.saturating_mul(rate_hz)) / 1_000_000).saturating_add(1) as usize;
        want = want.min(clock_budget.max(1));
    }
    want
}

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

        let now = k_nano::tsc::now_us();
        let last = LAST_POP_US.load(Ordering::Relaxed);
        let dt = if last == 0 {
            0
        } else {
            now.saturating_sub(last).min(100_000) // cap 100 ms (stall)
        };

        let hda_free = k_hal::audio::hda::playback_free_mono_samples();
        let ring_avail = self.out_ring.available();
        let want = compute_mixer_want(hda_free, ring_avail, MAX_POP_PER_TICK, dt, VOICE_RATE_HZ);
        if want == 0 {
            return AgentTickResult::Pending;
        }
        let mut buf = [0i16; MAX_POP_PER_TICK];
        let n = self.out_ring.pop(&mut buf[..want]);
        if n > 0 {
            LAST_POP_US.store(now.max(1), Ordering::Relaxed);
            crate::display::avatar::process_audio_fft(&buf[..n]);
            k_hal::audio::hda::write_hda_playback(&buf[..n]);
            crate::audio::usb::write_uac_playback(&buf[..n]);
        }

        // Honesty: se o driver ainda dropou, log METRIC quando o contador sobe.
        let dropped = k_nano::audio::hda::PLAY_SAMPLES_DROPPED.load(Ordering::Relaxed);
        let prev = LAST_DROP_LOG.load(Ordering::Relaxed);
        if dropped > prev
            && LAST_DROP_LOG
                .compare_exchange(prev, dropped, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            k_nano::slog_jarbas!(
                "MIXER",
                "warn",
                "METRIC PLAY_SAMPLES_DROPPED={} (pacing free+TSC)",
                dropped
            );
        }
        AgentTickResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn want_limited_by_hda_free() {
        assert_eq!(compute_mixer_want(10, 1000, 320, 0, VOICE_RATE_HZ), 10);
    }

    #[test]
    fn want_limited_by_max_pop() {
        assert_eq!(compute_mixer_want(10_000, 10_000, 320, 0, VOICE_RATE_HZ), 320);
    }

    #[test]
    fn want_limited_by_clock_budget() {
        // 20 ms @16 kHz = 320 samples (+1) → min com max_pop 320 = 320
        let w = compute_mixer_want(10_000, 10_000, 320, 20_000, VOICE_RATE_HZ);
        assert!(w <= 321, "w={}", w);
        assert!(w >= 320, "w={}", w);
    }

    #[test]
    fn want_short_dt_still_at_least_one_when_free() {
        // 1 ms → 16+1=17
        assert_eq!(compute_mixer_want(100, 100, 320, 1_000, VOICE_RATE_HZ), 17);
    }

    #[test]
    fn want_zero_when_no_free() {
        assert_eq!(compute_mixer_want(0, 1000, 320, 20_000, VOICE_RATE_HZ), 0);
    }
}
