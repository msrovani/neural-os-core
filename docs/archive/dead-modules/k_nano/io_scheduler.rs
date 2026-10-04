//! I/O Scheduler com Deadline + write coalescing.
use alloc::collections::VecDeque;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IoPriority {
    Critical = 0, Read = 1, Write = 2, Idle = 3,
}

#[derive(Debug, Clone)]
pub struct IoRequest {
    pub ctrl_idx: u8, pub disk_idx: u8, pub lba: u64, pub data: Vec<u8>,
    pub priority: IoPriority, pub deadline_tick: u64, pub agent_id: u16,
}

pub struct DeadlineQueue {
    queues: [VecDeque<IoRequest>; 4],
    batch_write: Vec<IoRequest>,
    max_batch: usize,
    tick: u64,
    stats_read: u64, stats_write: u64, stats_merged: u64,
}

impl DeadlineQueue {
    pub fn new() -> Self {
        DeadlineQueue {
            queues: [VecDeque::new(), VecDeque::new(), VecDeque::new(), VecDeque::new()],
            batch_write: Vec::new(), max_batch: 16,
            tick: 0, stats_read: 0, stats_write: 0, stats_merged: 0,
        }
    }

    pub fn push(&mut self, req: IoRequest) {
        let prio = req.priority as usize;
        self.queues[prio].push_back(req);
    }

    fn try_merge(&mut self, req: &IoRequest) -> bool {
        for q in &mut self.queues {
            for existing in q.iter_mut() {
                if existing.ctrl_idx == req.ctrl_idx && existing.disk_idx == req.disk_idx
                    && existing.lba + existing.data.len() as u64 / 512 == req.lba
                    && existing.priority as u8 == req.priority as u8
                {
                    existing.data.extend_from_slice(&req.data);
                    self.stats_merged += 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn pop(&mut self) -> Option<IoRequest> {
        if let Some(req) = self.queues[0].pop_front() { return Some(req); }
        if let Some(req) = self.queues[1].pop_front() { return Some(req); }
        // Retorna proximo item do batch pendente antes de acumular novo batch
        if !self.batch_write.is_empty() {
            let req = self.batch_write.remove(0);
            self.stats_write += 1;
            return Some(req);
        }
        // Acumula novo batch de writes, ordenados por LBA
        while self.batch_write.len() < self.max_batch {
            if let Some(next) = self.queues[2].pop_front() {
                self.batch_write.push(next);
            } else { break; }
        }
        if !self.batch_write.is_empty() {
            self.batch_write.sort_by_key(|r| r.lba);
            let req = self.batch_write.remove(0);
            self.stats_write += 1;
            return Some(req);
        }
        if let Some(req) = self.queues[3].pop_front() { return Some(req); }
        None
    }

    pub fn flush_writes(&mut self) -> Vec<IoRequest> {
        core::mem::take(&mut self.batch_write)
    }

    pub fn tick(&mut self) {
        self.tick += 1;
        if self.tick % 100 == 0 {
            for q in &mut self.queues {
                q.retain(|r| r.deadline_tick == 0 || r.deadline_tick > self.tick);
            }
        }
    }

    pub fn stats(&self) -> (u64, u64, u64, usize) {
        let total: usize = self.queues.iter().map(|q| q.len()).sum();
        (self.stats_read, self.stats_write, self.stats_merged, total)
    }
}
