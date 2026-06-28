use alloc::collections::VecDeque;
use alloc::vec::Vec;

const WINDOW_TICKS: u64 = 300;
const MAX_HASHES: usize = 64;

fn shash(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

pub struct DedupWindow {
    entries: VecDeque<(u64, u32)>,
}

impl DedupWindow {
    pub fn new() -> Self { DedupWindow { entries: VecDeque::new() } }

    pub fn is_duplicate(&mut self, data: &[u8], tick: u64) -> bool {
        while self.entries.len() > MAX_HASHES { self.entries.pop_front(); }
        while let Some(&(t, _)) = self.entries.front() {
            if t + WINDOW_TICKS < tick { self.entries.pop_front(); } else { break; }
        }
        let h = shash(data);
        for &(_, eh) in &self.entries {
            if eh == h { return true; }
        }
        self.entries.push_back((tick, h));
        false
    }
}
