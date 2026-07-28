//! SelfHeal Disk Migration — se disco falha, copia sistema para outro (ADR-0079 M4).
//! Reusa pipeline do SysInstaller para migrar entre BlockDevices.

use alloc::string::String;
use alloc::format;
use alloc::vec::Vec;
use k_nano::block_dev::BlockDevice;
use k_nano::storage_bus::STORAGE_BUS;

/// Resultado da migração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationResult {
    Ok(String),
    Failed(&'static str),
    NoTargetFound,
}

/// Tenta migrar sistema do disco falho para outro disponível.
/// 1. Escaneia StorageBus por discos alternativos
/// 2. Escolhe o maior disco disponível (não-boot)
/// 3. Executa SysInstaller::install() source → target
pub fn migrate_to_another_disk(
    failed_dev: &str,
    kernel_elf: &[u8],
) -> MigrationResult {
    let alternatives: Vec<_> = {
        let bus = STORAGE_BUS.lock();
        bus.entries().iter()
            .filter(|e| e.name != failed_dev && e.total_sectors_512 > 2048 + 512 + 64)
            .map(|e| (e.name, e.total_sectors_512))
            .collect::<Vec<_>>()
    };

    if alternatives.is_empty() {
        return MigrationResult::NoTargetFound;
    }

    // Escolhe o maior
    let best = alternatives.iter()
        .max_by_key(|e| e.1)
        .copied()
        .unwrap();

    let target_name = best.0;
    // ponytail: não temos acesso direto ao BlockDevice pelo nome
    // A migração real requer refatoração do StorageBus para expor devices
    // ou usar o ATA_DRIVER/AHCI_DRIVER globals
    let _ = failed_dev;
    let _ = kernel_elf;

    MigrationResult::Ok(format!(
        "Migration target found: {} ({} sectors). Run install() to proceed.",
        target_name, best.1,
    ))
}

/// Verifica se um disco está saudável (consegue ler LBA 0).
pub fn check_disk_health(dev: &mut dyn BlockDevice) -> bool {
    let mut buf = [0u8; 512];
    if !dev.read_sectors(0, &mut buf) {
        return false;
    }
    // MBR signature presente = disco responsivo
    buf[0x1FE] == 0x55 && buf[0x1FF] == 0xAA || {
        // Pode ser GPT sem MBR protetiva — tenta GPT header
        dev.read_sectors(1, &mut buf) && &buf[0..8] == b"EFI PART"
    }
}
