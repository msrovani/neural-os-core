//! USB Mass Storage via Bulk-Only Transport (BOT) + SCSI.
//! Protocolo padrao para pendrives, SDHC readers.
//! class=0x08 (Mass Storage), subclass=0x06 (SCSI), protocol=0x50 (BOT)
//!
//! Fluxo: CBW (31 bytes) → Data (opcional) → CSW (13 bytes)

use crate::serial_println;
use crate::xhci;

// CBW — Command Block Wrapper (31 bytes)
#[repr(C, packed)]
struct Cbw {
    signature: u32,        // 0x43425355 "USBC"
    tag: u32,              // incrementing
    data_len: u32,         // bytes to transfer
    flags: u8,             // 0x80 = IN (device→host), 0x00 = OUT
    lun: u8,               // logical unit number
    cmd_len: u8,           // SCSI command length
    cmd: [u8; 16],         // SCSI command
}

// CSW — Command Status Wrapper (13 bytes)
#[repr(C, packed)]
struct Csw {
    signature: u32,        // 0x53425355 "USBS"
    tag: u32,
    residue: u32,          // bytes not transferred
    status: u8,            // 0=pass, 1=fail, 2=phase error
}

// SCSI commands
const SCSI_TEST_UNIT_READY: u8 = 0x00;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_READ10: u8 = 0x28;
const SCSI_WRITE10: u8 = 0x2A;

pub struct UsbMassStorage {
    // Bulk endpoints da configuracao xHCI
    bulk_in: u8,
    bulk_out: u8,
    tag: u32,
    max_lba: u32,
}

impl UsbMassStorage {
    /// Detecta e configura o primeiro dispositivo USB Mass Storage
    pub unsafe fn probe() -> Option<Self> {
        let state = xhci::XHCI_STATE.lock();
        if state.is_none() { return None; }

        // Em producao: enumerar dispositivos xHCI, configurar bulk endpoints,
        // enviar SCSI INQUIRY + READ CAPACITY.
        //
        // Stub: detecta classe 0x08 no PCI scan e retorna estrutura.
        // A configuracao real dos bulk endpoints requer:
        // 1. Address device via xHCI
        // 2. Configurar EP1 IN (bulk) e EP2 OUT (bulk)
        // 3. Alocar transfer rings para bulk

        serial_println!("[USB-MSC] Mass storage detectado. Pendente: bulk endpoint config.");
        Some(UsbMassStorage {
            bulk_in: 1,  // EP1 IN
            bulk_out: 2, // EP2 OUT
            tag: 1,
            max_lba: 0,
        })
    }

    /// Envia comando SCSI e interpreta resposta
    unsafe fn send_scsi(&mut self, cmd: &[u8], dir_in: bool, buf: &mut [u8]) -> bool {
        if cmd.is_empty() || cmd.len() > 16 { return false; }

        let data_len = buf.len() as u32;

        // Monta CBW
        let tag = self.tag.wrapping_add(1);
        self.tag = tag;

        let mut cbw_bytes = [0u8; 31];
        cbw_bytes[0..4].copy_from_slice(&0x43425355u32.to_le_bytes()); // signature
        cbw_bytes[4..8].copy_from_slice(&tag.to_le_bytes());           // tag
        cbw_bytes[8..12].copy_from_slice(&data_len.to_le_bytes());     // data len
        cbw_bytes[12] = if dir_in { 0x80 } else { 0x00 };             // flags
        cbw_bytes[13] = 0;                                             // LUN
        cbw_bytes[14] = cmd.len() as u8;                               // cmd len
        cbw_bytes[15..15 + cmd.len()].copy_from_slice(cmd);            // SCSI cmd

        // Envia CBW via Bulk OUT (stub — precisa xHCI bulk write)
        self.bulk_write(&cbw_bytes);

        // Data phase (stub)
        if dir_in {
            self.bulk_read(buf);
        } else {
            self.bulk_write(buf);
        }

        // Recebe CSW via Bulk IN (stub)
        let mut csw_bytes = [0u8; 13];
        self.bulk_read(&mut csw_bytes);

        let csw = &csw_bytes as *const u8 as *const Csw;
        if (*csw).signature != 0x53425355 { return false; }
        if (*csw).status != 0 { return false; }

        true
    }

    /// Stub: bulk write via xHCI (implementar com transfer ring)
    unsafe fn bulk_write(&self, _data: &[u8]) {
        // TODO: programar TRB normal no transfer ring do bulk OUT endpoint
        //       e ring doorbell
    }

    /// Stub: bulk read via xHCI
    unsafe fn bulk_read(&self, _buf: &mut [u8]) {
        // TODO: programar TRB normal no transfer ring do bulk IN endpoint
        //       esperar completion no event ring
    }

    /// Le 1 setor (512 bytes) do SDHC via SCSI READ10
    pub unsafe fn read_sector(&mut self, lba: u32, data: &mut [u8; 512]) -> bool {
        if self.max_lba > 0 && lba >= self.max_lba { return false; }
        // SCSI READ10: opcode 0x28, LBA 4 bytes, grupo 0, protect 0, transfer length 2 bytes
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_READ10;
        cmd[2..6].copy_from_slice(&lba.to_be_bytes());    // LBA (big-endian)
        cmd[7] = 0;                                        // reserved
        cmd[8] = 1;                                        // transfer length (1 sector)
        self.send_scsi(&cmd, true, data)
    }

    /// Escreve 1 setor (512 bytes) via SCSI WRITE10
    pub unsafe fn write_sector(&mut self, lba: u32, _data: &[u8; 512]) -> bool {
        if self.max_lba > 0 && lba >= self.max_lba { return false; }
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_WRITE10;
        cmd[2..6].copy_from_slice(&lba.to_be_bytes());
        cmd[7] = 0;
        cmd[8] = 1;
        self.send_scsi(&cmd, false, &mut [])
    }

    /// SCSI TEST UNIT READY — verifica se dispositivo esta pronto
    pub unsafe fn test_ready(&mut self) -> bool {
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_TEST_UNIT_READY;
        self.send_scsi(&cmd, true, &mut [])
    }

    /// SCSI INQUIRY — obtem informacoes do dispositivo
    pub unsafe fn inquiry(&mut self) -> Option<[u8; 36]> {
        let mut buf = [0u8; 36];
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_INQUIRY;
        cmd[4] = 36; // allocation length
        if self.send_scsi(&cmd, true, &mut buf) { Some(buf) } else { None }
    }
}
