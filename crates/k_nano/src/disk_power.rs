//! Disk Power Management — stubs seguros.
//! ATA IDLE requer acesso ao taskfile via AtaDriver (nao BlockDevice).
//! NVMe Set Features requer admin command queue via NvmeDriver.

/// Stub: ATA IDLE — implementacao segura (apenas log).
/// O comando ATA IDLE (0xE3/E2) deve ser enviado via taskfile regs, nao via write_sectors.
pub fn ata_idle(spindown_secs: u16) {
    crate::serial_println!("[DISK_POWER] ATA IDLE {}s (stub)", spindown_secs);
}

/// Stub: NVMe Set Power State — implementacao segura (apenas log).
/// Deve usar admin command Set Features (0x09) via NVMe admin queue.
pub fn nvme_set_power(ps: u8) {
    crate::serial_println!("[DISK_POWER] NVMe PS{} (stub)", ps);
}

pub fn apply_idle_policy(is_ssd: bool, idle_ticks: u64) {
    if is_ssd {
        if idle_ticks > 3600 { nvme_set_power(5); }
        else if idle_ticks > 600 { nvme_set_power(3); }
    } else {
        if idle_ticks > 3600 { ata_idle(120); }
        else if idle_ticks > 600 { ata_idle(30); }
    }
}
