//! NeuralFsAgent — VFS em /mnt/neural.
//! Ordem: ATA (MBR 0x7F / GPT NeuralFS) → USB-MSC (mount sempre; format opt-in) → RAM 4MB.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::block_dev::BlockDevice;
use crate::neural_fs::volume::{MemoryDisk, NeuralVolume, MBR_TYPE_NEURALFS};
use spin::Mutex;

/// Opt-in runtime: permitir formatar NeuralFS em USB.
/// Default `false`. `debug_assertions` tambem libera (ciclo QEMU/dev).
/// CONFIG.TXT: `NEURALFS_USB_FORMAT=1` no volume de dados do stick.
static USB_FORMAT_ALLOWED: AtomicBool = AtomicBool::new(false);

/// Libera formatacao USB em runtime (testes / tools).
pub fn allow_usb_format(enable: bool) {
    USB_FORMAT_ALLOWED.store(enable, Ordering::Relaxed);
}

fn usb_format_allowed(dev: &mut dyn BlockDevice) -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    if USB_FORMAT_ALLOWED.load(Ordering::Relaxed) {
        return true;
    }
    if config_flag_true(dev, "NEURALFS_USB_FORMAT") {
        USB_FORMAT_ALLOWED.store(true, Ordering::Relaxed);
        return true;
    }
    false
}

/// Le CONFIG.TXT (exFAT ou FAT32) e procura `KEY=1` / `KEY=true`.
fn config_flag_true(dev: &mut dyn BlockDevice, key: &str) -> bool {
    let Some(data) = peek_config_txt(dev) else {
        return false;
    };
    let Ok(text) = core::str::from_utf8(&data) else {
        return false;
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start().trim_start_matches('=').trim();
            if rest.eq_ignore_ascii_case("1")
                || rest.eq_ignore_ascii_case("true")
                || rest.eq_ignore_ascii_case("yes")
            {
                return true;
            }
        }
    }
    false
}

fn peek_config_txt(dev: &mut dyn BlockDevice) -> Option<Vec<u8>> {
    let parts = crate::fat32::read_mbr_dev(dev);
    for p in &parts {
        let start = p.lba_start as u64;
        let mut vbr = [0u8; 512];
        if !dev.read_sectors(start, &mut vbr) {
            continue;
        }
        if &vbr[3..11] != b"EXFAT   " {
            continue;
        }
        let Some(mut ex) = crate::exfat::ExfatReader::new(dev, start) else {
            continue;
        };
        for (fname, is_dir, cluster, size) in ex.list_root() {
            if is_dir {
                continue;
            }
            if fname.eq_ignore_ascii_case("CONFIG.TXT") {
                return ex.read_file(cluster, size.min(4096) as usize);
            }
        }
    }
    None
}

enum Backend {
    Ram(MemoryDisk),
    Ata { start_lba: u64 },
    Usb { start_lba: u64 },
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
        if agent.try_bootstrap_ata() {
            agent.ensure_ecosystem_tree();
            Self::try_exfat_write_smoke();
            return agent;
        }
        if agent.try_bootstrap_usb() {
            agent.ensure_ecosystem_tree();
            Self::try_exfat_write_smoke();
            return agent;
        }
        agent.bootstrap_ram();
        agent.ensure_ecosystem_tree();
        Self::try_exfat_write_smoke();
        agent
    }

    /// IDEA #417 — write opt-in: `EXFAT_WRITE=1` no CONFIG.TXT do volume exFAT.
    fn try_exfat_write_smoke() {
        let mut guard = crate::ATA_DRIVER.lock();
        let Some(ata) = guard.as_mut() else {
            return;
        };
        if !config_flag_true(ata, "EXFAT_WRITE") {
            crate::slog_bin!(
                "EXFAT",
                "info",
                "write smoke SKIP (set EXFAT_WRITE=1 in CONFIG.TXT)"
            );
            return;
        }
        let parts = crate::fat32::read_mbr_dev(ata);
        for p in &parts {
            let start = p.lba_start as u64;
            let mut vbr = [0u8; 512];
            if !ata.read_sectors(start, &mut vbr) {
                continue;
            }
            if &vbr[3..11] != b"EXFAT   " {
                continue;
            }
            match crate::exfat_write::smoke_write_roundtrip(ata, start) {
                Ok(()) => crate::slog_bin!("EXFAT", "info", "smoke_write=OK"),
                Err(e) => crate::slog_bin!("EXFAT", "info", "smoke_write=FAIL {}", e),
            }
            return;
        }
        crate::slog_bin!("EXFAT", "info", "write smoke SKIP (no exFAT partition)");
    }

    /// ADR-0051 / NeuralFS §12 — árvore canônica do PackageHub.
    fn ensure_ecosystem_tree(&self) {
        let mut guard = self.state.lock();
        let Some(st) = guard.as_mut() else {
            return;
        };
        let result = Self::with_dev(st, |vol, dev| {
            let root = vol.resolve_path(dev, "").ok_or("root missing")?;
            let eco = match vol.lookup_dir_entry(dev, root, "ecosystem") {
                Some(ino) => ino,
                None => vol.create_dir(dev, root, "ecosystem")?,
            };
            for name in [
                "skills",
                "agents",
                "plugins",
                "mcp",
                "models",
                "firmware",
                "workflows",
                "devices",
            ] {
                if vol.lookup_dir_entry(dev, eco, name).is_none() {
                    let _ = vol.create_dir(dev, eco, name);
                }
            }
            // IDEA #6 — /system/trust/ para usb.tbl
            let sys = match vol.lookup_dir_entry(dev, root, "system") {
                Some(ino) => ino,
                None => vol.create_dir(dev, root, "system")?,
            };
            if vol.lookup_dir_entry(dev, sys, "trust").is_none() {
                let _ = vol.create_dir(dev, sys, "trust");
            }
            Ok(())
        });
        match result {
            Ok(()) => crate::slog_bin!("NEURALFS", "info", "ecosystem/ tree ready (ADR-0051)"),
            Err(e) => crate::slog_bin!("NEURALFS", "info", "ecosystem/ tree fail: {}", e),
        }
        // Drop lock before sync (sync re-locks state).
        drop(guard);
        self.sync_usb_trust_table();
    }

    /// Carrega `system/trust/usb.tbl` e aplica `USB_TRUST_ENFORCE` do CONFIG.
    fn sync_usb_trust_table(&self) {
        // Enforce via CONFIG no ATA (mesmo peek do EXFAT_WRITE).
        {
            let mut guard = crate::ATA_DRIVER.lock();
            if let Some(ata) = guard.as_mut() {
                if config_flag_true(ata, "USB_TRUST_ENFORCE") {
                    crate::usb_trust::set_enforce(true);
                }
            }
        }
        let mut guard = self.state.lock();
        let Some(st) = guard.as_mut() else {
            return;
        };
        let loaded = Self::with_dev(st, |vol, dev| {
            let root = vol.resolve_path(dev, "").ok_or("root")?;
            let sys = vol
                .lookup_dir_entry(dev, root, "system")
                .ok_or("no system")?;
            let trust = vol
                .lookup_dir_entry(dev, sys, "trust")
                .ok_or("no trust")?;
            let Some(ino) = vol.lookup_dir_entry(dev, trust, "usb.tbl") else {
                // Persistir tabela RAM (BOOT seed) se existir
                let blob = crate::usb_trust::serialize();
                if blob.len() > 12 {
                    let ino = vol.create_file(dev, trust, "usb.tbl")?;
                    vol.write_file(dev, ino, &blob)?;
                    crate::slog_bin!("USB-TRUST", "info", "created {}", crate::usb_trust::TBL_PATH);
                }
                return Ok(0usize);
            };
            let data = vol.read_file(dev, ino).map_err(|_| "read usb.tbl")?;
            crate::usb_trust::load_bytes(&data).map_err(|_| "parse usb.tbl")
        });
        match loaded {
            Ok(n) => crate::slog_bin!("USB-TRUST", "info", "sync ok entries={}", n),
            Err(e) => crate::slog_bin!("USB-TRUST", "info", "sync skip: {}", e),
        }
    }

    fn with_dev<R, F>(st: &mut NeuralFsState, f: F) -> Result<R, &'static str>
    where
        F: FnOnce(&mut NeuralVolume, &mut dyn BlockDevice) -> Result<R, &'static str>,
    {
        // ADR-0087 §6: envolve o device no CachedDisk (write-through) — TODA
        // operação NeuralFS (read/write/list/mount de /models/ + SGDB) passa
        // pela cache. Transparente: delega total_sectors/name/sync_cache.
        match &mut st.backend {
            Backend::Ram(disk) => {
                let mut cached = crate::disk_agent::cache::CachedDisk::new(disk);
                f(&mut st.volume, &mut cached)
            }
            Backend::Ata { .. } => {
                let mut guard = crate::ATA_DRIVER.lock();
                let ata = guard.as_mut().ok_or("no ata")?;
                let mut cached = crate::disk_agent::cache::CachedDisk::new(ata);
                f(&mut st.volume, &mut cached)
            }
            Backend::Usb { .. } => {
                let mut guard = crate::globals::USB_MSC.lock();
                let msc = guard.as_mut().ok_or("no usb")?;
                let mut cached = crate::disk_agent::cache::CachedDisk::new(msc);
                f(&mut st.volume, &mut cached)
            }
        }
    }

    /// Pendrive: monta NeuralFS existente sempre; formata so com opt-in.
    fn try_bootstrap_usb(&self) -> bool {
        let mut guard = crate::globals::USB_MSC.lock();
        let Some(msc) = guard.as_mut() else {
            return false;
        };
        let total = BlockDevice::total_sectors(msc);
        if total < 16384 {
            crate::slog_bin!("NEURALFS", "info", "USB skip (sectors={} < 8MB)", total);
            return false;
        }
        let allow_fmt = usb_format_allowed(msc);
        if !allow_fmt {
            crate::slog_bin!("NEURALFS", "info", "USB format locked (set NEURALFS_USB_FORMAT=1 or debug build)");
        }
        Self::try_mount_or_format(msc, "USB", allow_fmt, |start, vol| NeuralFsState {
            backend: Backend::Usb { start_lba: start },
            volume: vol,
        })
        .map(|st| {
            *self.state.lock() = Some(st);
            true
        })
        .unwrap_or(false)
    }

    fn try_bootstrap_ata(&self) -> bool {
        let mut guard = crate::ATA_DRIVER.lock();
        let Some(ata) = guard.as_mut() else {
            return false;
        };
        // ATA: format cauda livre permitido (disco de dados QEMU/HW do OS).
        Self::try_mount_or_format(ata, "ATA", true, |start, vol| NeuralFsState {
            backend: Backend::Ata { start_lba: start },
            volume: vol,
        })
        .map(|st| {
            *self.state.lock() = Some(st);
            true
        })
        .unwrap_or(false)
    }

    /// Mount 0x7F/GPT NeuralFS → (se allow_format) format in-place / cauda / GPT virgin.
    fn try_mount_or_format<D, F>(
        dev: &mut D,
        tag: &str,
        allow_format: bool,
        make: F,
    ) -> Option<NeuralFsState>
    where
        D: BlockDevice,
        F: FnOnce(u64, NeuralVolume) -> NeuralFsState,
    {
        let parts = crate::fat32::read_mbr_dev(dev);
        for p in &parts {
            if p.type_code != MBR_TYPE_NEURALFS {
                continue;
            }
            let start = p.lba_start as u64;
            if NeuralVolume::probe_magic(dev, start) {
                if let Some(vol) = NeuralVolume::mount(dev, start) {
                    crate::slog_bin!("NEURALFS", "info", "{} mount LBA={} free_blocks={} inodes={}",
                        tag,
                        start,
                        vol.sb.free_blocks,
                        vol.sb.allocated_inodes);
                    return Some(make(start, vol));
                }
                // F3+F5: volume EXISTE (probe true) mas mount falhou (journal
                // corrompido/CRC) — NUNCA formatar por cima. Wipe destruiria os
                // dados (ex: /models/). Exige fsck/format explícito.
                crate::slog_bin!(
                    "NEURALFS",
                    "error",
                    "{}: volume LBA={} existe mas mount falhou (corrompido?) — sem format",
                    tag,
                    start
                );
                return None;
            }
            let total_lba = p.sector_count as u64;
            if allow_format && total_lba >= 16384 {
                if NeuralVolume::format(dev, start, total_lba) {
                    if let Some(mut vol) = NeuralVolume::mount(dev, start) {
                        if let Ok(ino) = vol.create_file(dev, 1, "hello.txt") {
                            let msg = alloc::format!("NeuralFS {} online\n", tag);
                            let _ = vol.write_file(dev, ino, msg.as_bytes());
                        }
                        crate::slog_bin!("NEURALFS", "info", "{} format+mount LBA={} size={}MB",
                            tag,
                            start,
                            total_lba * 512 / (1024 * 1024));
                        return Some(make(start, vol));
                    }
                }
            }
        }
        if allow_format {
            if Self::try_format_free_tail(dev, &parts, tag) {
                return Self::remount_neural(dev, tag, make);
            }
            if Self::try_format_gpt_virgin(dev, &parts, tag) {
                return Self::remount_neural(dev, tag, make);
            }
        }
        None
    }

    fn remount_neural<D, F>(dev: &mut D, tag: &str, make: F) -> Option<NeuralFsState>
    where
        D: BlockDevice,
        F: FnOnce(u64, NeuralVolume) -> NeuralFsState,
    {
        let parts2 = crate::fat32::read_mbr_dev(dev);
        for p in &parts2 {
            if p.type_code != MBR_TYPE_NEURALFS {
                continue;
            }
            let start = p.lba_start as u64;
            if let Some(mut vol) = NeuralVolume::mount(dev, start) {
                if let Ok(ino) = vol.create_file(dev, 1, "hello.txt") {
                    let msg = alloc::format!("NeuralFS {} online\n", tag);
                    let _ = vol.write_file(dev, ino, msg.as_bytes());
                }
                crate::slog_bin!("NEURALFS", "info", "{} mount after format LBA={}", tag, start);
                return Some(make(start, vol));
            }
        }
        None
    }

    /// Disco GPT/MBR vazio (so protective EE ou sem particoes): GPT dedicada NeuralFS.
    fn try_format_gpt_virgin<D: BlockDevice>(
        dev: &mut D,
        parts: &[crate::fat32::Partition],
        tag: &str,
    ) -> bool {
        let disk_sectors = BlockDevice::total_sectors(dev);
        if disk_sectors < 16384 + 2048 {
            return false;
        }
        // NUNCA formatar disco que tem particoes (incl. protective GPT 0xEE =
        // ESP/bootloader do Limine). Formatacao destrutiva so em disco VIRGEM.
        let has_data = parts.iter().any(|p| p.type_code != 0);
        if has_data {
            crate::slog_bin!("NEURALFS", "info",
                "{} SKIP format: disco tem {} particao(es) (protect boot/ESP)",
                tag, parts.len());
            return false;
        }
        // Vazio, so EE, ou sem assinatura → GPT single NeuralFS
        if !crate::gpt::gpt_format_single(
            dev,
            disk_sectors,
            &crate::gpt::GPT_TYPE_NEURALFS,
            "NeuralFS",
        ) {
            return false;
        }
        let start = 2048u64;
        let size = disk_sectors.saturating_sub(start + 34);
        if size < 16384 {
            return false;
        }
        if !NeuralVolume::format(dev, start, size) {
            return false;
        }
        crate::slog_bin!("NEURALFS", "info", "{} GPT NeuralFS LBA={} size={}MB",
            tag,
            start,
            size * 512 / (1024 * 1024));
        true
    }

    fn try_format_free_tail<D: BlockDevice>(
        dev: &mut D,
        parts: &[crate::fat32::Partition],
        tag: &str,
    ) -> bool {
        let disk_sectors = BlockDevice::total_sectors(dev);
        if disk_sectors < 16384 {
            return false;
        }
        let mut used_end = 1u64;
        for p in parts {
            let end = p.lba_start as u64 + p.sector_count as u64;
            if end > used_end {
                used_end = end;
            }
        }
        // Stick quase vazio (so MBR / sem particoes): usa LBA 2048
        if parts.is_empty() || parts.iter().all(|p| p.type_code == 0) {
            used_end = 1;
        }
        let start = (used_end + 2047) & !2047;
        if start + 16384 > disk_sectors {
            return false;
        }
        let size = disk_sectors - start;
        let mut mbr = [0u8; 512];
        if !dev.read_sectors(0, &mut mbr) {
            return false;
        }
        // Sem assinatura MBR: cria MBR basico (pendrive virgem de teste)
        if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
            mbr = [0u8; 512];
            mbr[0x1FE] = 0x55;
            mbr[0x1FF] = 0xAA;
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
            crate::slog_bin!("NEURALFS", "info", "{} no free MBR slot", tag);
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
        if !dev.write_sectors(0, &mbr) {
            return false;
        }
        if !NeuralVolume::format(dev, start, size_u32 as u64) {
            return false;
        }
        crate::slog_bin!("NEURALFS", "info", "{} created MBR 0x7F LBA={} size={}MB (dev/debug)",
            tag,
            start,
            size_u32 as u64 * 512 / (1024 * 1024));
        true
    }

    fn bootstrap_ram(&self) {
        let mut disk = MemoryDisk::new(4 * 1024 * 1024);
        let total_lba = disk.sector_count();
        if !NeuralVolume::format(&mut disk, 0, total_lba) {
            crate::slog_bin!("NEURALFS", "info", "format RAM FAILED");
            return;
        }
        let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
            crate::slog_bin!("NEURALFS", "info", "mount RAM FAILED");
            return;
        };
        if let Ok(ino) = vol.create_file(&mut disk, 1, "hello.txt") {
            let _ = vol.write_file(&mut disk, ino, b"NeuralFS online\n");
        }
        crate::slog_bin!("NEURALFS", "info", "RAM 4MB mounted free_blocks={} inodes={}",
            vol.sb.free_blocks,
            vol.sb.allocated_inodes);
        *self.state.lock() = Some(NeuralFsState {
            backend: Backend::Ram(disk),
            volume: vol,
        });
        if crate::neural_fs::tests::smoke_ram_roundtrip() {
            crate::slog_bin!("NEURALFS", "info", "smoke_ram_roundtrip=OK");
        } else {
            crate::slog_bin!("NEURALFS", "info", "smoke_ram_roundtrip=FAIL");
        }
        if crate::neural_fs::tests::smoke_reclaim() {
            crate::slog_bin!("NEURALFS", "info", "smoke_reclaim=OK");
        } else {
            crate::slog_bin!("NEURALFS", "info", "smoke_reclaim=FAIL");
        }
        if crate::neural_fs::tests::smoke_split() {
            crate::slog_bin!("NEURALFS", "info", "smoke_split=OK");
        } else {
            crate::slog_bin!("NEURALFS", "info", "smoke_split=FAIL");
        }
        // F14: level2 (B-tree nível ≥2, 4000 keys) e power_loss (journal recover
        // via drop+remount) — os dois caminhos críticos antes só existiam sem
        // caller. Com heap 512MB+ atual, os 64MB do level2 cabem.
        if crate::neural_fs::tests::smoke_level2() {
            crate::slog_bin!("NEURALFS", "info", "smoke_level2=OK");
        } else {
            crate::slog_bin!("NEURALFS", "info", "smoke_level2=FAIL");
        }
        if crate::neural_fs::tests::smoke_power_loss_soft() {
            crate::slog_bin!("NEURALFS", "info", "smoke_power_loss_soft=OK");
        } else {
            crate::slog_bin!("NEURALFS", "info", "smoke_power_loss_soft=FAIL");
        }
        // ponytail: smokes em RAM não cobrem flush real de disco (AWAITING_HW)
    }

    fn parent_and_name(path: &str) -> (&str, &str) {
        let p = path.trim_matches('/');
        if let Some(i) = p.rfind('/') {
            (&p[..i], &p[i + 1..])
        } else {
            ("", p)
        }
    }

    /// NeuralFS v1 guarda até 22 bytes por dir entry; o VFS mantém o path
    /// lógico e codifica componentes longos de forma determinística.
    fn storage_path(path: &str) -> String {
        let mut out = String::new();
        for part in path.trim_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            out.push('/');
            if part.len() <= 22 {
                out.push_str(part);
                continue;
            }
            let mut hash = 0x811c9dc5u32;
            for byte in part.as_bytes() {
                hash ^= *byte as u32;
                hash = hash.wrapping_mul(0x01000193);
            }
            let prefix: String = part.chars().take(13).collect();
            out.push_str(&alloc::format!("{}-{:08x}", prefix, hash));
        }
        if out.is_empty() {
            out.push('/');
        }
        out
    }

    fn ensure_dir_path(
        vol: &mut NeuralVolume,
        dev: &mut dyn BlockDevice,
        path: &str,
    ) -> Result<u64, &'static str> {
        let mut parent = vol.resolve_path(dev, "").ok_or("root missing")?;
        for part in path.trim_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            parent = match vol.lookup_dir_entry(dev, parent, part) {
                Some(ino) => {
                    let (mode, _, _, _) = vol.lookup_inode(dev, ino).ok_or("dir inode")?;
                    if mode & crate::neural_fs::inode::Inode::S_IFDIR == 0 {
                        return Err("parent is file");
                    }
                    ino
                }
                None => vol.create_dir(dev, parent, part)?,
            };
        }
        Ok(parent)
    }
}

impl NeuralFsAgent {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn mount_point(&self) -> &str {
        &self.mount_point
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let mut guard = self.state.lock();
        let st = guard.as_mut().ok_or("not mounted")?;
        let storage_path = Self::storage_path(path);
        Self::with_dev(st, |vol, dev| {
            let ino = vol.resolve_path(dev, &storage_path).ok_or("not found")?;
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

    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> {
        let mut guard = self.state.lock();
        let st = guard.as_mut().ok_or("not mounted")?;
        let storage_path = Self::storage_path(path);
        let (parent_path, name) = Self::parent_and_name(&storage_path);
        if name.is_empty() {
            return Err("bad path");
        }
        let data = data.to_vec();
        Self::with_dev(st, |vol, dev| {
            let parent = Self::ensure_dir_path(vol, dev, parent_path)?;
            let ino = match vol.lookup_dir_entry(dev, parent, name) {
                Some(i) => i,
                None => vol.create_file(dev, parent, name).map_err(|_| "create failed")?,
            };
            vol.write_file(dev, ino, &data).map_err(|_| "write failed")
        })
    }

    pub fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        let mut guard = self.state.lock();
        let st = guard.as_mut().ok_or("not mounted")?;
        let storage_path = Self::storage_path(path);
        Self::with_dev(st, |vol, dev| {
            let ino = vol.resolve_path(dev, &storage_path).ok_or("not found")?;
            vol.list_dir(dev, ino).map_err(|_| "list failed")
        })
    }
}
