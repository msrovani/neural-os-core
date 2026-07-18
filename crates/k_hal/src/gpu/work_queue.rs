//! GPU G1 persistent work-queue (ADR-0047-GPU). CPU enfileira; drain HW ou CPU fallback.

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
pub fn drain(hw_ready: bool) -> u32 {
    let mut n = 0u32;
    let mut q = QUEUE.lock();
    while let Some(job) = q.dequeue() {
        n += 1;
        match job.op {
            GpuOp::Nop | GpuOp::Fence => {}
            GpuOp::VectorAdd | GpuOp::MatmulTernary | GpuOp::BitLinearW2A8 => {
                // Dispatch real via backend quando state=Ready; queue rastreia intent.
            }
        }
        if hw_ready {
            q.completed_hw.fetch_add(1, Ordering::Relaxed);
        } else {
            q.completed_cpu.fetch_add(1, Ordering::Relaxed);
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
