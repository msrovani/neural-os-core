//! FilesystemDriver trait — driver unificado para todos os FS.
//! Cada FS (FAT32, exFAT, NTFS, EXT2, NeuralFS) implementa este trait.
//! VFS resolve path -> FilesystemAgent -> FilesystemDriver.

use alloc::vec::Vec;
use alloc::string::String;
use crate::block_dev::BlockDevice;

/// Resultado de deteccao de FS
#[derive(Debug, Clone)]
pub struct FsInfo {
    pub fs_type: &'static str,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: Option<u64>,
    pub block_size: u32,
    pub writable: bool,
}

/// Driver unificado para um sistema de arquivos
pub trait FilesystemDriver: Send {
    fn name(&self) -> &str;
    fn detect(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> where Self: Sized;
    fn mount(&mut self, dev: &mut dyn BlockDevice, start_lba: u64) -> Result<FsInfo, &'static str>;
    fn read(&self, path: &str, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str>;
    fn write(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<(), &'static str>;
    fn list(&self, path: &str) -> Result<Vec<(String, bool)>, &'static str>; // (name, is_dir)
    fn free_space(&self) -> u64;
    fn total_space(&self) -> u64;
}
