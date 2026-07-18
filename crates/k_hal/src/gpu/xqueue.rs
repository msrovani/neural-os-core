//! XQueue — fila de comandos GPU preemptível com 3 níveis.
//! pending: não submetido, in-flight: submetido mas não executando, running: em execução.
//! Referência: XSched (OSDI 2025).

use alloc::collections::VecDeque;
use crate::gpu::ring::{GpuJobRing, GpuJob};
use core::sync::atomic::Ordering;
use k_nano::interrupts::TIMER_TICKS;

/// Nível de preempção
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XqLevel { Low, Medium, High }

/// Estado do job na fila
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XqState { Pending, InFlight, Running, Done, Failed }

/// Descritor de job XQueue
#[derive(Debug, Clone)]
pub struct XqJob {
    pub id: u64,
    pub cmd: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
    pub level: XqLevel,
    pub state: XqState,
    pub submitted_tick: u64,
}

static NEXT_XQ_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// XQueue — fila preemptível sobre SPSC job ring
pub struct XQueue {
    pending: VecDeque<XqJob>,
    in_flight: VecDeque<XqJob>,
    running: Option<XqJob>,
    ring: *mut GpuJobRing,  // raw ptr para evitar borrow checker
    max_pending: usize,
    completed: u64,
    failed: u64,
}

unsafe impl Send for XQueue {}

impl XQueue {
    pub fn new(ring: &mut GpuJobRing) -> Self {
        XQueue {
            pending: VecDeque::new(),
            in_flight: VecDeque::new(),
            running: None,
            ring: ring as *mut GpuJobRing,
            max_pending: 64,
            completed: 0,
            failed: 0,
        }
    }

    /// Enfileira job (pending)
    pub fn enqueue(&mut self, cmd: u32, a0: u32, a1: u32, a2: u32, level: XqLevel) -> u64 {
        let id = NEXT_XQ_ID.fetch_add(1, Ordering::Relaxed);
        let tick = TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.pending.push_back(XqJob {
            id, cmd, arg0: a0, arg1: a1, arg2: a2,
            level, state: XqState::Pending, submitted_tick: tick,
        });
        id
    }

    /// Tick do scheduler: promove pending → in-flight conforme nível
    pub fn tick(&mut self, now_tick: u64) {
        let ring = unsafe { &mut *self.ring };

        // 1. Verifica se o running completou (take + match evita borrow conflict)
        let completed = match self.running.take() {
            Some(r) => {
                if ring.poll_head(1) {
                    self.completed += 1;
                    true
                } else if now_tick.wrapping_sub(r.submitted_tick) > 500 {
                    self.failed += 1;
                    k_nano::slog_hal!("XQUEUE", "info", "Job {} timeout", r.id);
                    true
                } else {
                    self.running = Some(r); // put back
                    false
                }
            }
            None => false,
        };
        // Se running completou ou deu timeout, pega próximo da in_flight
        if completed {
            if let Some(job) = self.in_flight.pop_front() {
                let gj = GpuJob { cmd: job.cmd, arg0: job.arg0, arg1: job.arg1, arg2: job.arg2 };
                if unsafe { ring.submit_and_wait(&gj, 100) } {
                    self.running = Some(XqJob { submitted_tick: now_tick, ..job });
                } else {
                    self.failed += 1;
                    k_nano::slog_hal!("XQUEUE", "info", "Job {} submit failed", job.id);
                }
            }
        }

        // 2. Promove pending → in-flight
        while self.in_flight.len() < self.max_pending {
            if let Some(job) = self.pending.pop_front() {
                self.in_flight.push_back(job);
            } else { break; }
        }
    }

    /// Preempt: rebaixa in-flight de volta para pending
    pub fn preempt(&mut self) {
        while let Some(job) = self.in_flight.pop_front() {
            self.pending.push_front(job);
        }
    }

    /// Status
    pub fn status(&self) -> alloc::string::String {
        alloc::format!("XQueue: {} pending, {} in-flight, {} running, {} done, {} failed",
            self.pending.len(), self.in_flight.len(),
            if self.running.is_some() { 1 } else { 0 },
            self.completed, self.failed)
    }
}
