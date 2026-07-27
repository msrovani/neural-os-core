//! Lock-free telemetry ring buffer.
//!
//! SPSC (Single Producer, Single Consumer) ring buffer for kernel telemetry events.
//! Producer: interrupt handlers, syscall handlers (writes to `head`).
//! Consumer: shell, diagnostic commands (reads from `tail`).
//! Never blocks the producer — silently drops the newest event when the ring is full.
//!
//! Inspired by Folkering OS's kernel-resident telemetry ring (ADR-0076 Onda 2.4-2.5).

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};

// ─── Event type constants ─────────────────────────────────────────────────────

/// Scheduler tick event.
pub const EV_SCHED: u8 = 0;
/// Agent started.
pub const EV_AGENT_START: u8 = 1;
/// Agent stopped.
pub const EV_AGENT_STOP: u8 = 2;
/// WASM host function call.
pub const EV_WASM_CALL: u8 = 3;
/// Capability denied.
pub const EV_CAP_DENY: u8 = 4;
/// Capability allowed.
pub const EV_CAP_ALLOW: u8 = 5;
/// Network packet sent.
pub const EV_NET_TX: u8 = 6;
/// Network packet received.
pub const EV_NET_RX: u8 = 7;
/// Health check result.
pub const EV_HEALTH: u8 = 8;
/// Kernel error.
pub const EV_ERROR: u8 = 9;
/// DMA operation.
pub const EV_DMA: u8 = 10;
/// User-defined event.
pub const EV_USER: u8 = 11;

// ─── Constants ────────────────────────────────────────────────────────────────

/// Ring buffer capacity (4096 slots, power of 2).
const CAPACITY: usize = 4096;
/// Bitmask for wrapping index into [0, 4095].
const MASK: usize = 4095;

// ─── TelemetryEvent ───────────────────────────────────────────────────────────

/// A single telemetry event (48 bytes, repr(C)).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TelemetryEvent {
    /// System tick counter at the moment the event was pushed.
    pub tick: u64,
    /// Event type discriminator (one of the `EV_*` constants).
    pub event_type: u8,
    /// Padding for 2-byte alignment of `agent_id`.
    pub _pad: [u8; 3],
    /// Agent or subsystem identifier that generated the event.
    pub agent_id: u16,
    /// Flexible payload carried by the event (32 bytes).
    pub data: [u8; 32],
}

// ─── TelemetryRing ────────────────────────────────────────────────────────────

/// Lock-free single-producer single-consumer telemetry ring buffer.
///
/// # SPSC contract
/// - **`head`** — written **only** by the producer (interrupt / syscall context).
/// - **`tail`** — written **only** by the consumer (shell / diagnostic context).
/// - There must never be concurrent producers or concurrent consumers.
/// - The `UnsafeCell` buffer is safe because the producer and consumer
///   never access the same slot simultaneously (`head` and `tail` define
///   disjoint regions of the live ring).
///
/// # Ordering
/// | Operation | Atomic | Ordering |
/// |-----------|--------|----------|
/// | push: load head | `Relaxed` | local cursor |
/// | push: load tail | `Acquire` | see freed slots from consumer |
/// | push: store head | `Release` | make written data visible to consumer |
/// | drain: load tail | `Relaxed` | local cursor |
/// | drain: load head | `Acquire` | see written data from producer |
/// | drain: store tail | `Release` | make freed slots visible to producer |
pub struct TelemetryRing {
    /// Ring buffer storage (power-of-2 size, 4096 slots).
    buffer: UnsafeCell<[MaybeUninit<TelemetryEvent>; CAPACITY]>,
    /// Producer write index (only the producer modifies this).
    head: AtomicUsize,
    /// Consumer read index (only the consumer modifies this).
    tail: AtomicUsize,
}

// SAFETY: SPSC — no concurrent access to the same slot when used correctly.
unsafe impl Send for TelemetryRing {}
// SAFETY: SPSC — &self methods use internal synchronisation via atomics.
unsafe impl Sync for TelemetryRing {}

impl TelemetryRing {
    /// Creates a new (empty) telemetry ring buffer.
    ///
    /// All slots are `MaybeUninit::uninit()` — no initialisation cost at
    /// construction time.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: UnsafeCell::new([const { MaybeUninit::uninit() }; CAPACITY]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push a telemetry event into the ring.
    ///
    /// Called from **interrupt or syscall context** (producer side).
    /// The `tick` field is automatically filled from the global
    /// [`TIMER_TICKS`](crate::interrupts::TIMER_TICKS) counter.
    ///
    /// **Never blocks or allocates.**
    /// If the ring is full the event is silently dropped (newest-disappear
    /// policy) so the producer is never stalled.
    pub fn push(&self, event_type: u8, agent_id: u16, data: &[u8]) {
        let h = self.head.load(Ordering::Relaxed);
        // Acquire: synchronise with the consumer's tail store so we see
        // slots that have been consumed and freed.
        let t = self.tail.load(Ordering::Acquire);

        // Ring full → drop the newest event (never block the producer).
        if h.wrapping_sub(t) >= CAPACITY {
            return;
        }

        let idx = h & MASK;

        // SAFETY:
        // - Only the producer writes to `buffer[idx]`; the consumer reads
        //   this slot only *after* the producer has advanced `head` past
        //   `idx` and *before* its own `tail` passes `idx`.
        // - `h` and `idx` come from a single atomic load, so no other
        //   thread writes this slot concurrently.
        unsafe {
            let events = &mut *self.buffer.get();
            let slot: *mut TelemetryEvent = events[idx].as_mut_ptr();

            (*slot).tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
            (*slot).event_type = event_type;
            (*slot)._pad = [0; 3];
            (*slot).agent_id = agent_id;

            // Copy up to 32 bytes of payload; zero the remainder.
            let copy_len = data.len().min(32);
            let dest = &mut (*slot).data;
            dest[..copy_len].copy_from_slice(&data[..copy_len]);
            if copy_len < 32 {
                dest[copy_len..].fill(0);
            }
        }

        // Release: make the event data visible to the consumer.
        self.head.store(h + 1, Ordering::Release);
    }

    /// Drain up to `max` events from the ring into `out`.
    ///
    /// Called from **shell or diagnostic context** (consumer side).
    /// Events are copied out via `core::ptr::read` (the ring slot is
    /// logically consumed even though `MaybeUninit` retains the bytes).
    ///
    /// Returns the number of events actually drained.
    pub fn drain(&self, out: &mut alloc::vec::Vec<TelemetryEvent>, max: usize) -> usize {
        let t = self.tail.load(Ordering::Relaxed);
        // Acquire: synchronise with the producer's head store so we see
        // all events that have been committed.
        let h = self.head.load(Ordering::Acquire);

        let avail = h.wrapping_sub(t);
        if avail == 0 || max == 0 {
            return 0;
        }

        let count = avail.min(max);
        let new_tail = t + count;

        // Pre-allocate a single batch to minimise reallocations.
        out.reserve(count);

        for i in 0..count {
            let idx = (t + i) & MASK;

            // SAFETY:
            // - Only the consumer reads `buffer[idx]`; the producer has
            //   already finished writing this slot and advanced `head`
            //   past it.
            // - The slot will not be re-written by the producer until
            //   `tail` advances past `idx` (which happens after this loop).
            unsafe {
                let events = &*self.buffer.get();
                let event = events[idx].as_ptr().read();
                out.push(event);
            }
        }

        // Release: make the consumed slots visible to the producer.
        self.tail.store(new_tail, Ordering::Release);
        count
    }

    /// Returns `true` when the ring contains no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let h = self.head.load(Ordering::Relaxed);
        let t = self.tail.load(Ordering::Relaxed);
        h == t
    }

    /// Returns the number of events currently available to drain.
    #[must_use]
    pub fn len(&self) -> usize {
        let h = self.head.load(Ordering::Relaxed);
        let t = self.tail.load(Ordering::Relaxed);
        h.wrapping_sub(t)
    }

    /// Returns the total capacity (always [`CAPACITY`] = 4096).
    #[must_use]
    pub const fn capacity(&self) -> usize {
        CAPACITY
    }

    /// Discards all events currently in the ring.
    ///
    /// Safe to call from either producer or consumer context.
    /// Simply sets `tail = head`, making the ring appear empty.
    pub fn clear(&self) {
        let h = self.head.load(Ordering::Relaxed);
        // Release: make the empty state visible to the producer.
        self.tail.store(h, Ordering::Release);
    }
}

// ─── Global static ────────────────────────────────────────────────────────────

use lazy_static::lazy_static;

lazy_static! {
    /// Global telemetry ring buffer, accessible from anywhere in the kernel.
    ///
    /// # Usage (producer side — interrupt/syscall)
    /// ```ignore
    /// TELEMETRY.push(EV_SCHED, 0, &[]);
    /// ```
    ///
    /// # Usage (consumer side — shell/diagnostics)
    /// ```ignore
    /// let mut buf = alloc::vec::Vec::new();
    /// let n = TELEMETRY.drain(&mut buf, 64);
    /// ```
    pub static ref TELEMETRY: TelemetryRing = TelemetryRing::new();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Basic push + drain round-trip.
    #[test]
    fn test_push_drain() {
        let ring = TelemetryRing::new();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.capacity(), 4096);

        ring.push(EV_SCHED, 1, &[0xAA]);
        assert!(!ring.is_empty());
        assert_eq!(ring.len(), 1);

        let mut out = Vec::new();
        let n = ring.drain(&mut out, 10);
        assert_eq!(n, 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].event_type, EV_SCHED);
        assert_eq!(out[0].agent_id, 1);
        assert_eq!(out[0].data[0], 0xAA);
        assert!(ring.is_empty());
    }

    /// Multiple pushes and bulk drain.
    #[test]
    fn test_multi_push_drain() {
        let ring = TelemetryRing::new();
        let payload = [0x10, 0x20, 0x30];

        ring.push(EV_AGENT_START, 2, &payload);
        ring.push(EV_NET_TX, 3, &[]);
        ring.push(EV_ERROR, 4, &[0xFF; 32]);

        assert_eq!(ring.len(), 3);

        let mut out = Vec::new();
        let n = ring.drain(&mut out, 100);
        assert_eq!(n, 3);
        assert_eq!(out[0].event_type, EV_AGENT_START);
        assert_eq!(out[0].data[..3], payload);
        assert_eq!(out[1].event_type, EV_NET_TX);
        assert_eq!(out[2].event_type, EV_ERROR);
        assert_eq!(out[2].data, [0xFF; 32]);
    }

    /// Drain with smaller max than available.
    #[test]
    fn test_drain_partial() {
        let ring = TelemetryRing::new();
        for i in 0..10u8 {
            ring.push(EV_USER, i as u16, &[i]);
        }

        let mut out = Vec::new();
        let n = ring.drain(&mut out, 3);
        assert_eq!(n, 3);
        assert_eq!(out.len(), 3);
        assert_eq!(ring.len(), 7); // 7 remaining

        let mut out2 = Vec::new();
        let n2 = ring.drain(&mut out2, 10);
        assert_eq!(n2, 7);
        assert_eq!(out2.len(), 7);
        assert!(ring.is_empty());
    }

    /// Silent drop when full (drops newest).
    #[test]
    fn test_full_drop_newest() {
        let ring = TelemetryRing::new();
        // Fill the ring completely
        for i in 0..4096u16 {
            ring.push(EV_USER, i, &[]);
        }
        assert_eq!(ring.len(), 4096);

        // One more push should be silently dropped
        ring.push(EV_USER, 9999, &[]);
        // Length stays at capacity
        assert_eq!(ring.len(), 4096);

        // Verify the last event is NOT the one we tried to push
        // and the first event is still index 0
        let mut out = Vec::new();
        ring.drain(&mut out, 4096);
        assert_eq!(out.len(), 4096);
        assert_eq!(out[0].agent_id, 0);
        // The 4097th event (agent_id=9999) should NOT be present
        let has_dropped = out.iter().any(|e| e.agent_id == 9999);
        assert!(!has_dropped, "dropped event should not appear");
    }

    /// Clear resets the ring.
    #[test]
    fn test_clear() {
        let ring = TelemetryRing::new();
        ring.push(EV_DMA, 7, &[]);
        ring.push(EV_DMA, 8, &[]);
        assert_eq!(ring.len(), 2);

        ring.clear();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);

        // After clear, new events still work
        ring.push(EV_HEALTH, 9, &[0x42]);
        assert_eq!(ring.len(), 1);
        let mut out = Vec::new();
        let n = ring.drain(&mut out, 10);
        assert_eq!(n, 1);
        assert_eq!(out[0].agent_id, 9);
    }

    /// Wrapping behaviour at capacity boundary.
    #[test]
    fn test_wrap_around() {
        let ring = TelemetryRing::new();

        // Write around 4100 events (wrapping past 4096)
        for i in 0..4100u16 {
            ring.push(EV_USER, i, &[]);
        }

        // Ring should hold at most 4096 events
        assert_eq!(ring.len(), 4096);

        let mut out = Vec::new();
        let n = ring.drain(&mut out, 5000);
        assert_eq!(n, 4096);

        // The oldest 4 events were dropped (0..3), first should be 4
        assert_eq!(out[0].agent_id, 4);
        assert_eq!(out[4095].agent_id, 4099);
    }

    /// Drain with zero max returns zero.
    #[test]
    fn test_drain_zero_max() {
        let ring = TelemetryRing::new();
        ring.push(EV_SCHED, 1, &[]);
        let mut out = Vec::new();
        let n = ring.drain(&mut out, 0);
        assert_eq!(n, 0);
        assert_eq!(ring.len(), 1); // event still there
    }

    /// Payload is zero-padded when the input slice is shorter than 32 bytes.
    #[test]
    fn test_payload_zero_pad() {
        let ring = TelemetryRing::new();
        ring.push(EV_DMA, 5, &[0xAB; 12]);

        let mut out = Vec::new();
        ring.drain(&mut out, 1);
        assert_eq!(out[0].data[..12], [0xAB; 12]);
        assert_eq!(out[0].data[12..], [0; 20]);
    }

    /// Payload is truncated when the input slice exceeds 32 bytes.
    #[test]
    fn test_payload_truncate() {
        let ring = TelemetryRing::new();
        let long: [u8; 40] = [0xCD; 40];
        ring.push(EV_DMA, 6, &long);

        let mut out = Vec::new();
        ring.drain(&mut out, 1);
        assert_eq!(out[0].data.len(), 32);
        assert_eq!(out[0].data, [0xCD; 32]);
    }
}
