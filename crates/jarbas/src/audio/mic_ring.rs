//! MicFrameRing — SPSC de frames de voz (IDEA #562 / SESSION_383).
//!
//! Produzido só por `AudioInputAgent`; consumido por Voice + WakeWord.
//! Substitui `AUDIO_FRAME` no EventBus (clone/alloc por assinante).

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::audio::capture::FRAME_SAMPLES;

/// Slots por consumidor (potência de 2). ~640 ms @ 20 ms/frame.
const CAP: usize = 32;
const MASK: usize = CAP - 1;

/// Contador global de frames descartados (anel cheio).
pub static MIC_OVERRUN: AtomicU64 = AtomicU64::new(0);

/// Anel SPSC estático: 1 produtor (capture), 1 consumidor.
pub struct MicFrameRing {
    slots: UnsafeCell<[[i16; FRAME_SAMPLES]; CAP]>,
    write: AtomicUsize,
    read: AtomicUsize,
}

// SAFETY: SPSC — um writer (AudioInputAgent BSP) e um reader por anel.
unsafe impl Sync for MicFrameRing {}

impl MicFrameRing {
    pub const fn new() -> Self {
        MicFrameRing {
            slots: UnsafeCell::new([[0i16; FRAME_SAMPLES]; CAP]),
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    /// Push; se cheio, descarta o frame mais antigo (drop_oldest) e conta overrun.
    pub fn push(&self, frame: &[i16; FRAME_SAMPLES]) {
        let w = self.write.load(Ordering::Relaxed);
        let r = self.read.load(Ordering::Acquire);
        let filled = w.wrapping_sub(r);
        if filled >= CAP {
            // Drop oldest: avança read.
            self.read.store(r.wrapping_add(1), Ordering::Release);
            MIC_OVERRUN.fetch_add(1, Ordering::Relaxed);
        }
        let slot = w & MASK;
        unsafe {
            (*self.slots.get())[slot].copy_from_slice(frame);
        }
        self.write.store(w.wrapping_add(1), Ordering::Release);
    }

    pub fn try_pop(&self) -> Option<[i16; FRAME_SAMPLES]> {
        let r = self.read.load(Ordering::Relaxed);
        let w = self.write.load(Ordering::Acquire);
        if r == w {
            return None;
        }
        let slot = r & MASK;
        let mut frame = [0i16; FRAME_SAMPLES];
        unsafe {
            frame.copy_from_slice(&(*self.slots.get())[slot]);
        }
        self.read.store(r.wrapping_add(1), Ordering::Release);
        Some(frame)
    }

    pub fn len(&self) -> usize {
        self.write
            .load(Ordering::Relaxed)
            .wrapping_sub(self.read.load(Ordering::Relaxed))
            .min(CAP)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Fan-out: capture escreve nos dois; cada consumidor drena o seu.
pub static VOICE_MIC_RING: MicFrameRing = MicFrameRing::new();
pub static WAKE_MIC_RING: MicFrameRing = MicFrameRing::new();

/// Producer: empurra o mesmo frame para Voice + WakeWord.
#[inline]
pub fn push_frame(frame: &[i16; FRAME_SAMPLES]) {
    VOICE_MIC_RING.push(frame);
    WAKE_MIC_RING.push(frame);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_roundtrip() {
        let ring = MicFrameRing::new();
        let mut f = [0i16; FRAME_SAMPLES];
        f[0] = 42;
        f[319] = -7;
        ring.push(&f);
        let out = ring.try_pop().expect("frame");
        assert_eq!(out[0], 42);
        assert_eq!(out[319], -7);
        assert!(ring.try_pop().is_none());
    }

    #[test]
    fn overrun_drops_oldest() {
        let ring = MicFrameRing::new();
        let before = MIC_OVERRUN.load(Ordering::Relaxed);
        for i in 0..(CAP + 3) {
            let mut f = [0i16; FRAME_SAMPLES];
            f[0] = i as i16;
            ring.push(&f);
        }
        assert!(MIC_OVERRUN.load(Ordering::Relaxed) >= before + 3);
        let first = ring.try_pop().unwrap();
        // Mais antigo sobrevivente = CAP+3-CAP = 3 (0,1,2 dropados)
        assert_eq!(first[0], 3);
    }
}
