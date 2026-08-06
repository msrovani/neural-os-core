//! Detecção de modo de boot — instalador vs sistema (ADR-0086 §2.6/2.7, gap I9).
//! O OS pergunta "onde estou?" no boot: pendrive (visitante/mensageiro) ou disco
//! instalado (residente). Sinal decisivo = presença de NeuralFS (0x7F) no boot
//! device (assinatura do SysInstaller); CONFIG.TXT afina live vs install.

use core::sync::atomic::{AtomicU8, Ordering};

/// Modo de boot do sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    /// Pendrive em modo live — provar o OS, sem instalar.
    Live,
    /// Pendrive em modo instalador — entregar o sistema ao silício.
    Install,
    /// Sistema instalado no disco interno (GPT dual ESP+NeuralFS) — Residente.
    Installed,
    /// Não determinado (sem ATA/config).
    Unknown,
}

static CACHED: AtomicU8 = AtomicU8::new(0); // 0 = unknown, 1=live, 2=install, 3=installed

fn to_code(m: BootMode) -> u8 {
    match m {
        BootMode::Live => 1,
        BootMode::Install => 2,
        BootMode::Installed => 3,
        BootMode::Unknown => 0,
    }
}

/// Lê CONFIG.TXT da primeira FAT32 (0x0B/0x0C/0x1C/0xEF) e extrai `KEY=value`.
/// ponytail: só procura BOOT_MODE (o único consumidor hoje).
fn boot_mode_from_config() -> Option<BootMode> {
    let ata = crate::globals::ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { crate::fat32::read_mbr(ata) };
    for part in &parts {
        if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
            let fs = unsafe { crate::fat32::Fat32Reader::new(ata, part) };
            let fs = fs?;
            let data = unsafe { fs.read_file("CONFIG.TXT") }?;
            let text = core::str::from_utf8(&data).ok()?;
            for line in text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("BOOT_MODE=") {
                    let v = rest.trim().to_ascii_lowercase();
                    return match v.as_str() {
                        "install" => Some(BootMode::Install),
                        "live" => Some(BootMode::Live),
                        _ => None,
                    };
                }
            }
            return None;
        }
    }
    None
}

/// Detecta se o boot device é um sistema instalado (GPT dual com NeuralFS 0x7F).
fn has_neuralfs_on_boot() -> bool {
    let ata = crate::globals::ATA_DRIVER.lock();
    let Some(ata) = ata.as_ref() else { return false };
    let parts = unsafe { crate::fat32::read_mbr(ata) };
    parts.iter().any(|p| p.type_code == crate::neural_fs::volume::MBR_TYPE_NEURALFS)
}

/// Determina o modo de boot (cacheado).
/// Ordem: CONFIG.TXT BOOT_MODE (instalação explícita) → NeuralFS no boot (instalado)
/// → senão pendrive live (default seguro, não-destrutivo).
pub fn boot_mode() -> BootMode {
    let cached = CACHED.load(Ordering::Relaxed);
    if cached != 0 {
        return match cached {
            1 => BootMode::Live,
            2 => BootMode::Install,
            3 => BootMode::Installed,
            _ => BootMode::Unknown,
        };
    }
    let mode = if let Some(m) = boot_mode_from_config() {
        m
    } else if has_neuralfs_on_boot() {
        BootMode::Installed
    } else {
        BootMode::Live
    };
    CACHED.store(to_code(mode), Ordering::Relaxed);
    crate::slog_nano!("BOOT", "mode", "boot_mode={:?}", mode);
    mode
}

/// Força um modo (ex: menu live/install escolheu). Teste/hook.
pub fn set_boot_mode(m: BootMode) {
    CACHED.store(to_code(m), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::{to_code, BootMode};

    #[test]
    fn mode_codes_roundtrip() {
        assert_eq!(to_code(BootMode::Live), 1);
        assert_eq!(to_code(BootMode::Install), 2);
        assert_eq!(to_code(BootMode::Installed), 3);
        assert_eq!(to_code(BootMode::Unknown), 0);
    }
}
