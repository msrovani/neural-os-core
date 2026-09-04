//! AtaAgent — acesso a blocos ATA como arquivos.
//! Mount: /mnt/hdd/
//! Arquivos: /mnt/hdd/sda (disco inteiro), /mnt/hdd/sda1 (particao)

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::fs::FilesystemAgent;
use crate::ata::AtaDriver;
pub struct AtaAgent {
    ata: Option<AtaDriver>,
    ata_inited: bool,
}

impl AtaAgent {
    /// Não re-probe no boot: `AtaDriver::probe()` no metal (portas mortas / HD
    /// interno) trava em PIO e congela em `BOOT: VFS+FS...` (init_fs_agents).
    /// Reusa `ATA_DRIVER` global; live USB sem MSC → agent vazio (honesty).
    pub fn new() -> Self {
        if crate::boot_logger::internal_disk_skipped() {
            crate::slog_nano!(
                "ATA",
                "ok",
                "AtaAgent skip probe (live USB — sem I/O HD interno)"
            );
            return AtaAgent {
                ata: None,
                ata_inited: false,
            };
        }
        if let Some(existing) = crate::ATA_DRIVER.lock().as_ref() {
            crate::slog_nano!("ATA", "ok", "AtaAgent reuse ATA_DRIVER (sem re-probe)");
            return AtaAgent {
                ata: Some(existing.clone()),
                ata_inited: true,
            };
        }
        let ata = unsafe { AtaDriver::probe() };
        let inited = ata.is_some();
        if inited {
            crate::slog_nano!("ATA", "ok", "ATA detectado. /mnt/hdd/ disponivel.");
        }
        AtaAgent {
            ata,
            ata_inited: inited,
        }
    }
}

impl FilesystemAgent for AtaAgent {
    fn name(&self) -> &str { "ata" }
    fn mount_point(&self) -> &str { "/mnt/hdd" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let ata = self.ata.as_ref().ok_or("ATA nao disponivel")?;
        match path.trim_matches('/') {
            "sda" | "sda/raw" => {
                // Le os primeiros 4KB do disco (MBR + teste)
                let mut buf = [0u8; 512 * 8];
                if unsafe { ata.read_sectors(0, &mut buf, 8) } {
                    Ok(buf.to_vec())
                } else {
                    Err("Falha ao ler setor 0")
                }
            }
            "sda1" => {
                // Le primeiro setor da particao 1 via MBR
                let mbr_lba = 0u32;
                let mut mbr = [0u8; 512];
                if !unsafe { ata.read_sectors(mbr_lba, &mut mbr, 1) } { return Err("Falha ao ler MBR"); }
                if mbr[510] == 0x55 && mbr[511] == 0xAA {
                    let p1_lba = u32::from_le_bytes([mbr[0x1BE+8], mbr[0x1BE+9], mbr[0x1BE+10], mbr[0x1BE+11]]);
                    if p1_lba > 0 {
                        let mut buf = [0u8; 512];
                        if unsafe { ata.read_sectors(p1_lba, &mut buf, 1) } { return Ok(buf.to_vec()); }
                    }
                }
                Err("Particao 1 nao encontrada")
            }
            "info" => {
                let info = alloc::format!("ATA disk. IO base: {:#06x}\n", 
                    self.ata.as_ref().map_or(0, |a| a.io_base));
                Ok(info.into_bytes())
            }
            _ => Err("Arquivo nao encontrado em /mnt/hdd/"),
        }
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> {
        let ata = self.ata.as_mut().ok_or("ATA nao disponivel")?;
        match path.trim_matches('/') {
            "sda" | "sda/raw" => {
                let lba = 0u32;
                let count = (data.len() + 511) / 512;
                let count = core::cmp::min(count, 8) as u8;
                if unsafe { ata.write_sectors(lba, data, count) } {
                    Ok(())
                } else {
                    Err("Falha ao escrever")
                }
            }
            _ => Err("Arquivo nao encontrado em /mnt/hdd/"),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        match path.trim_matches('/') {
            "" => Ok(vec![
                String::from("sda"), String::from("sda1"), String::from("info"),
            ]),
            "sda" | "sda/" => Ok(vec![
                String::from("raw"), String::from("info"),
            ]),
            _ => Err("Diretorio nao encontrado"),
        }
    }
}
