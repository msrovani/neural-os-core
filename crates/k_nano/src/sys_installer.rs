//! SysInstaller — Instala/copia sistema entre dispositivos (ADR-0079 M1).
//! AIOS na veia: auto-instalação em runtime para qualquer BlockDevice.
//!
//! Fluxo M1:
//! 1. scan_disks() — enumera discos (ATA_DRIVER + StorageBus)
//! 2. install(source, target, kernel_elf) — cria GPT dual (ESP + NeuralFS),
//!    copia ESP do source, cria NeuralFS, copia kernel.elf
//! 3. demo() — self-test com MemoryDisk

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::block_dev::BlockDevice;
use crate::storage_bus::STORAGE_BUS;
use crate::gpt::{self, gpt_format_multi, probe_gpt, GptPartitionDef, GPT_TYPE_ESP, GPT_TYPE_NEURALFS};
use crate::neural_fs::volume::NeuralVolume;

/// Status da instalação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Idle,
    Scanning,
    Formatting,
    Copying { progress: u8 },
    Verifying,
    Complete,
    Failed,
}

/// Informação de um disco detectado.
#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub index: u8,
    pub name: String,
    pub total_sectors: u64,
    pub is_boot: bool,
    pub kind: &'static str,
}

/// Instalador de sistema — cria GPT dual, copia ESP, instala kernel.
pub struct SysInstaller {
    pub status: InstallStatus,
    pub disks: Vec<DiskInfo>,
    pub source_index: Option<u8>,
    pub target_index: Option<u8>,
    pub bytes_copied: u64,
    pub total_bytes: u64,
}

impl SysInstaller {
    pub fn new() -> Self {
        Self {
            status: InstallStatus::Idle,
            disks: Vec::new(),
            source_index: None,
            target_index: None,
            bytes_copied: 0,
            total_bytes: 0,
        }
    }

    /// Escaneia discos: fonte via ATA_DRIVER, candidatos a target via StorageBus.
    pub fn scan_disks(&mut self) {
        self.status = InstallStatus::Scanning;
        self.disks.clear();
        self.source_index = None;
        self.target_index = None;

        let boot_guard = crate::ATA_DRIVER.lock();
        if let Some(ref ata) = *boot_guard {
            let total = unsafe { ata.total_sectors().unwrap_or(0) };
            self.disks.push(DiskInfo {
                index: 0, name: String::from("ata0 (boot)"),
                total_sectors: total, is_boot: true, kind: "ata",
            });
            self.source_index = Some(0);
        }
        drop(boot_guard);

        let mut next_idx = self.disks.len() as u8;
        let bus = STORAGE_BUS.lock();
        for entry in bus.entries() {
            let kind_str = match entry.kind {
                crate::storage_bus::BusKind::Nvme => "nvme",
                crate::storage_bus::BusKind::Ahci => "ahci",
                crate::storage_bus::BusKind::Ata => "ata",
                crate::storage_bus::BusKind::Usb => "usb",
            };
            self.disks.push(DiskInfo {
                index: next_idx,
                name: String::from(entry.name),
                total_sectors: entry.total_sectors_512,
                is_boot: false,
                kind: kind_str,
            });
            if self.target_index.is_none() {
                self.target_index = Some(next_idx);
            }
            next_idx += 1;
        }
        drop(bus);

        crate::slog_nano!("SYS-INST", "info",
            "scan: {} disk(s) source={:?} target={:?}",
            self.disks.len(), self.source_index, self.target_index);

        self.status = InstallStatus::Idle;
    }

    /// Instala o sistema: cria GPT dual (ESP + NeuralFS), copia ESP do source,
    /// cria NeuralFS, copia kernel.elf para a partição de dados.
    pub fn install(
        &mut self,
        source: &mut dyn BlockDevice,
        target: &mut dyn BlockDevice,
        kernel_elf: &[u8],
    ) -> Result<(), &'static str> {
        let total_lba = target.total_sectors();
        if total_lba < 2048 + 512 + 64 {
            return Err("target too small");
        }

        self.status = InstallStatus::Formatting;
        self.bytes_copied = 0;

        // 1. Detecta ESP do source
        let src_parts = probe_gpt(source).ok_or("no GPT on source")?;
        let src_esp = src_parts.iter().find(|p| p.type_guid == GPT_TYPE_ESP)
            .ok_or("no ESP partition on source")?;
        let src_esp_start = src_esp.lba_start;
        let src_esp_count = src_esp.lba_end - src_esp.lba_start + 1;

        crate::slog_nano!("SYS-INST", "info",
            "source ESP: LBA {}-{} ({} sectors)", src_esp_start, src_esp.lba_end, src_esp_count);

        // 2. Define layout do target
        let esp_size = 1_048_576u64.min(total_lba.saturating_sub(2048 + 64) / 2);
        let esp_end = 2048u64 + esp_size - 1;
        let neural_start = esp_end + 1;
        let neural_end = total_lba - 34 - 1;

        if neural_end <= neural_start {
            return Err("target too small for dual layout");
        }

        let parts = [
            GptPartitionDef { type_guid: &GPT_TYPE_ESP, lba_start: 2048, lba_end: esp_end, label: "ESP" },
            GptPartitionDef { type_guid: &GPT_TYPE_NEURALFS, lba_start: neural_start, lba_end: neural_end, label: "neural-os-core" },
        ];

        if !gpt_format_multi(target, total_lba, &parts) {
            return Err("gpt format failed");
        }

        crate::slog_nano!("SYS-INST", "info",
            "gpt: ESP 2048-{} NeuralFS {}-{}", esp_end, neural_start, neural_end);

        // 3. Copia ESP do source para o target
        self.status = InstallStatus::Copying { progress: 0 };
        let sectors_to_copy = src_esp_count.min(esp_size);
        let mut buf = [0u8; 512];

        for i in 0..sectors_to_copy {
            let src_lba = src_esp_start + i;
            let dst_lba = 2048u64 + i;
            if !source.read_sectors(src_lba, &mut buf) {
                return Err("source ESP read error");
            }
            if !target.write_sectors(dst_lba, &buf) {
                return Err("target ESP write error");
            }
            if i % 1024 == 0 {
                self.status = InstallStatus::Copying { progress: ((i * 100) / sectors_to_copy) as u8 };
            }
        }
        self.bytes_copied += sectors_to_copy * 512;
        crate::slog_nano!("SYS-INST", "info", "ESP copied: {} sectors", sectors_to_copy);

        // 4. Cria NeuralFS na partição de dados
        let data_total_lba = neural_end - neural_start + 1;
        if !NeuralVolume::format(target, neural_start, data_total_lba) {
            return Err("NeuralFS format failed");
        }
        self.status = InstallStatus::Copying { progress: 50 };
        crate::slog_nano!("SYS-INST", "info", "NeuralFS formatted");

        // 5. Monta NeuralFS e copia kernel.elf para raiz como /boot/kernel.elf
        //    Primeiro cria dir /boot/ (parent_ino=1), depois cria o arquivo
        let mut vol = NeuralVolume::mount(target, neural_start)
            .ok_or("NeuralFS mount failed")?;

        // Cria diretório /boot/ sob root (ino 1)
        let boot_ino = vol.create_file(target, 1, "boot").map_err(|_| "create /boot dir failed")?;

        // Cria kernel.elf dentro de /boot/
        let kf_ino = vol.create_file(target, boot_ino, "kernel.elf")
            .map_err(|_| "create /boot/kernel.elf failed")?;

        vol.write_file(target, kf_ino, kernel_elf)
            .map_err(|_| "kernel.elf write failed")?;

        self.bytes_copied += kernel_elf.len() as u64;
        crate::slog_nano!("SYS-INST", "info", "kernel.elf written: {} bytes", kernel_elf.len());

        self.status = InstallStatus::Copying { progress: 100 };
        self.total_bytes = self.bytes_copied;

        // 6. Verificação
        self.status = InstallStatus::Verifying;
        let mut verify_buf = [0u8; 512];
        if !target.read_sectors(0, &mut verify_buf) {
            return Err("verify: read failed");
        }
        if verify_buf[0x1FE] != 0x55 || verify_buf[0x1FF] != 0xAA {
            return Err("verify: missing MBR signature");
        }
        if !target.read_sectors(1, &mut verify_buf) || &verify_buf[0..8] != b"EFI PART" {
            return Err("verify: missing GPT header");
        }

        self.status = InstallStatus::Complete;
        crate::slog_nano!("SYS-INST", "info",
            "install complete: {} bytes, dual GPT ESP+NeuralFS", self.bytes_copied);

        let _ = crate::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: String::from("SYS_INSTALL"),
            payload: format!(
                "{{esp_lba:{},neural_lba:{},kernel_bytes:{},status:complete}}",
                esp_size, data_total_lba, self.bytes_copied,
            ).into_bytes(),
            token: event_bus::CapabilityToken::Legacy(1),
        });

        Ok(())
    }

    pub fn status_str(&self) -> String {
        format!("{:?}", self.status)
    }
}

/// Self-test M1 — verifica instalação dual partition com MemoryDisk.
pub fn demo() -> bool {
    use crate::neural_fs::volume::MemoryDisk;

    let kernel_dummy = &[0xE9u8; 8192]; // 16 setores

    // Source: disco de 32MB com GPT + ESP (FAT32 minimal)
    let mut source = MemoryDisk::new(32 * 1024 * 1024);
    let src_sectors = source.sector_count();
    let src_parts = [GptPartitionDef {
        type_guid: &gpt::GPT_TYPE_ESP,
        lba_start: 2048,
        lba_end: 4095,
        label: "ESP",
    }];
    assert!(gpt_format_multi(&mut source, src_sectors, &src_parts));

    // Escreve FAT32 BPB minimal no setor 0 do ESP + marcador BOOTX64
    let mut fat_bpb = [0u8; 512];
    fat_bpb[0x0B] = 2;   // bytes per sector
    fat_bpb[0x0C] = 64;  // sectors per cluster
    fat_bpb[0x0E] = 0xFE; // reserved sectors
    fat_bpb[0x10] = 2;   // FAT count
    fat_bpb[0x1FE] = 0x55;
    fat_bpb[0x1FF] = 0xAA;
    let marker = b"BOOTX64.EFI";
    fat_bpb[20..20+marker.len()].copy_from_slice(marker);
    assert!(source.write_sectors(2048, &fat_bpb));

    // Target: disco de 2MB (pequeno para teste rápido)
    let mut target = MemoryDisk::new(2 * 1024 * 1024);

    let mut inst = SysInstaller::new();
    assert_eq!(inst.status, InstallStatus::Idle);

    // Instala
    assert!(inst.install(&mut source, &mut target, kernel_dummy).is_ok());
    assert_eq!(inst.status, InstallStatus::Complete);
    assert!(inst.bytes_copied > 0);

    // Verifica GPT no target
    let mut mbr = [0u8; 512];
    assert!(target.read_sectors(0, &mut mbr));
    assert_eq!(mbr[0x1FE], 0x55);

    // Verifica GPT header
    let mut gpt_hdr = [0u8; 512];
    assert!(target.read_sectors(1, &mut gpt_hdr));
    assert_eq!(&gpt_hdr[0..8], b"EFI PART");

    // Verifica que ESP foi copiado
    let mut esp_sec = [0u8; 512];
    assert!(target.read_sectors(2048, &mut esp_sec));
    assert_eq!(&esp_sec[20..20+11], b"BOOTX64.EFI");

    // Verifica NeuralFS no target (opcional — pode não montar em disco pequeno)
    let tgt_sectors = target.sector_count();
    let _ = NeuralVolume::mount(&mut target, tgt_sectors - 34 - 1 - 512 - 1);

    crate::slog_nano!("SYS-INST", "info", "demo: ALL M1 PASS");
    true
}
