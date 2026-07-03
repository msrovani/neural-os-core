pub mod controller;
pub mod disk_info;
pub mod fs_probe;
pub mod vol_mgr;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
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

    fn read_partitions(&self, disk: &mut RawDisk, ctrl_idx: usize) {
        let read_fn = |lba: u64, buf: &mut [u8]| -> bool {
            if ctrl_idx < self.controllers.len() {
                self.controllers[ctrl_idx].read_blocks(0, lba, buf, (buf.len() + 511) / 512)
            } else { false }
        };
        let mut mbr = [0u8; 512];
        if !read_fn(0, &mut mbr) { return; }
        if mbr[510] != 0x55 || mbr[511] != 0xAA { return; }
        for i in 0..4 {
            let off = 0x1BE + i * 16;
            let ptype = mbr[off + 4];
            if ptype == 0x00 { continue; }
            let lba_start = u32::from_le_bytes([mbr[off+8], mbr[off+9], mbr[off+10], mbr[off+11]]);
            let count = u32::from_le_bytes([mbr[off+12], mbr[off+13], mbr[off+14], mbr[off+15]]);
            if count == 0 { continue; }
            let tier = AllocTier::from_usb_bw(disk.max_read_bw_mbs);
            disk.partitions.push(PartitionInfo {
                index: i as u8,
                lba_start: lba_start as u64,
                lba_end: lba_start as u64 + count as u64,
                sector_count: count as u64,
                mbr_type: ptype,
                fs_info: None,
                is_bootable: mbr[off] == 0x80,
                mhi_tier: tier,
                mount_point: None,
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
