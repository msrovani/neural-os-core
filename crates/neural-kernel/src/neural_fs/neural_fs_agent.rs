//! NeuralFsAgent — integracao do NeuralFS com o VFS via FilesystemAgent trait.

use alloc::vec::Vec;
use alloc::string::String;
use crate::fs::{FilesystemAgent};
use crate::neural_fs::volume::NeuralVolume;
use spin::Mutex;

pub struct NeuralFsAgent {
    pub name: String,
    pub mount_point: String,
    pub volume: Mutex<Option<NeuralVolume>>,
    pub start_lba: u64,
}

impl NeuralFsAgent {
    pub fn new(name: &str, mount: &str) -> Self {
        NeuralFsAgent {
            name: String::from(name),
            mount_point: String::from(mount),
            volume: Mutex::new(None),
            start_lba: 0,
        }
    }
}

impl FilesystemAgent for NeuralFsAgent {
    fn name(&self) -> &str { &self.name }
    fn mount_point(&self) -> &str { &self.mount_point }

    fn read(&self, _path: &str) -> Result<Vec<u8>, &str> {
        let vol_guard = self.volume.lock();
        let _vol = vol_guard.as_ref().ok_or("volume not mounted")?;
        // Placeholder: leitura sera implementada quando volume.rs estiver completo
        Err("NeuralFS read: not yet fully implemented")
    }

    fn write(&mut self, _path: &str, _data: &[u8]) -> Result<(), &str> {
        Err("NeuralFS write: not yet fully implemented")
    }

    fn list(&self, _path: &str) -> Result<Vec<String>, &str> {
        Err("NeuralFS list: not yet fully implemented")
    }
}
