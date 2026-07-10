//! NeuralFsAgent — integracao do NeuralFS com o VFS via FilesystemAgent trait.

use alloc::vec::Vec;
use alloc::string::String;
use crate::fs::FilesystemAgent;
use crate::neural_fs::volume::NeuralVolume;
use spin::Mutex;

pub struct NeuralFsAgent {
    pub name: String,
    pub mount_point: String,
    pub volume: Mutex<Option<NeuralVolume>>,
    pub start_lba: u64,
}

impl NeuralFsAgent {
    pub fn new(name: &str, mount: &str, start_lba: u64) -> Self {
        NeuralFsAgent {
            name: String::from(name),
            mount_point: String::from(mount),
            volume: Mutex::new(None),
            start_lba,
        }
    }

    pub fn mount(&self, dev: &mut dyn crate::block_dev::BlockDevice) -> bool {
        if let Some(vol) = NeuralVolume::mount(dev, self.start_lba) {
            *self.volume.lock() = Some(vol);
            true
        } else { false }
    }
}

impl FilesystemAgent for NeuralFsAgent {
    fn name(&self) -> &str { &self.name }
    fn mount_point(&self) -> &str { &self.mount_point }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let vol_guard = self.volume.lock();
        let _vol = vol_guard.as_ref().ok_or("volume not mounted")?;
        // Leitura: caminho resolvido via inode tree
        // v1: only root dir listing supported
        if path == "/" || path.is_empty() {
            return Ok(alloc::format!("NeuralFS mounted at {}\n", self.mount_point).into_bytes());
        }
        Err("not found")
    }

    fn write(&mut self, path: &str, _data: &[u8]) -> Result<(), &str> {
        let vol_guard = self.volume.lock();
        let _vol = vol_guard.as_ref().ok_or("volume not mounted")?;
        // v1: only supports /dev/null pattern (discard)
        if path == "/dev/null" { return Ok(()); }
        Err("read-only")
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        let vol_guard = self.volume.lock();
        let _vol = vol_guard.as_ref().ok_or("volume not mounted")?;
        if path == "/" || path.is_empty() {
            return Ok(alloc::vec!["NeuralFS volume".into()]);
        }
        Err("not found")
    }
}
