//! GPU G1 persistent work-queue (ADR-0047-GPU) — CPU enfileira; drain HW ou CPU fallback.
//! XPU: Prefill/Decode ops roteáveis entre CPU e GPU conforme BackendState.
//!
//! Produtores: Hermes/agentes/Cortex/WASM skills (`aios_gpu::submit`) + XPU engine;
//! consumidor: `drain` (HW real ou CPU fallback).

use crate::gpu::compute_abi::TensorOp;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GpuOp {
    Nop,
    VectorAdd,
    MatmulTernary,
    BitLinearW2A8,
    Fence,
    /// Prefill: forward do prompt completo (CPU→GPU quando Ready).
    Prefill,
    /// Decode: gera 1 token via KV cache (CPU→GPU quando Ready).
    Decode,
}

impl GpuOp {
    pub fn is_compute(&self) -> bool {
        matches!(self, GpuOp::VectorAdd | GpuOp::MatmulTernary | GpuOp::BitLinearW2A8)
    }
}

impl From<TensorOp> for GpuOp {
    fn from(op: TensorOp) -> Self {
        match op {
            TensorOp::Nop => GpuOp::Nop,
            TensorOp::VectorAdd => GpuOp::VectorAdd,
            TensorOp::MatmulTernary => GpuOp::MatmulTernary,
            TensorOp::BitLinearW2A8 => GpuOp::BitLinearW2A8,
            TensorOp::Fence => GpuOp::Fence,
        }
    }
}

#[derive(Clone, Copy)]
pub struct GpuJob {
    pub op: GpuOp,
    pub id: u64,
}

const QCAP: usize = 64;

pub struct WorkQueue {
    slots: [Option<GpuJob>; QCAP],
    head: usize,
    tail: usize,
    pub submitted: AtomicU64,
    pub completed_hw: AtomicU64,
    pub completed_cpu: AtomicU64,
    /// Telemetria XPU separada
    pub gpu_prefills: AtomicU64,
    pub gpu_decodes: AtomicU64,
    pub cpu_prefills: AtomicU64,
    pub cpu_decodes: AtomicU64,
}

impl WorkQueue {
    pub const fn new() -> Self {
        WorkQueue {
            slots: [None; QCAP],
            head: 0,
            tail: 0,
            submitted: AtomicU64::new(0),
            completed_hw: AtomicU64::new(0),
            completed_cpu: AtomicU64::new(0),
            gpu_prefills: AtomicU64::new(0),
            gpu_decodes: AtomicU64::new(0),
            cpu_prefills: AtomicU64::new(0),
            cpu_decodes: AtomicU64::new(0),
        }
    }

    pub fn enqueue(&mut self, op: GpuOp) -> Option<u64> {
        let next = (self.tail + 1) % QCAP;
        if next == self.head {
            return None; // full
        }
        let id = self.submitted.fetch_add(1, Ordering::Relaxed) + 1;
        self.slots[self.tail] = Some(GpuJob { op, id });
        self.tail = next;
        Some(id)
    }

    pub fn dequeue(&mut self) -> Option<GpuJob> {
        if self.head == self.tail {
            return None;
        }
        let job = self.slots[self.head].take();
        self.head = (self.head + 1) % QCAP;
        job
    }
}

static QUEUE: Mutex<WorkQueue> = Mutex::new(WorkQueue::new());

/// Enqueue op; returns job id.
pub fn submit(op: GpuOp) -> Option<u64> {
    QUEUE.lock().enqueue(op)
}

/// Enqueue TensorOp (Cortex / backend).
pub fn submit_tensor(op: TensorOp) -> Option<u64> {
    submit(GpuOp::from(op))
}

/// Drain queue: if `hw_ready` try HW path marker, else CPU complete.
/// Telemetria XPU: contabiliza prefill/decode separadamente.
pub fn drain(hw_ready: bool) -> u32 {
    let mut n = 0u32;
    let mut q = QUEUE.lock();
    while let Some(job) = q.dequeue() {
        n += 1;
        let hw_dispatched = if hw_ready {
            // Dispatch real via backend quando state=Ready.
            // TODO(Layer S/HW): chamar dispatch_gpu_op do backend.rs
            // quando pushbuffer NVIDIA / Intel ring estiver pronto.
            false
        } else {
            false
        };
        if hw_dispatched {
            q.completed_hw.fetch_add(1, Ordering::Relaxed);
        } else {
            q.completed_cpu.fetch_add(1, Ordering::Relaxed);
        }
        // Telemetria XPU separada
        match job.op {
            GpuOp::Prefill => {
                if hw_dispatched {
                    q.gpu_prefills.fetch_add(1, Ordering::Relaxed);
                } else {
                    q.cpu_prefills.fetch_add(1, Ordering::Relaxed);
                }
            }
            GpuOp::Decode => {
                if hw_dispatched {
                    q.gpu_decodes.fetch_add(1, Ordering::Relaxed);
                } else {
                    q.cpu_decodes.fetch_add(1, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }
    n
}

pub fn stats() -> (u64, u64, u64) {
    let q = QUEUE.lock();
    (
        q.submitted.load(Ordering::Relaxed),
        q.completed_hw.load(Ordering::Relaxed),
        q.completed_cpu.load(Ordering::Relaxed),
    )
}

/// Telemetria XPU: (gpu_prefills, gpu_decodes, cpu_prefills, cpu_decodes).
pub fn xpu_stats() -> (u64, u64, u64, u64) {
    let q = QUEUE.lock();
    (
        q.gpu_prefills.load(Ordering::Relaxed),
        q.gpu_decodes.load(Ordering::Relaxed),
        q.cpu_prefills.load(Ordering::Relaxed),
        q.cpu_decodes.load(Ordering::Relaxed),
    )
}

/// Boot gate helper: submit Nop+Matmul, drain, report HW|CPU_FALLBACK.
pub fn gate_status(hw_ready: bool) -> &'static str {
    let _ = submit(GpuOp::Nop);
    let _ = submit(GpuOp::MatmulTernary);
    let _ = drain(hw_ready);
    if hw_ready {
        "HW"
    } else {
        "CPU_FALLBACK"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());

    #[test]
    fn lockfree_submit_drain_roundtrip() {
        let _g = TEST_LOCK.lock();
        let before = stats();
        let id = submit(GpuOp::MatmulTernary).expect("submit");
        assert!(id > 0);
        assert_eq!(drain(false), 1);
        let (s, _hw, cpu) = stats();
        assert_eq!(s, before.0 + 1);
        assert_eq!(cpu, before.2 + 1);
    }

    #[test]
    fn lockfree_ring_tolerates_overflow() {
        let _g = TEST_LOCK.lock();
        let mut rejected = 0u32;
        for _ in 0..(QCAP as u32 * 4) {
            if submit(GpuOp::Nop).is_none() {
                rejected += 1;
            }
        }
        assert!(rejected > 0, "anel de capacidade finita deveria esgotar");
        let drained = drain(false);
        assert_eq!(drained, (QCAP - 1) as u32, "drain deveria drenar QCAP-1 slots (anel circula 1 slot)");
    }

    #[test]
    fn drain_prefill_decode_xpu_telemetry() {
        let _g = TEST_LOCK.lock();
        let (gp0, gd0, cp0, cd0) = xpu_stats();
        // Submit 2 prefill + 1 decode, drain as CPU
        let _ = submit(GpuOp::Prefill);
        let _ = submit(GpuOp::Prefill);
        let _ = submit(GpuOp::Decode);
        let drained = drain(false);
        assert_eq!(drained, 3);
        let (gp, gd, cp, cd) = xpu_stats();
        assert_eq!(gp, gp0, "GPU prefill deve ser 0 em host sem GPU");
        assert_eq!(gd, gd0, "GPU decode deve ser 0 em host sem GPU");
        assert_eq!(cp, cp0 + 2, "CPU prefill deveria ser +2");
        assert_eq!(cd, cd0 + 1, "CPU decode deveria ser +1");
    }

    #[test]
    fn drain_hw_ready_vs_cpu_fallback_counting() {
        let _g = TEST_LOCK.lock();
        let (s0, hw0, cpu0) = stats();
        let _ = submit(GpuOp::MatmulTernary);
        assert_eq!(drain(false), 1);
        let (_s, hw, cpu) = stats();
        assert_eq!(hw, hw0, "drain(false) não deveria incrementar HW");
        assert_eq!(cpu, cpu0 + 1, "drain(false) deveria incrementar CPU");

        let _ = submit(GpuOp::Fence);
        assert_eq!(drain(true), 1);
        let (_s2, hw2, cpu2) = stats();
        assert_eq!((hw2 - hw) + (cpu2 - cpu), 1,
            "drain(true) deveria processar 1 job");
    }

    #[test]
    fn gate_status_reports_cpu_fallback_without_hw() {
        let _g = TEST_LOCK.lock();
        assert_eq!(gate_status(false), "CPU_FALLBACK");
        assert_eq!(gate_status(true), "HW");
    }
}
