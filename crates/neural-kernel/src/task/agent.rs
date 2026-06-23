use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

pub struct AgentTask {
    pub id: u64,
    pub future: Pin<Box<dyn Future<Output = ()>>>,
}

impl AgentTask {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Self {
        let id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
        AgentTask {
            id,
            future: Box::pin(future),
        }
    }
}
