use alloc::collections::VecDeque;
use core::task::{Context, Poll};

use super::agent::AgentTask;
use super::dummy_waker;
use crate::memory::BitmapFrameAllocator;
use crate::serial_println;
use crate::println;

pub struct NeuralExecutor<'a> {
    task_queue: VecDeque<AgentTask>,
    frame_allocator: &'a BitmapFrameAllocator,
    iteration: u64,
}

impl<'a> NeuralExecutor<'a> {
    pub fn new(frame_allocator: &'a BitmapFrameAllocator) -> Self {
        NeuralExecutor {
            task_queue: VecDeque::new(),
            frame_allocator,
            iteration: 0,
        }
    }

    pub fn spawn(&mut self, task: AgentTask) {
        serial_println!("[EXECUTOR] Spawning AgentTask id={}", task.id);
        println!("[EXECUTOR] Spawning AgentTask id={}", task.id);
        self.task_queue.push_back(task);
    }

    pub fn run(&mut self) -> ! {
        serial_println!("[EXECUTOR] Cooperative scheduler started ({} tasks queued)", self.task_queue.len());
        println!("[EXECUTOR] Cooperative scheduler started ({} tasks queued)", self.task_queue.len());

        loop {
            if let Some(mut task) = self.task_queue.pop_front() {
                let waker = dummy_waker();
                let mut context = Context::from_waker(&waker);

                match task.future.as_mut().poll(&mut context) {
                    Poll::Pending => {
                        self.task_queue.push_back(task);
                    }
                    Poll::Ready(()) => {
                        serial_println!("[EXECUTOR] AgentTask id={} completed", task.id);
                        println!("[EXECUTOR] AgentTask id={} completed", task.id);
                    }
                }
            }

            self.iteration += 1;

            if self.iteration % 100 == 0 {
                let ram = self.frame_allocator.hardware_context_tensor();
                serial_println!("[EXECUTOR] Hardware context: RAM=[{:.6}, {:.6}] tasks={}",
                    ram[0], ram[1], self.task_queue.len());
            }

            let ticks = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            if ticks > 0 && ticks % 100 == 0 {
                serial_println!("[WATCHDOG] Ticks do temporizador: {}", ticks);
            }

            x86_64::instructions::hlt();
        }
    }
}
