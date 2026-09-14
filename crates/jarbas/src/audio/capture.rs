//! AudioInputAgent — **dono único do microfone** (WS2 / SESSION_346).
//!
//! É o ÚNICO lugar que chama `poll_hda_audio`/`poll_uac_audio`. Faz o que antes não
//! existia em nenhum lugar:
//!
//!   1. **de-interleave** estéreo → mono (média L/R);
//!   2. **decimação 48 kHz → 16 kHz** com filtro box de `VOICE_DECIM` taps
//!      (o kernel lia 48 kHz estéreo e tratava como 16 kHz mono: pitch 3× errado e
//!      canais alternando dentro do mesmo sinal);
//!   3. **carry-over** entre eventos, publicando frames de tamanho FIXO.
//!
//! Antes, `voice.rs` e `wakeword.rs` faziam cada um `pcm.chunks(320)` com
//! `if chunk.len() < 320 { continue; }` sobre eventos de 512 amostras → 192 de cada
//! 512 amostras (37,5%) eram descartadas silenciosamente, sempre nas mesmas
//! fronteiras. Aqui a invariante é `OUT_SAMPLES * VOICE_DECIM == IN_SAMPLES`.
//!
//! Um único VAD vive aqui e publica `VAD_TRANSITION` ("start"/"end"), eliminando as
//! duas instâncias independentes (wakeword + voice) que processavam o mesmo stream.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};
use event_bus::{CapabilityToken, Event, Receiver};

use crate::audio::settings;
use crate::audio::vad::{VAD, VadTransition};

/// Frame canônico consumido por wakeword/sessão: 20 ms @ 16 kHz.
pub const FRAME_SAMPLES: usize = 320;
/// Taxa do pipeline de voz (após decimação).
pub const VOICE_RATE: u32 = k_nano::audio::hda::VOICE_RATE_HZ;
pub const VOICE_DECIM: usize = k_nano::audio::hda::VOICE_DECIM;

pub const TOPIC_AUDIO_FRAME: &str = "AUDIO_FRAME";
pub const TOPIC_VAD_TRANSITION: &str = "VAD_TRANSITION";

/// Contadores auditáveis. `OUT * VOICE_DECIM == IN` é a invariante do dono do mic.
pub static IN_SAMPLES: AtomicU64 = AtomicU64::new(0);
pub static OUT_SAMPLES: AtomicU64 = AtomicU64::new(0);
pub static FRAMES_PUBLISHED: AtomicU64 = AtomicU64::new(0);
/// Amostras perdidas por overflow do carry (deve ser 0).
pub static CARRY_OVERFLOW: AtomicU64 = AtomicU64::new(0);

/// Capacidade do carry: 8 frames (160 ms). Se estourar, é bug — contabilizado.
const CARRY_CAP: usize = FRAME_SAMPLES * 8;

/// Conversão de stream cru (48 kHz estéreo intercalado) → frames mono 16 kHz.
///
/// Separado do agente de propósito: é DSP puro, sem EventBus, e por isso a
/// invariante "nenhuma amostra perdida" é testável no host.
pub struct FrameAssembler {
    mono: [i16; CARRY_CAP],
    mono_len: usize,
    /// Fase do de-interleave (eventos ímpares não desalinham o par L/R).
    channel_phase: usize,
    /// Amostra L pendente quando o evento corta o par no meio.
    pending_l: i16,
    decim_sum: i32,
    decim_count: usize,
}

impl FrameAssembler {
    pub const fn new() -> Self {
        FrameAssembler {
            mono: [0i16; CARRY_CAP],
            mono_len: 0,
            channel_phase: 0,
            pending_l: 0,
            decim_sum: 0,
            decim_count: 0,
        }
    }

    /// Consome um evento cru. Retorna quantas amostras mono 16 kHz foram produzidas.
    pub fn ingest(&mut self, pcm: &[i16]) -> usize {
        IN_SAMPLES.fetch_add(pcm.len() as u64, Ordering::Relaxed);
        let chans = k_nano::audio::hda::CAPTURE_CHANNELS.max(1);
        let decim = VOICE_DECIM.max(1);
        let mut produced = 0usize;

        for &s in pcm {
            if chans == 1 {
                if self.decim_push(s as i32, decim) {
                    produced += 1;
                }
                continue;
            }
            if self.channel_phase == 0 {
                self.pending_l = s;
                self.channel_phase = 1;
            } else {
                let m = ((self.pending_l as i32) + (s as i32)) / 2;
                self.channel_phase = 0;
                if self.decim_push(m, decim) {
                    produced += 1;
                }
            }
        }
        OUT_SAMPLES.fetch_add(produced as u64, Ordering::Relaxed);
        produced
    }

    /// Acumula `decim` amostras e emite a média (filtro box = anti-alias).
    fn decim_push(&mut self, v: i32, decim: usize) -> bool {
        self.decim_sum += v;
        self.decim_count += 1;
        if self.decim_count < decim {
            return false;
        }
        let avg = (self.decim_sum / decim as i32).clamp(-32768, 32767) as i16;
        self.decim_sum = 0;
        self.decim_count = 0;
        if self.mono_len >= CARRY_CAP {
            CARRY_OVERFLOW.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.mono[self.mono_len] = avg;
        self.mono_len += 1;
        true
    }

    /// Retira o próximo frame completo, se houver.
    pub fn take_frame(&mut self) -> Option<[i16; FRAME_SAMPLES]> {
        if self.mono_len < FRAME_SAMPLES {
            return None;
        }
        let mut frame = [0i16; FRAME_SAMPLES];
        frame.copy_from_slice(&self.mono[..FRAME_SAMPLES]);
        self.mono.copy_within(FRAME_SAMPLES..self.mono_len, 0);
        self.mono_len -= FRAME_SAMPLES;
        Some(frame)
    }

    pub fn pending(&self) -> usize {
        self.mono_len
    }
}

impl Default for FrameAssembler {
    fn default() -> Self {
        Self::new()
    }
}

const CAPTURE_MANIFEST: AgentManifest = AgentManifest {
    name: "audio_input",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct AudioInputAgent {
    receiver: Receiver,
    vad: VAD,
    asm: FrameAssembler,
}

impl Default for AudioInputAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioInputAgent {
    pub fn new() -> Self {
        AudioInputAgent {
            receiver: k_nano::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_IN),
            vad: VAD::new(settings::vad_threshold(), VOICE_RATE),
            asm: FrameAssembler::new(),
        }
    }

    fn drain_frames(&mut self) {
        while let Some(frame) = self.asm.take_frame() {
            let bytes: alloc::vec::Vec<u8> =
                frame.iter().flat_map(|s| s.to_le_bytes()).collect();
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(TOPIC_AUDIO_FRAME),
                payload: bytes,
                token: CapabilityToken::Legacy(1),
            });
            FRAMES_PUBLISHED.fetch_add(1, Ordering::Relaxed);

            // VAD único do sistema: roda aqui, não em cada consumidor.
            let (energy, _zcr, _active, transition) = self.vad.process_frame(&frame);
            if transition != VadTransition::None {
                let tag = match transition {
                    VadTransition::SpeechStart => "start",
                    VadTransition::SpeechEnd => "end",
                    VadTransition::None => "none",
                };
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from(TOPIC_VAD_TRANSITION),
                    payload: alloc::format!("{} energy={:.0}", tag, energy).into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            }
        }
    }
}

impl Agent for AudioInputAgent {
    fn manifest(&self) -> &AgentManifest {
        &CAPTURE_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Dono único do mic: nenhum outro agente faz poll nem lê AUDIO_IN.
        k_hal::audio::hda::poll_hda_audio();
        crate::audio::usb::poll_uac_audio();

        while let Some(ev) = self.receiver.try_receive() {
            if ev.payload.len() < 2 {
                continue;
            }
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(
                    ev.payload.as_ptr() as *const i16,
                    ev.payload.len() / 2,
                )
            };
            // FFT do orb continua alimentada pelo sinal CRU (o orb reage a voz).
            crate::display::avatar::process_audio_fft(pcm);
            self.asm.ingest(pcm);
        }
        self.drain_frames();
        AgentTickResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A invariante do dono do mic: NENHUMA amostra é descartada, mesmo com eventos
    /// de tamanho irregular (o bug antigo perdia `len % 320` de cada evento).
    #[test]
    fn no_sample_is_lost_across_irregular_events() {
        let mut asm = FrameAssembler::new();
        let chans = k_nano::audio::hda::CAPTURE_CHANNELS.max(1);
        let decim = VOICE_DECIM.max(1);

        // Eventos de tamanhos irregulares, incluindo ímpares (quebram o par L/R).
        let sizes = [2048usize, 512, 37, 1, 999, 320, 4096, 13];
        let mut total_in = 0usize;
        let mut total_out = 0usize;
        let mut frames = 0usize;
        for (k, &n) in sizes.iter().enumerate() {
            let samples: alloc::vec::Vec<i16> =
                (0..n).map(|i| ((i + k) % 100) as i16).collect();
            total_in += n;
            total_out += asm.ingest(&samples);
            while asm.take_frame().is_some() {
                frames += 1;
            }
        }

        if chans == 1 {
            // Sem de-interleave: cada amostra entra, uma a cada `decim` sai.
            assert_eq!(total_out, total_in / decim, "mono: out deve ser in/decim");
        } else {
            // Estéreo: cada par L/R vira 1 amostra mono; a cada `decim` sai 1.
            let pairs = total_in / 2;
            assert_eq!(total_out, pairs / decim, "estéreo: out deve ser pares/decim");
        }
        // Nada sobrou além de menos de um frame (o resto fica no carry, não é perdido).
        assert!(asm.pending() < FRAME_SAMPLES);
        assert_eq!(total_out, frames * FRAME_SAMPLES + asm.pending());
    }

    /// Frames saem SEMPRE com o tamanho canônico (o consumidor não fatia mais).
    #[test]
    fn frames_are_exactly_canonical_size() {
        let mut asm = FrameAssembler::new();
        let chans = k_nano::audio::hda::CAPTURE_CHANNELS.max(1);
        let decim = VOICE_DECIM.max(1);
        // amostras suficientes para >= 3 frames de 16 kHz
        let need_mono = FRAME_SAMPLES * 3;
        let need_raw = need_mono * decim * chans;
        let samples: alloc::vec::Vec<i16> = (0..need_raw).map(|i| (i % 50) as i16).collect();
        asm.ingest(&samples);
        for _ in 0..3 {
            let f = asm.take_frame().expect("frame completo disponível");
            assert_eq!(f.len(), FRAME_SAMPLES);
        }
    }

    /// Carry-over: um evento cortado no meio do par L/R não desalinha o próximo.
    #[test]
    fn odd_event_split_does_not_desalign_channels() {
        let mut asm = FrameAssembler::new();
        if k_nano::audio::hda::CAPTURE_CHANNELS < 2 {
            return; // invariante só faz sentido em estéreo
        }
        let decim = VOICE_DECIM.max(1);
        // 3 pares + 1 amostra solta: o L=70 fica pendente para o próximo evento.
        let first = [10i16, 20, 30, 40, 50, 60, 70];
        let mut out = asm.ingest(&first);
        // 3 pares completos → 3 amostras mono → decim=3 → 1 amostra emitida.
        assert_eq!(out, 3 / decim);
        // O próximo evento começa com um R: precisa fechar o par (70,80), senão os
        // canais desalinham e TODO o resto do stream vira dados cruzados.
        let second = [80i16, 90, 100, 110, 120];
        out += asm.ingest(&second);
        // Pares totais: 3 (primeiro) + 3 [(70,80),(90,100),(110,120)] = 6 mono.
        // O acumulador de decimação NÃO reinicia entre eventos → 6/3 = 2 saídas.
        assert_eq!(out, 2, "par pendente fechado + contador de decimação contínuo");
    }
}
