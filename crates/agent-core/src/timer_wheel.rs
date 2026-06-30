use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

const WHEEL_SLOTS: usize = 64;

pub struct TimerWheel {
    slots: Vec<VecDeque<WheelEntry>>,
    current_slot: usize,
    pub total_ticks: u64,
}

#[derive(Debug, Clone)]
pub struct WheelEntry {
    pub agent_id: u64,
    pub callback_id: u16,
    pub interval: u64,
}

impl TimerWheel {
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(WHEEL_SLOTS);
        for _ in 0..WHEEL_SLOTS { slots.push(VecDeque::new()); }
        TimerWheel { slots, current_slot: 0, total_ticks: 0 }
    }

    pub fn schedule(&mut self, agent_id: u64, callback_id: u16, delay: u64) {
        let slot = (self.current_slot + delay as usize) % WHEEL_SLOTS;
        self.slots[slot].push_back(WheelEntry { agent_id, callback_id, interval: 0 });
    }

    pub fn schedule_repeating(&mut self, agent_id: u64, callback_id: u16, interval: u64) {
        let slot = (self.current_slot + interval as usize) % WHEEL_SLOTS;
        self.slots[slot].push_back(WheelEntry { agent_id, callback_id, interval });
    }

    pub fn tick(&mut self) -> Vec<WheelEntry> {
        self.total_ticks += 1;
        let due = core::mem::take(&mut self.slots[self.current_slot]);
        self.current_slot = (self.current_slot + 1) % WHEEL_SLOTS;
        let mut result = Vec::new();
        for entry in due {
            if entry.interval > 0 {
                let slot = (self.current_slot + entry.interval as usize) % WHEEL_SLOTS;
                self.slots[slot].push_back(entry.clone());
            }
            result.push(entry);
        }
        result
    }

    pub fn pending(&self) -> usize {
        self.slots.iter().map(|s| s.len()).sum()
    }
}
