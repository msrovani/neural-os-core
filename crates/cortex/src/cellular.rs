use alloc::vec::Vec;
use alloc::boxed::Box;
use k_nano::sync::mpmc::MpmcQueue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType { Reasoning, Memory, Perception, Motor }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState { Idle, Active, Blocked, Dead }

pub type CellId = u64;
pub type CellIndex = usize;

#[derive(Debug, Clone)]
pub struct CellMessage {
    pub sender: CellId,
    pub payload: CellPayload,
}

#[derive(Debug, Clone)]
pub enum CellPayload {
    Activate(Vec<f32>),
    Response(Vec<f32>),
    Train { input: Vec<f32>, target: Vec<f32> },
    Shutdown,
}

pub struct CognitiveCell {
    pub id: CellId,
    pub cell_type: CellType,
    pub state: CellState,
    pub region: usize,
    pub inbox: Box<MpmcQueue<CellMessage>>,
    pub dead_since: u64,
    pub fan_in: Vec<CellIndex>,
    pub fan_out: Vec<CellIndex>,
}

impl CognitiveCell {
    fn new(id: CellId, cell_type: CellType, region: usize, inbox_cap: usize) -> Option<Self> {
        let inbox = MpmcQueue::new(inbox_cap)?;
        Some(CognitiveCell {
            id,
            cell_type,
            state: CellState::Idle,
            region,
            inbox: Box::new(inbox),
            dead_since: 0,
            fan_in: Vec::new(),
            fan_out: Vec::new(),
        })
    }

    pub fn unprocessed(&self) -> usize {
        self.inbox.len()
    }
}

pub struct CellNetwork {
    cells: Vec<CognitiveCell>,
    next_id: CellId,
    scheduler_cursor: CellIndex,
    budget_per_tick: usize,
    inbox_capacity: usize,
    tick: u64,
    reap_after_ticks: u64,
}

impl CellNetwork {
    pub fn new(inbox_capacity: usize, budget_per_tick: usize) -> Option<Self> {
        Some(CellNetwork {
            cells: Vec::new(),
            next_id: 1,
            scheduler_cursor: 0,
            budget_per_tick,
            inbox_capacity,
            tick: 0,
            reap_after_ticks: 1000,
        })
    }

    pub fn spawn_cell(&mut self, cell_type: CellType, region: usize) -> Option<CellId> {
        let id = self.next_id;
        self.next_id += 1;
        let idx = self.cells.len();
        let cell = CognitiveCell::new(id, cell_type, region, self.inbox_capacity)?;
        self.cells.push(cell);
        Some(id)
    }

    pub fn connect(&mut self, from_id: CellId, to_id: CellId) {
        let from_idx = self.index_of(from_id);
        let to_idx = self.index_of(to_id);
        if let (Some(fi), Some(ti)) = (from_idx, to_idx) {
            if !self.cells[fi].fan_out.contains(&ti) {
                self.cells[fi].fan_out.push(ti);
            }
            if !self.cells[ti].fan_in.contains(&fi) {
                self.cells[ti].fan_in.push(fi);
            }
        }
    }

    fn index_of(&self, id: CellId) -> Option<CellIndex> {
        self.cells.iter().position(|c| c.id == id)
    }

    pub fn send(&self, sender: CellId, recipient: CellId, payload: CellPayload) -> Result<(), CellPayload> {
        let idx = self.index_of(recipient);
        match idx {
            Some(ri) if self.cells[ri].state != CellState::Dead => {
                let msg = CellMessage { sender, payload };
                self.cells[ri].inbox.try_send(msg).map_err(|m| m.payload)
            }
            _ => Err(payload),
        }
    }

    pub fn send_to_index(&self, sender: CellId, recipient_idx: CellIndex, payload: CellPayload) -> Result<(), CellPayload> {
        if recipient_idx < self.cells.len() && self.cells[recipient_idx].state != CellState::Dead {
            let msg = CellMessage { sender, payload };
            self.cells[recipient_idx].inbox.try_send(msg).map_err(|m| m.payload)
        } else {
            Err(payload)
        }
    }

    pub fn drain_cell(&self, id: CellId) -> Vec<CellMessage> {
        let mut msgs = Vec::new();
        if let Some(idx) = self.index_of(id) {
            while let Some(msg) = self.cells[idx].inbox.try_recv() {
                msgs.push(msg);
            }
        }
        msgs
    }

    pub fn tick_advance(&mut self) {
        self.tick += 1;
    }

    pub fn round_robin(&mut self) -> Option<(CellId, Vec<CellMessage>)> {
        if self.cells.is_empty() {
            return None;
        }
        let start = self.scheduler_cursor;
        for offset in 0..self.cells.len() {
            let idx = (start + offset) % self.cells.len();
            if self.cells[idx].state == CellState::Idle && self.cells[idx].inbox.len() > 0 {
                self.cells[idx].state = CellState::Active;
                self.scheduler_cursor = (idx + 1) % self.cells.len();
                let id = self.cells[idx].id;
                let msgs = self.drain_cell(id);
                return Some((id, msgs));
            }
        }
        None
    }

    pub fn mark_processed(&mut self, id: CellId) {
        if let Some(idx) = self.index_of(id) {
            if self.cells[idx].state != CellState::Dead {
                self.cells[idx].state = CellState::Idle;
            }
        }
    }

    pub fn mark_dead(&mut self, id: CellId) {
        if let Some(idx) = self.index_of(id) {
            if self.cells[idx].state != CellState::Dead {
                self.cells[idx].state = CellState::Dead;
                self.cells[idx].dead_since = self.tick;
            }
        }
    }

    pub fn fan_out(&self, id: CellId) -> &[CellIndex] {
        self.index_of(id)
            .and_then(|idx| Some(self.cells[idx].fan_out.as_slice()))
            .unwrap_or(&[])
    }

    pub fn fan_in(&self, id: CellId) -> &[CellIndex] {
        self.index_of(id)
            .and_then(|idx| Some(self.cells[idx].fan_in.as_slice()))
            .unwrap_or(&[])
    }

    pub fn broadcast(&self, sender: CellId, payload: CellPayload) {
        let idx = self.index_of(sender);
        if let Some(si) = idx {
            let out = self.cells[si].fan_out.clone();
            for &ti in &out {
                let _ = self.send_to_index(sender, ti, payload.clone());
            }
        }
    }

    pub fn reap_dead(&mut self) {
        self.cells.retain(|c| {
            c.state != CellState::Dead || (self.tick - c.dead_since < self.reap_after_ticks)
        });
        self.scheduler_cursor = self.scheduler_cursor.min(
            self.cells.len().saturating_sub(1)
        );
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn alive_count(&self) -> usize {
        self.cells.iter().filter(|c| c.state != CellState::Dead).count()
    }

    pub fn get_cell(&self, id: CellId) -> Option<&CognitiveCell> {
        self.index_of(id).map(|idx| &self.cells[idx])
    }

    pub fn get_cell_mut(&mut self, id: CellId) -> Option<&mut CognitiveCell> {
        self.index_of(id).map(move |idx| &mut self.cells[idx])
    }

    pub fn cells(&self) -> &[CognitiveCell] {
        &self.cells
    }
}
