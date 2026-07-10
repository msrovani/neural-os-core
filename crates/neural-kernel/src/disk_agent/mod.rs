pub mod cache;
pub mod controller;
pub mod disk_info;
pub mod fs_probe;
pub mod nvme;
pub mod vol_mgr;


use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use cache::ArcCache;
use controller::StorageController;
use disk_info::*;
use fs_probe::FsProbeRegistry;
use vol_mgr::VolMgrRegistry;
use crate::mhi::AllocTier;

pub static DISK_AGENT_INIT: AtomicBool = AtomicBool::new(false);

pub struct DiskIntelligenceAgent {
    manifest: AgentManifest,
    controllers: Vec<Box<dyn StorageController>>,
    disks: Vec<RawDisk>,
    fs_registry: FsProbeRegistry,
    vol_registry: VolMgrRegistry,
    tick_run: bool,
    tick_count: u64,
    io_queue: Vec<(u8, u8, u64, Vec<u8>)>,
    readahead_cache: Vec<(u64, u64, Vec<u8>)>,
    cache: ArcCache,
    last_migration_tick: u64,
}

impl DiskIntelligenceAgent {
    pub fn new() -> Self {
        DiskIntelligenceAgent {
            manifest: AgentManifest {
                name: "DiskIntelligenceAgent",
                kind: AgentKind::System,
                schedule: ScheduleKind::Oneshot,
                auto_start: true,
                persist: false,
            },
            controllers: Vec::new(),
            disks: Vec::new(),
            fs_registry: FsProbeRegistry::new(),
            vol_registry: VolMgrRegistry::new(),
            tick_run: false,
            tick_count: 0,
            io_queue: Vec::new(),
            readahead_cache: Vec::new(),
            cache: ArcCache::new(1024), // 1MB cache
            last_migration_tick: 0,
        }
    }

    pub fn register_controller(&mut self, ctrl: Box<dyn StorageController>) {
        self.controllers.push(ctrl);
    }

    pub fn probe_all(&mut self) {
        crate::serial_println!("[DISK] DiskIntelligenceAgent: probing storage...");
        let ctrl_count = self.controllers.len();
        for ctrl_idx in 0..ctrl_count {
            let ctrl_name: String;
            let ctrl_type: ControllerType;
            {
                let ctrl = &mut *self.controllers[ctrl_idx];
                ctrl_name = ctrl.name().into();
                ctrl_type = ctrl.controller_type();
                crate::serial_println!("[DISK]  Controller: {} ({:?})", ctrl_name, ctrl_type);
                let disks = ctrl.probe_disks();
                for mut disk in disks {
                    // S.M.A.R.T. probe
                    disk.smart = self.controllers[ctrl_idx].read_smart(0);
                    if let Some(ref smart) = disk.smart {
                        let status = if smart.healthy { "healthy" } else { "⚠ UNHEALTHY" };
                        crate::serial_println!("[SMART] {}: {}, {}°C, {}h on, realloc={}, pending={}",
                            disk.name, status, smart.temp_c, smart.power_on_hours,
                            smart.realloc_sectors, smart.pending_sectors);
                        if !smart.healthy {
                            crate::serial_println!("[SMART] *** {} HEALTH ALERT: atributos criticos! ***", disk.name);
                        }
                    } else {
                        crate::serial_println!("[SMART] {}: S.M.A.R.T. nao disponivel", disk.name);
                    }

                    self.read_partitions(&mut disk, ctrl_idx);
                    self.detect_fs(&mut disk, ctrl_idx);
                    self.detect_volume_mgrs(&mut disk, ctrl_idx);
                    self.register_mhi(&disk);
                    self.mount_vfs(&disk);
                    self.disks.push(disk);
                }
            }
        }
        self.print_topology();
        DISK_AGENT_INIT.store(true, Ordering::Release);
    }

    fn io_scheduler_enqueue(&mut self, ctrl_idx: u8, disk: u8, lba: u64, data: Vec<u8>) {
        self.io_queue.push((ctrl_idx, disk, lba, data));
        if self.io_queue.len() >= 32 { self.io_scheduler_flush(); }
    }

    fn io_scheduler_flush(&mut self) {
        if self.io_queue.is_empty() { return; }
        let batch = core::mem::take(&mut self.io_queue);
        for (ctrl_idx, disk, lba, data) in batch {
            if (ctrl_idx as usize) < self.controllers.len() {
                self.controllers[ctrl_idx as usize].write_blocks(disk, lba, &data, (data.len() + 511) / 512);
            }
        }
    }

    fn readahead_hint(&mut self, ctrl_idx: u8, disk: u8, lba: u64, count: u64) {
        let key = ((disk as u64) << 56) | (ctrl_idx as u64) << 48 | lba;
        if self.readahead_cache.iter().any(|(k, _, _)| *k == key) { return; }
        let prefetch_blocks = 32usize.min(4096 / 512);
        let mut buf = alloc::vec![0u8; prefetch_blocks * 512];
        if (ctrl_idx as usize) < self.controllers.len() {
            self.controllers[ctrl_idx as usize].read_blocks(disk, lba + count, &mut buf, prefetch_blocks);
        }
        self.readahead_cache.push((key, lba + count, buf));
        if self.readahead_cache.len() > 64 { self.readahead_cache.remove(0); }
    }

    fn read_partitions(&self, disk: &mut RawDisk, ctrl_idx: usize) {
        let read_fn = |lba: u64, buf: &mut [u8]| -> bool {
            if ctrl_idx < self.controllers.len() {
                self.controllers[ctrl_idx].read_blocks(0, lba, buf, (buf.len() + 511) / 512)
            } else { false }
        };

        // Try GPT first (priority over MBR)
        if let Some(gpt_parts) = self.probe_gpt(&read_fn, disk.max_read_bw_mbs) {
            disk.partitions = gpt_parts;
            return;
        }

        // Fallback to MBR
        self.probe_mbr(disk, &read_fn);
    }

    fn probe_gpt(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, bw_mbs: u32) -> Option<Vec<PartitionInfo>> {
        // Check MBR protective entry
        let mut mbr = [0u8; 512];
        if !read_fn(0, &mut mbr) { return None; }
        if mbr[510] != 0x55 || mbr[511] != 0xAA { return None; }

        // MBR partition type 0xEE = GPT protective (must be at entry 0 or entry matching all-zeros area)
        let has_protective = (0..4).any(|i| mbr[0x1BE + i * 16 + 4] == 0xEE);
        if !has_protective { return None; }

        // Read GPT header at LBA 1
        let mut hdr = [0u8; 512];
        if !read_fn(1, &mut hdr) { return None; }
        if &hdr[0..8] != b"EFI PART" { return None; }

        let revision = u32::from_le_bytes([hdr[8], hdr[9], hdr[10], hdr[11]]);
        if revision < 0x00010000 { return None; } // need at least rev 1.0

        let entries_lba = u64::from_le_bytes([hdr[72], hdr[73], hdr[74], hdr[75], hdr[76], hdr[77], hdr[78], hdr[79]]);
        let entry_count = u32::from_le_bytes([hdr[80], hdr[81], hdr[82], hdr[83]]);
        let entry_size = u32::from_le_bytes([hdr[84], hdr[85], hdr[86], hdr[87]]);
        if entry_count > 128 || entry_size != 128 { return None; }

        // Read partition entries (typically LBA 2-33)
        let entries_per_block = 512 / entry_size as usize; // typically 4 per sector
        let total_blocks = (entry_count as usize + entries_per_block - 1) / entries_per_block;
        let mut parts = Vec::new();
        for blk in 0..total_blocks {
            let mut buf = [0u8; 512];
            if !read_fn(entries_lba + blk as u64, &mut buf) { break; }
            for ent in 0..entries_per_block {
                let off = ent * entry_size as usize;
                let type_guid = &buf[off..off+16];
                if type_guid.iter().all(|&b| b == 0) { continue; }
                let lba_start = u64::from_le_bytes([buf[off+32], buf[off+33], buf[off+34], buf[off+35],
                    buf[off+36], buf[off+37], buf[off+38], buf[off+39]]);
                let lba_end = u64::from_le_bytes([buf[off+40], buf[off+41], buf[off+42], buf[off+43],
                    buf[off+44], buf[off+45], buf[off+46], buf[off+47]]);
                let attrs = u64::from_le_bytes([buf[off+56], buf[off+57], buf[off+58], buf[off+59],
                    buf[off+60], buf[off+61], buf[off+62], buf[off+63]]);
                let name_utf16 = &buf[off+64..off+108];
                let name = String::from_utf16le(name_utf16).unwrap_or(String::new());
                let _name_str = name.trim_end_matches('\0');

                let mbr_type = match type_guid {
                    g if g == &[0x28,0x73,0x2A,0xC1,0x1F,0xF8,0xD2,0x11,0xBA,0x4B,0x00,0xA0,0xC9,0x3E,0xC9,0x3B] => 0xEF, // ESP
                    g if g == &[0xA2,0xA0,0xD0,0xEB,0xE5,0xB9,0x33,0x44,0x87,0xC0,0x68,0xB6,0xB7,0x26,0x99,0xC7] => 0x07, // NTFS
                    g if g == &[0xAF,0x3D,0xC6,0x0F,0x83,0x84,0x72,0x47,0x8E,0x79,0x3D,0x69,0xD8,0x47,0x7D,0xE4] => 0x83, // Linux
                    g if g == &[0x79,0xD3,0xD6,0xE6,0xF5,0x07,0x44,0xC2,0xA2,0x3C,0x23,0x8F,0x2A,0x3D,0xF9,0x28] => 0x8E, // LVM
                    _ => 0xEE, // unknown GPT type
                };
                let tier = AllocTier::from_usb_bw(bw_mbs);

                parts.push(PartitionInfo {
                    index: parts.len() as u8,
                    lba_start,
                    lba_end,
                    sector_count: lba_end - lba_start,
                    mbr_type,
                    fs_info: None,
                    is_bootable: attrs & 0x0000000000000004 == 0, // bit 2 = no auto mount
                    mhi_tier: tier,
                    mount_point: None,
                });
            }
        }
        if parts.is_empty() { None } else { Some(parts) }
    }

    fn probe_mbr(&self, disk: &mut RawDisk, read_fn: &dyn Fn(u64, &mut [u8]) -> bool) {
        let mut mbr = [0u8; 512];
        if !read_fn(0, &mut mbr) { return; }
        if mbr[510] != 0x55 || mbr[511] != 0xAA { return; }
        for i in 0..4 {
            let off = 0x1BE + i * 16;
            let ptype = mbr[off + 4];
            if ptype == 0x00 || ptype == 0xEE { continue; }
            let lba_start = u32::from_le_bytes([mbr[off+8], mbr[off+9], mbr[off+10], mbr[off+11]]);
            let count = u32::from_le_bytes([mbr[off+12], mbr[off+13], mbr[off+14], mbr[off+15]]);
            if count == 0 { continue; }
            let tier = AllocTier::from_usb_bw(disk.max_read_bw_mbs);
            disk.partitions.push(PartitionInfo {
                index: i as u8, lba_start: lba_start as u64,
                lba_end: lba_start as u64 + count as u64,
                sector_count: count as u64, mbr_type: ptype,
                fs_info: None, is_bootable: mbr[off] == 0x80,
                mhi_tier: tier, mount_point: None,
            });
        }
    }

    fn detect_fs(&self, disk: &mut RawDisk, ctrl_idx: usize) {
        for part in &mut disk.partitions {
            let base_lba = part.lba_start;
            let read_fn = |lba: u64, buf: &mut [u8]| -> bool {
                if ctrl_idx < self.controllers.len() {
                    self.controllers[ctrl_idx].read_blocks(0, base_lba + lba, buf, (buf.len() + 511) / 512)
                } else { false }
            };
            part.fs_info = self.fs_registry.detect(&read_fn, part.sector_count);

            // V2: tenta montar exFAT se FAT32 nao for detectado
            if part.fs_info.is_none() && ctrl_idx < self.controllers.len() {
                let mut temp_buf = [0u8; 512];
                if read_fn(0, &mut temp_buf) && &temp_buf[3..14] == b"EXFAT   " {
                    part.fs_info = Some(disk_info::FsInfo {
                        fs_type: disk_info::FilesystemType::ExFat,
                        label: alloc::format!("exFAT"),
                        uuid: alloc::string::String::new(),
                        total_bytes: 0,
                        free_bytes: None,
                        block_size: 512,
                        is_writeable: true,
                    });
                    crate::serial_println!("[DISK]  Partition {}: exFAT detectado em {}", part.index, disk.name);
                }
            }
        }
    }

    fn detect_volume_mgrs(&self, disk: &mut RawDisk, ctrl_idx: usize) {
        let read_fn = |lba: u64, buf: &mut [u8]| -> bool {
            if ctrl_idx < self.controllers.len() {
                self.controllers[ctrl_idx].read_blocks(0, lba, buf, (buf.len() + 511) / 512)
            } else { false }
        };
        let max_lba = disk.capacity_bytes / disk.sector_size as u64;
        disk.volume_groups = self.vol_registry.detect_all(&read_fn, max_lba);
    }

    fn register_mhi(&self, disk: &RawDisk) {
        for part in &disk.partitions {
            let phys = part.lba_start * disk.sector_size as u64;
            let size = part.sector_count * disk.sector_size as u64;
            crate::mhi::MHI_REGISTRY.lock().register(
                x86_64::PhysAddr::new(phys), size as usize, part.mhi_tier, &disk.name);
            crate::serial_println!("[DISK]  MHI: {} part{} {}MB tier={:?}",
                disk.name, part.index, size / (1024*1024), part.mhi_tier);
        }
    }

    fn mount_vfs(&self, disk: &RawDisk) {
        for part in &disk.partitions {
            let label = part.fs_info.as_ref().map(|f| f.label.clone()).unwrap_or(String::from("data"));
            let mount = alloc::format!("/mnt/{}/p{}", disk.name, part.index);
            if let Some(ref mut vfs) = *crate::vfs::VFS.lock() {
                vfs.mount(Box::leak(mount.clone().into_boxed_str()), Box::leak(label.clone().into_boxed_str()));
            }
            crate::serial_println!("[DISK]  Mount: {} -> {}", mount, label);
        }
    }

    fn print_topology(&self) {
        crate::serial_println!("[DISK] === Topology ===");
        for disk in &self.disks {
            crate::serial_println!("[DISK]  {}: {}GB {:?} ({})",
                disk.name, disk.capacity_bytes / (1024*1024*1024), disk.interface, disk.controller);
            for part in &disk.partitions {
                let fs = part.fs_info.as_ref().map(|f| alloc::format!("{:?}", f.fs_type)).unwrap_or("?".into());
                let vol = part.mount_point.as_ref().map(|m| alloc::format!(" -> {}", m)).unwrap_or(String::new());
                crate::serial_println!("[DISK]    p{}: {:#04x} {} {}", part.index, part.mbr_type, fs, vol);
            }
            for vg in &disk.volume_groups {
                crate::serial_println!("[DISK]    VG: {} ({:?}) uuid={}", vg.name, vg.technology, vg.uuid);
            }
        }
        crate::serial_println!("[DISK] === End Topology ===");
    }
}

impl Agent for DiskIntelligenceAgent {
    fn manifest(&self) -> &AgentManifest {
        &self.manifest
    }

    fn tick(&mut self, _tick: u64, _tick_count: u64) -> AgentTickResult {
        if !self.tick_run {
            self.tick_run = true;
            self.probe_all();
            AgentTickResult::Done
        } else {
            self.tick_count += 1;

            // I/O scheduler flush (a cada 10 ticks)
            if self.tick_count % 10 == 0 {
                self.io_scheduler_flush();
            }

            // Cache write-back flush (a cada 100 ticks)
            if self.tick_count % 100 == 0 {
                let ctrl_idx = 0;
                let mut flush_fn = |lba: u64, data: &[u8]| {
                    if ctrl_idx < self.controllers.len() {
                        self.controllers[ctrl_idx].write_blocks(0, lba, data, (data.len() + 511) / 512);
                    }
                };
                self.cache.tick(&mut flush_fn);
            }

            // MHI tier migration + hotplug (a cada 1000 ticks)
            if self.tick_count % 1000 == 0 {
                self.last_migration_tick = self.tick_count;
                self.run_tier_migration();
            }

            // Hotplug scan (a cada 100 ticks)
            if self.tick_count % 100 == 0 {
                for ctrl_idx in 0..self.controllers.len() {
                    if self.controllers[ctrl_idx].controller_type() == ControllerType::Usb {
                        let new_disks = self.controllers[ctrl_idx].probe_disks();
                        for disk in new_disks {
                            if !self.disks.iter().any(|d| d.name == disk.name) {
                                crate::serial_println!("[DISK] Hotplug: {} detectado!", disk.name);
                            }
                        }
                    }
                }
            }

            AgentTickResult::Done
        }
    }
}

impl DiskIntelligenceAgent {
    fn run_tier_migration(&mut self) {
        let tick = self.tick_count;
        for disk in &self.disks {
            for part in &disk.partitions {
                let ideal = AllocTier::from_usb_bw(disk.max_read_bw_mbs);
                if part.mhi_tier != ideal {
                    let phys = part.lba_start * disk.sector_size as u64;
                    let size = part.sector_count * disk.sector_size as u64;
                    crate::mhi::MHI_REGISTRY.lock().register(
                        x86_64::PhysAddr::new(phys), size as usize, ideal, &disk.name);
                    crate::serial_println!("[MHI] Tier migration: {} p{} {:?}→{:?} @ tick {}",
                        disk.name, part.index, part.mhi_tier, ideal, tick);
                }
            }
        }
            crate::serial_println!("[MHI] {} allocations", crate::mhi::MHI_REGISTRY.lock().allocations.len());
    }
}
