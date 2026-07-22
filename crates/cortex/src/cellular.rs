use alloc::vec::Vec;
use alloc::boxed::Box;
use k_nano::sync::mpmc::MpmcQueue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType { Reasoning, Memory, Perception, Motor }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState { Idle, Active, Blocked, Dead }

pub type CellId = u64;

#[derive(Debug, Clone)]
pub struct CellMessage {
    pub sender: CellId,
    pub recipient: CellId,
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
    pub weight_count: usize,
}

impl CognitiveCell {
    pub fn new(id: CellId, cell_type: CellType, region: usize, weight_count: usize) -> Self {
        CognitiveCell { id, cell_type, state: CellState::Idle, region, weight_count }
    }
}

pub struct CellNetwork {
    cells: Vec<CognitiveCell>,
    queue: Box<MpmcQueue<CellMessage>>,
    next_id: CellId,
    scheduler_cursor: usize,
    budget_per_tick: usize,
}

impl CellNetwork {
    pub fn new(capacity: usize, budget_per_tick: usize) -> Option<Self> {
        let queue = MpmcQueue::new(capacity)?;
        Some(CellNetwork {
            cells: Vec::new(),
            queue: Box::new(queue),
            next_id: 1,
            scheduler_cursor: 0,
            budget_per_tick,
        })
    }

    pub fn spawn_cell(&mut self, cell_type: CellType, region: usize, weight_count: usize) -> CellId {
        let id = self.next_id;
        self.next_id += 1;
        self.cells.push(CognitiveCell::new(id, cell_type, region, weight_count));
        id
    }

    pub fn send(&self, msg: CellMessage) -> Result<(), CellMessage> {
        self.queue.try_send(msg)
    }

    pub fn tick(&mut self) -> usize {
        let mut processed = 0;
        for _ in 0..self.budget_per_tick {
            if let Some(msg) = self.queue.try_recv() {
                if let Some(cell) = self.cells.iter_mut().find(|c| c.id == msg.recipient) {
                    cell.state = CellState::Active;
                    match &msg.payload {
                        CellPayload::Activate(_) | CellPayload::Response(_) => {
                            cell.state = CellState::Idle;
                        }
                        CellPayload::Train { .. } => {
                            cell.state = CellState::Idle;
                        }
                        CellPayload::Shutdown => {
                            cell.state = CellState::Dead;
                        }
                    }
                }
                processed += 1;
            } else {
                break;
            }
        }
        processed
    }

    pub fn round_robin(&mut self) -> Option<CellId> {
        if self.cells.is_empty() {
            return None;
        }
        let start = self.scheduler_cursor;
        for offset in 0..self.cells.len() {
            let idx = (start + offset) % self.cells.len();
            if self.cells[idx].state == CellState::Idle && self.cells[idx].state != CellState::Dead {
                self.cells[idx].state = CellState::Active;
                self.scheduler_cursor = (idx + 1) % self.cells.len();
                return Some(self.cells[idx].id);
            }
        }
        None
    }

    pub fn mark_idle(&mut self, id: CellId) {
        if let Some(cell) = self.cells.iter_mut().find(|c| c.id == id) {
            if cell.state != CellState::Dead {
                cell.state = CellState::Idle;
            }
        }
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn alive_count(&self) -> usize {
        self.cells.iter().filter(|c| c.state != CellState::Dead).count()
    }
}
