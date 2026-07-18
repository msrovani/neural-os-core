//! NeuralFsAgent — VFS em /mnt/neural (ATA type 0x7F se existir; senao RAM 4MB).

use alloc::string::String;
use alloc::vec::Vec;
use crate::block_dev::BlockDevice;
use crate::fs::FilesystemAgent;
use crate::neural_fs::volume::{MemoryDisk, NeuralVolume, MBR_TYPE_NEURALFS};
use spin::Mutex;

enum Backend {
    Ram(MemoryDisk),
    Ata { start_lba: u64 },
}

struct NeuralFsState {
    backend: Backend,
    volume: NeuralVolume,
}

pub struct NeuralFsAgent {
    name: String,
    mount_point: String,
    state: Mutex<Option<NeuralFsState>>,
}

impl NeuralFsAgent {
    pub fn new() -> Self {
        let agent = NeuralFsAgent {
            name: String::from("neuralfs"),
            mount_point: String::from("/mnt/neural"),
            state: Mutex::new(None),
        };
        if !agent.try_bootstrap_ata() {
            agent.bootstrap_ram();
        }
        agent
    }

    fn with_dev<R, F>(st: &mut NeuralFsState, f: F) -> Result<R, &'static str>
    where
        F: FnOnce(&mut NeuralVolume, &mut dyn BlockDevice) -> Result<R, &'static str>,
    {
        match &mut st.backend {
            Backend::Ram(disk) => f(&mut st.volume, disk),
            Backend::Ata { start_lba } => {
                let mut guard = crate::ATA_DRIVER.lock();
                let ata = guard.as_mut().ok_or("no ata")?;
                // start_lba ja esta no volume; so precisamos do dispositivo
                let _ = start_lba;
                f(&mut st.volume, ata)
            }
        }
    }

    /// Tenta montar particao MBR 0x7F; se magico ausente e cauda livre >=8MB, formata.
    fn try_bootstrap_ata(&self) -> bool {
        let mut guard = crate::ATA_DRIVER.lock();
        let Some(ata) = guard.as_mut() else {
            return false;
        };
        let parts = crate::fat32::read_mbr(ata);
        // 1) particao NeuralFS existente
        for p in &parts {
            if p.type_code != MBR_TYPE_NEURALFS {
                continue;
            }
            let start = p.lba_start as u64;
            if NeuralVolume::probe_magic(ata, start) {
                if let Some(vol) = NeuralVolume::mount(ata, start) {
                    crate::slog_nano!("NEURALFS", "info", "ATA mount LBA={} free_blocks={} inodes={}",
                        start,
                        vol.sb.free_blocks,
                        vol.sb.allocated_inodes);
                    *self.state.lock() = Some(NeuralFsState {
                        backend: Backend::Ata { start_lba: start },
                        volume: vol,
                    });
                    return true;
                }
            }
            // particao marcada 0x7F sem magic → formatar in-place (nao toca FAT)
            let total_lba = p.sector_count as u64;
            if total_lba >= 16384 {
                if NeuralVolume::format(ata, start, total_lba) {
                    if let Some(mut vol) = NeuralVolume::mount(ata, start) {
                        if let Ok(ino) = vol.create_file(ata, 1, "hello.txt") {
                            let _ = vol.write_file(ata, ino, b"NeuralFS ATA online\n");
                        }
                        crate::slog_nano!("NEURALFS", "info", "ATA format+mount LBA={} size={}MB",
                            start,
                            total_lba * 512 / (1024 * 1024));
                        *self.state.lock() = Some(NeuralFsState {
                            backend: Backend::Ata { start_lba: start },
                            volume: vol,
                        });
                        return true;
                    }
                }
            }
        }
        // 2) cauda livre: so se houver slot MBR vazio e >=8MB apos ultima particao
        if Self::try_format_free_tail(ata, &parts) {
            // remount via recursive unlock — solta o lock e tenta de novo
            drop(guard);
            return self.try_bootstrap_ata();
        }
        false
    }

    fn try_format_free_tail(
        ata: &mut crate::ata::AtaDriver,
        parts: &[crate::fat32::Partition],
    ) -> bool {
        let Some(disk_sectors) = (unsafe { ata.total_sectors() }) else {
            return false;
        };
        if disk_sectors < 16384 {
            return false;
        }
        let mut used_end = 1u64; // reserva MBR
        for p in parts {
            let end = p.lba_start as u64 + p.sector_count as u64;
            if end > used_end {
                used_end = end;
            }
        }
        // alinha a 1MB
        let start = (used_end + 2047) & !2047;
        if start + 16384 > disk_sectors {
            return false;
        }
        let size = disk_sectors - start;
        let mut mbr = [0u8; 512];
        if !BlockDevice::read_sectors(ata, 0, &mut mbr) {
            return false;
        }
        if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
            return false;
        }
        let mut free_slot: Option<usize> = None;
        for i in 0..4 {
            let off = 0x1BE + i * 16;
            if mbr[off + 4] == 0 {
                free_slot = Some(off);
                break;
            }
        }
        let Some(off) = free_slot else {
            return false;
        };
        let size_u32 = if size > u32::MAX as u64 {
            u32::MAX
        } else {
            size as u32
        };
        mbr[off] = 0x00;
        mbr[off + 4] = MBR_TYPE_NEURALFS;
        mbr[off + 8..off + 12].copy_from_slice(&(start as u32).to_le_bytes());
        mbr[off + 12..off + 16].copy_from_slice(&size_u32.to_le_bytes());
        if !BlockDevice::write_sectors(ata, 0, &mbr) {
            return false;
        }
        if !NeuralVolume::format(ata, start, size_u32 as u64) {
            return false;
        }
        crate::slog_nano!("NEURALFS", "info", "created MBR 0x7F LBA={} size={}MB",
            start,
            size_u32 as u64 * 512 / (1024 * 1024));
        true
    }

    fn bootstrap_ram(&self) {
        let mut disk = MemoryDisk::new(4 * 1024 * 1024);
        let total_lba = disk.sector_count();
        if !NeuralVolume::format(&mut disk, 0, total_lba) {
            crate::slog_nano!("NEURALFS", "info", "format RAM FAILED");
            return;
        }
        let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
            crate::slog_nano!("NEURALFS", "info", "mount RAM FAILED");
            return;
        };
        if let Ok(ino) = vol.create_file(&mut disk, 1, "hello.txt") {
            let _ = vol.write_file(&mut disk, ino, b"NeuralFS online\n");
        }
        crate::slog_nano!("NEURALFS", "info", "RAM 4MB mounted free_blocks={} inodes={}",
            vol.sb.free_blocks,
            vol.sb.allocated_inodes);
        *self.state.lock() = Some(NeuralFsState {
            backend: Backend::Ram(disk),
            volume: vol,
        });
        if crate::neural_fs::tests::smoke_ram_roundtrip() {
            crate::slog_nano!("NEURALFS", "info", "smoke_ram_roundtrip=OK");
        } else {
            crate::slog_nano!("NEURALFS", "info", "smoke_ram_roundtrip=FAIL");
        }
        if crate::neural_fs::tests::smoke_reclaim() {
            crate::slog_nano!("NEURALFS", "info", "smoke_reclaim=OK");
        } else {
            crate::slog_nano!("NEURALFS", "info", "smoke_reclaim=FAIL");
        }
        if crate::neural_fs::tests::smoke_split() {
            crate::slog_nano!("NEURALFS", "info", "smoke_split=OK");
        } else {
            crate::slog_nano!("NEURALFS", "info", "smoke_split=FAIL");
        }
    }

    fn parent_and_name(path: &str) -> (&str, &str) {
        let p = path.trim_matches('/');
        if let Some(i) = p.rfind('/') {
            (&p[..i], &p[i + 1..])
        } else {
            ("", p)
        }
    }
}

impl FilesystemAgent for NeuralFsAgent {
    fn name(&self) -> &str {
        &self.name
    }
    fn mount_point(&self) -> &str {
        &self.mount_point
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let mut guard = self.state.lock();
        let st = guard.as_mut().ok_or("not mounted")?;
        Self::with_dev(st, |vol, dev| {
            let ino = vol.resolve_path(dev, path).ok_or("not found")?;
            let (mode, _, _, _) = vol.lookup_inode(dev, ino).ok_or("no inode")?;
            if mode & crate::neural_fs::inode::Inode::S_IFDIR != 0 {
                let list = vol.list_dir(dev, ino).unwrap_or_default();
                let mut s = String::from("dir:\n");
                for n in list {
                    s.push_str(&n);
                    s.push('\n');
                }
                return Ok(s.into_bytes());
            }
            vol.read_file(dev, ino).map_err(|_| "read failed")
        })
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> {
        let mut guard = self.state.lock();
        let st = guard.as_mut().ok_or("not mounted")?;
        let (parent_path, name) = Self::parent_and_name(path);
        if name.is_empty() {
            return Err("bad path");
        }
        let data = data.to_vec();
        Self::with_dev(st, |vol, dev| {
            let parent = vol.resolve_path(dev, parent_path).ok_or("no parent")?;
            let ino = match vol.lookup_dir_entry(dev, parent, name) {
                Some(i) => i,
                None => vol.create_file(dev, parent, name).map_err(|_| "create failed")?,
            };
            vol.write_file(dev, ino, &data).map_err(|_| "write failed")
        })
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        let mut guard = self.state.lock();
        let st = guard.as_mut().ok_or("not mounted")?;
        Self::with_dev(st, |vol, dev| {
            let ino = vol.resolve_path(dev, path).ok_or("not found")?;
            vol.list_dir(dev, ino).map_err(|_| "list failed")
        })
    }
}
