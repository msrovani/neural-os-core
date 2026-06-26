use alloc::collections::VecDeque;
use core::task::{Context, Poll};

use super::agent::AgentTask;
use super::dummy_waker;
use crate::serial_println;
use crate::println;

pub struct NeuralExecutor {
    task_queue: VecDeque<AgentTask>,
    iteration: u64,
}

impl NeuralExecutor {
    pub fn new() -> Self {
        NeuralExecutor { task_queue: VecDeque::new(), iteration: 0 }
    }

    pub fn spawn(&mut self, task: AgentTask) {
        serial_println!("[EXECUTOR] Spawning AgentTask id={}", task.id);
        println!("[EXECUTOR] Spawning AgentTask id={}", task.id);
        self.task_queue.push_back(task);
    }

    pub fn run(&mut self) -> ! {
        serial_println!("[EXECUTOR] Started ({} tasks)", self.task_queue.len());
        println!("[EXECUTOR] Started ({} tasks)", self.task_queue.len());

        loop {
            // Check for respawn requests (from SelfHeal recovery)
            let respawns = { let q = crate::RESPAWN_QUEUE.lock(); q.clone() };
            for name in &respawns {
                crate::spawn_task_by_name(name, self);
            }
            if !respawns.is_empty() {
                crate::RESPAWN_QUEUE.lock().clear();
            }

            if let Some(mut task) = self.task_queue.pop_front() {
                let waker = dummy_waker();
                let mut context = Context::from_waker(&waker);

                match task.future.as_mut().poll(&mut context) {
                    Poll::Pending => { self.task_queue.push_back(task); }
                    Poll::Ready(()) => {
                        serial_println!("[EXECUTOR] Task id={} completed", task.id);
                        println!("[EXECUTOR] Task id={} completed", task.id);
                    }
                }
            }

            self.iteration += 1;
            if self.iteration % 100 == 0 {
                let ram = crate::memory::global_hardware_context();
                serial_println!("[EXECUTOR] RAM=[{:.6},{:.6}] tasks={}", ram[0], ram[1], self.task_queue.len());
            }
            let ticks = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            if ticks > 0 && ticks % 100 == 0 {
                serial_println!("[WATCHDOG] Ticks: {}", ticks);
            }
            x86_64::instructions::hlt();
        }
    }
}
