//! AtaAgent — acesso a blocos ATA como arquivos.
//! Mount: /mnt/hdd/
//! Arquivos: /mnt/hdd/sda (disco inteiro), /mnt/hdd/sda1 (particao)

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::fs::FilesystemAgent;
use crate::ata::AtaDriver;
use crate::serial_println;

pub struct AtaAgent {
    ata: Option<AtaDriver>,
    ata_inited: bool,
}

impl AtaAgent {
    pub fn new() -> Self {
        let ata = unsafe { AtaDriver::probe() };
        let inited = ata.is_some();
        if inited {
            serial_println!("[ATA-FS] ATA detectado. /mnt/hdd/ disponivel.");
        }
        AtaAgent { ata, ata_inited: inited }
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
                let mut buf = [0u8; 512];
                if unsafe { ata.read_sectors(63, &mut buf, 1) } {
                    Ok(buf.to_vec())
                } else {
                    Err("Falha ao ler setor 63")
                }
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
            "" | "sda" | "sda/" => Ok(vec![
                String::from("raw"), String::from("info"),
            ]),
            "" => Ok(vec![
                String::from("sda"), String::from("sda1"), String::from("info"),
            ]),
            _ => Err("Diretorio nao encontrado"),
        }
    }
}
