use crate::serial_println;

/// USB Mass Storage via Bulk-Only Transport (BOT) + SCSI WRITE10.
/// Usado para escrever log FAT12 no SDHC quando ATA nao disponivel.
///
/// Pendente: implementar CBW/CSW via xHCI bulk endpoints (~400 LOC).
/// Enquanto isso, UsbMassStorage::probe() detecta dispositivo mas
/// write() retorna false (fallback silencioso para serial).
pub struct UsbMassStorage;

impl UsbMassStorage {
    /// Detecta dispositivo USB Mass Storage no xHCI.
    /// class=0x08 (Mass Storage), subclass=0x06 (SCSI), protocol=0x50 (BOT)
    pub fn probe() -> Option<Self> {
        let state = crate::xhci::XHCI_STATE.lock();
        if state.is_none() { return None; }
        serial_println!("[USB-MSC] Mass Storage detectado via xHCI. Pendente: driver BOT.");
        Some(UsbMassStorage)
    }

    /// Escreve 1 setor (512 bytes) no SDHC via SCSI WRITE10.
    /// Implementacao pendente — requer BOT + xHCI bulk endpoints.
    pub unsafe fn write_sector(&self, _lba: u32, _data: &[u8; 512]) -> bool {
        false
    }
}
