//! Disk Power Management — ATA IDLE, NVMe power states, spin-down.
//! Reduz consumo em modo ocioso. Configuravel por tipo de disco.

use crate::block_dev::BlockDevice;

/// Comando ATA IDLE (0xE3) com timeout em segundos.
/// `spindown_secs = 0` significa "sem timeout" (cancela).
pub fn ata_idle(dev: &mut dyn BlockDevice, lba: u64, spindown_secs: u16) -> bool {
    // ATA IDLE comando via write_sectors para enviar comando ao barramento
    let mut cmd = [0u8; 512];
    cmd[0] = 0xE3; // IDLE
    cmd[1] = (spindown_secs & 0xFF) as u8;
    cmd[2] = (spindown_secs >> 8) as u8;
    dev.write_sectors(lba, &cmd) // lba = 0 para o comando
}

/// NVMe Power State. PS0 = max performance, PS1-PS5 = economico.
/// Valores tipicos: PS0=25W, PS3=10W, PS5=2W
pub fn nvme_set_power(dev: &mut dyn BlockDevice, _lba: u64, ps: u8) -> bool {
    // NVMe Set Features (PS = power state)
    // Comando simplificado: escreve power state no buffer
    let mut cmd = [0u8; 512];
    cmd[0] = 0x09; // Set Features
    cmd[8] = ps;
    dev.write_sectors(0, &cmd)
}

/// Aplica politica de energia baseada no tipo de disco e tempo ocioso.
/// Chamado periodicamente pelo tick do DiskIntelligenceAgent.
pub fn apply_idle_policy(is_ssd: bool, idle_ticks: u64, dev: &mut dyn BlockDevice) {
    if is_ssd {
        // SSD: NVMe PS3 apos 600 ticks (~30s), PS5 apos 3600 ticks (~3min)
        if idle_ticks > 3600 { nvme_set_power(dev, 0, 5); }
        else if idle_ticks > 600 { nvme_set_power(dev, 0, 3); }
    } else {
        // HDD: spin-down apos 3600 ticks (~3min)
        if idle_ticks > 3600 { ata_idle(dev, 0, 120); } // spin-down em 120s
        else if idle_ticks > 600 { ata_idle(dev, 0, 30); } // idle em 30s
    }
}
