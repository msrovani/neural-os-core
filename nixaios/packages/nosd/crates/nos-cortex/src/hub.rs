//! ModelHub 8 slots + LOO seam (PORT ADR-0078).

use nos_core::ModelSlot;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SlotState {
    pub path: Option<PathBuf>,
    pub loaded: bool,
    pub bytes_len: u64,
}

impl Default for SlotState {
    fn default() -> Self {
        Self {
            path: None,
            loaded: false,
            bytes_len: 0,
        }
    }
}

#[derive(Clone, Default)]
pub struct ModelHub {
    slots: Arc<RwLock<HashMap<ModelSlot, SlotState>>>,
}

impl ModelHub {
    pub fn new() -> Self {
        let mut m = HashMap::new();
        for s in ModelSlot::ALL {
            m.insert(s, SlotState::default());
        }
        Self {
            slots: Arc::new(RwLock::new(m)),
        }
    }

    pub fn register_path(&self, slot: ModelSlot, path: PathBuf, len: u64) -> bool {
        let mut g = self.slots.write();
        let st = g.get_mut(&slot).unwrap();
        st.path = Some(path);
        st.bytes_len = len;
        st.loaded = false;
        true
    }

    /// LOO: marca loaded sob demanda (mmap real fica no HotSwap).
    pub fn ensure_loaded(&self, slot: ModelSlot) -> bool {
        let mut g = self.slots.write();
        let st = g.get_mut(&slot).unwrap();
        if st.path.is_none() {
            return false;
        }
        st.loaded = true;
        true
    }

    pub fn loaded_count(&self) -> u8 {
        self.slots
            .read()
            .values()
            .filter(|s| s.loaded)
            .count() as u8
    }

    pub fn list(&self) -> Vec<(ModelSlot, SlotState)> {
        let g = self.slots.read();
        ModelSlot::ALL
            .into_iter()
            .map(|s| (s, g.get(&s).cloned().unwrap_or_default()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loo_marks_loaded() {
        let h = ModelHub::new();
        assert!(!h.ensure_loaded(ModelSlot::Active));
        h.register_path(ModelSlot::Active, PathBuf::from("/tmp/a.gguf"), 100);
        assert!(h.ensure_loaded(ModelSlot::Active));
        assert_eq!(h.loaded_count(), 1);
    }
}
