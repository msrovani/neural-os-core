use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const RING_BUF_SAMPLES: usize = 16384;

pub struct AudioRingBuffer {
    buf: UnsafeCell<[i16; RING_BUF_SAMPLES]>,
    write: AtomicUsize,
    read: AtomicUsize,
}

unsafe impl Sync for AudioRingBuffer {}

impl AudioRingBuffer {
    pub const fn new() -> Self {
        AudioRingBuffer {
            buf: UnsafeCell::new([0i16; RING_BUF_SAMPLES]),
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, samples: &[i16]) -> usize {
        let w = self.write.load(Ordering::Relaxed);
        let r = self.read.load(Ordering::Acquire);
        let filled = w.wrapping_sub(r) % RING_BUF_SAMPLES;
        let space = RING_BUF_SAMPLES - 1 - filled;
        let n = samples.len().min(space);
        let buf = unsafe { &mut *self.buf.get() };
        for i in 0..n {
            buf[(w + i) % RING_BUF_SAMPLES] = samples[i];
        }
        core::sync::atomic::fence(Ordering::Release);
        self.write.store((w + n) % RING_BUF_SAMPLES, Ordering::Release);
        n
    }

    pub fn pop(&self, out: &mut [i16]) -> usize {
        let r = self.read.load(Ordering::Relaxed);
        let w = self.write.load(Ordering::Acquire);
        let filled = w.wrapping_sub(r) % RING_BUF_SAMPLES;
        let n = out.len().min(filled);
        let buf = unsafe { &mut *self.buf.get() };
        for i in 0..n {
            out[i] = buf[(r + i) % RING_BUF_SAMPLES];
        }
        core::sync::atomic::fence(Ordering::Release);
        self.read.store((r + n) % RING_BUF_SAMPLES, Ordering::Release);
        n
    }

    pub fn available(&self) -> usize {
        let w = self.write.load(Ordering::Acquire);
        let r = self.read.load(Ordering::Acquire);
        w.wrapping_sub(r) % RING_BUF_SAMPLES
    }

    pub fn clear(&self) {
        self.write.store(0, Ordering::Release);
        self.read.store(0, Ordering::Release);
    }
}
