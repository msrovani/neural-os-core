//! SysInstaller — Instala/copia sistema entre dispositivos (ADR-0040 #421).
//! AIOS na veia: auto-instalação em runtime, sem ferramenta externa.
//!
//! Fluxo MVP:
//! 1. Detecta discos disponíveis (ATA primário/secundário + StorageBus)
//! 2. Identifica source (boot disk) e target (vazio/secundário)
//! 3. Copia setores (MBR/GPT + dados da partição boot)
//! 4. Publica evento SYS_INSTALL no EventBus

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::ata::AtaDriver;
use crate::storage_bus::STORAGE_BUS;

/// Status da instalação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Idle,
    Scanning,
    Copying { progress: u8 }, // 0–100
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
}

/// Instalador de sistema — copia partições boot + dados entre discos.
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

    /// Escaneia discos disponíveis: fonte via ATA_DRIVER, destino via probe secundário
    /// + StorageBus (AHCI/NVMe/USB que já estejam registrados).
    pub fn scan_disks(&mut self) {
        self.status = InstallStatus::Scanning;
        self.disks.clear();
        self.source_index = None;
        self.target_index = None;

        // 1. Disco de boot via ATA_DRIVER
        let boot_guard = crate::ATA_DRIVER.lock();
        if let Some(ref ata) = *boot_guard {
            let total = unsafe { ata.total_sectors().unwrap_or(0) };
            self.disks.push(DiskInfo {
                index: 0,
                name: String::from("ata0 (boot)"),
                total_sectors: total,
                is_boot: true,
            });
            self.source_index = Some(0);
        }
        drop(boot_guard);

        // 2. Probe ATA secundário (0x170, master + slave)
        let mut next_idx = self.disks.len() as u8;
        for &slave in &[false, true] {
            let candidate = AtaDriver {
                io_base: 0x170,
                pci_bus: 0,
                pci_device: 0,
                pci_func: 0,
                slave,
            };
            if unsafe { AtaDriver::detect(0x170, if slave { 0xB0 } else { 0xA0 }) } {
                if let Some(id) = unsafe { candidate.identify() } {
                    let total = (id[60] as u64) | ((id[61] as u64) << 16);
                    if total > 0 && total < 0xFFFFFFFF {
                        let label = if slave { "ata1-slave" } else { "ata1-master" };
                        self.disks.push(DiskInfo {
                            index: next_idx,
                            name: String::from(label),
                            total_sectors: total,
                            is_boot: false,
                        });
                        // Primeiro secundário vira target default
                        if self.target_index.is_none() {
                            self.target_index = Some(next_idx);
                        }
                        next_idx += 1;
                    }
                }
            }
        }

        // 3. StorageBus (AHCI/NVMe/USB)
        let bus = STORAGE_BUS.lock();
        for entry in bus.entries() {
            self.disks.push(DiskInfo {
                index: next_idx,
                name: String::from(entry.name),
                total_sectors: entry.total_sectors_512,
                is_boot: false,
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

    /// Copia o setor de boot (MBR/GPT LBA 0) do source para o target.
    /// Para MVP copia apenas LBA 0–2047 (1MB) = boot record + GPT header + entries.
    /// Retorna quantidade de bytes copiados.
    fn copy_boot_sectors(
        ata_src: &AtaDriver,
        ata_dst: &AtaDriver,
        sectors: u32,
    ) -> Result<u64, &'static str> {
        let mut buf = [0u8; 512];
        for lba in 0..sectors {
            if !unsafe { ata_src.read_sectors(lba, &mut buf, 1) } {
                return Err("source read error");
            }
            if !unsafe { ata_dst.write_sectors(lba, &buf, 1) } {
                return Err("target write error");
            }
        }
        Ok(sectors as u64 * 512)
    }

    /// Executa a instalação: source → target.
    pub fn install(&mut self) -> Result<(), &'static str> {
        let src_idx = self.source_index.ok_or("no source disk")?;
        let tgt_idx = self.target_index.ok_or("no target disk")?;

        if src_idx == tgt_idx {
            return Err("source and target are the same disk");
        }

        self.status = InstallStatus::Copying { progress: 0 };
        self.bytes_copied = 0;

        // Estima total: MBR + GPT (até LBA 33) + boot partition (~64MB = 131072 setores)
        let total_sectors = 131_072u32; // ponytail: hardcoded 64MB para MVP
        self.total_bytes = total_sectors as u64 * 512;

        // Constrói drivers ATA para source e target
        let ata_src = self.build_source_ata()?;
        let ata_dst = self.build_target_ata()?;

        crate::slog_nano!("SYS-INST", "info",
            "copy start: {} -> {} ({} sectors)",
            self.disks[src_idx as usize].name,
            self.disks[tgt_idx as usize].name,
            total_sectors);

        // Copia setor a setor
        let copied = Self::copy_boot_sectors(&ata_src, &ata_dst, total_sectors)?;
        self.bytes_copied = copied;
        self.status = InstallStatus::Copying { progress: 100 };

        // Verificação (leitura de volta)
        self.status = InstallStatus::Verifying;
        let mut verify_buf = [0u8; 512];
        if !unsafe { ata_dst.read_sectors(0, &mut verify_buf, 1) } {
            return Err("verify read failed");
        }
        if verify_buf[0x1FE] != 0x55 || verify_buf[0x1FF] != 0xAA {
            return Err("verify: target LBA 0 missing MBR signature");
        }

        self.status = InstallStatus::Complete;
        crate::slog_nano!("SYS-INST", "info",
            "install complete: {} bytes copied", copied);

        // Audit trail via EventBus
        let _ = crate::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: String::from("SYS_INSTALL"),
            payload: format!(
                "{{src:{},tgt:{},bytes:{}}}",
                self.disks[src_idx as usize].name,
                self.disks[tgt_idx as usize].name,
                copied,
            )
            .into_bytes(),
            token: event_bus::CapabilityToken::Legacy(1),
        });

        Ok(())
    }

    /// Constrói AtaDriver para o disco source (boot disk).
    fn build_source_ata(&self) -> Result<AtaDriver, &'static str> {
        let guard = crate::ATA_DRIVER.lock();
        guard.as_ref().cloned().ok_or("ATA_DRIVER not initialized")
    }

    /// Constrói AtaDriver para o disco target (secundário).
    fn build_target_ata(&self) -> Result<AtaDriver, &'static str> {
        // ponytail: target é o primeiro ATA secundário detectado
        let slave = false;
        if unsafe { AtaDriver::detect(0x170, 0xA0) } {
            Ok(AtaDriver {
                io_base: 0x170,
                pci_bus: 0,
                pci_device: 0,
                pci_func: 0,
                slave,
            })
        } else if unsafe { AtaDriver::detect(0x170, 0xB0) } {
            Ok(AtaDriver {
                io_base: 0x170,
                pci_bus: 0,
                pci_device: 0,
                pci_func: 0,
                slave: true,
            })
        } else {
            Err("no ATA target disk found on secondary bus")
        }
    }

    pub fn status_str(&self) -> String {
        format!("{:?}", self.status)
    }
}

/// Self-test — verifica que o módulo compila e o ciclo básico funciona.
pub fn demo() -> bool {
    let mut inst = SysInstaller::new();
    assert_eq!(inst.status, InstallStatus::Idle);
    assert!(inst.source_index.is_none());
    inst.scan_disks();
    // scan pode ou não encontrar discos (test env) — não falha
    let ok = inst.disks.len() == 0 || inst.source_index.is_some();
    assert!(ok, "scan must either find disks (with source) or find none");
    true
}
