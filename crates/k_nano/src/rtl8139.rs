use alloc::vec::Vec;
use x86_64::instructions::port::Port;

use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
pub const RTL8139_VENDOR: u16 = 0x10EC;
pub const RTL8139_DEVICE: u16 = 0x8139;

const REG_MAC: u16 = 0x00;
const REG_TSD0: u16 = 0x10;
const REG_TSAD0: u16 = 0x20;
const REG_RBSTART: u16 = 0x30;
const REG_CR: u16 = 0x37;
const REG_CAPR: u16 = 0x38;
// CBR (0x3A) = Current Buffer Address (read-only, NIC escreve onde esta)
// CAPR (0x38) = Current Address of Packet Read (escrita = "host ja leu ate aqui")
// BUG FIX: o codigo anterior escrevia em CBR (read-only) em vez de CAPR
const REG_RCR: u16 = 0x44;
const REG_IMR: u16 = 0x3C;

const CR_RST: u8 = 0x10;
const CR_RE: u8 = 0x01;   // Receiver Enable (RX_BUF_EMPTY check bit)
const CR_TE: u8 = 0x04;   // Transmitter Enable
const CR_RXE: u8 = 0x08;  // RX Enable
const CR_TXE: u8 = 0x04;  // TX Enable (same as CR_TE)

const TSD_TOK: u32 = 0x0000_8000;
const TSD_TABT: u32 = 0x0000_2000;
const TSD_TUN: u32 = 0x0000_4000;
const TSD_SIZE_SHIFT: u32 = 0;

const TX_BUF_SIZE: usize = 4096;

// RX buffer: 8K + 16 bytes pad + 1500 bytes wrap (segundo referência)
const RX_BUF_LEN: usize = 8192;
const RX_BUF_PAD: usize = 16;
const RX_BUF_WRAP: usize = 1500;
const RX_BUF_SIZE: usize = RX_BUF_LEN + RX_BUF_PAD + RX_BUF_WRAP;

pub struct Rtl8139Driver {
    io_base: u16,
    mac_addr: [u8; 6],
    pci_bus: u8,
    pci_device: u8,
    pci_func: u8,
    tx_cur: usize,
    tx_buf_paddrs: [u64; 4],
    rx_buf_paddr: u64,
    rx_offset: u16,
    debug_count: u64,
}

impl Rtl8139Driver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.vendor_id != RTL8139_VENDOR || dev.device_id != RTL8139_DEVICE {
            return None;
        }
        let io_base = (dev.bar0 & !0x3) as u16;
        let mut mac = [0u8; 6];
        for i in 0..6 {
            mac[i] = Port::new(io_base + REG_MAC + i as u16).read();
        }
        // Enable PCI Bus Master for DMA operation (required on real HW)
        crate::pci::enable_pci_bus_master(dev);

        Some(Rtl8139Driver {
            io_base,
            mac_addr: mac,
            pci_bus: dev.bus,
            pci_device: dev.device,
            pci_func: dev.function,
            tx_cur: 0,
            tx_buf_paddrs: [0; 4],
            rx_buf_paddr: 0,
            rx_offset: 0,
            debug_count: 0,
        })
    }

    unsafe fn read8(&self, reg: u16) -> u8 {
        Port::new(self.io_base + reg).read()
    }
    unsafe fn read16(&self, reg: u16) -> u16 {
        Port::new(self.io_base + reg).read()
    }
    unsafe fn read32(&self, reg: u16) -> u32 {
        Port::new(self.io_base + reg).read()
    }
    unsafe fn write8(&self, reg: u16, val: u8) {
        Port::new(self.io_base + reg).write(val)
    }
    unsafe fn write16(&self, reg: u16, val: u16) {
        Port::new(self.io_base + reg).write(val)
    }
    unsafe fn write32(&self, reg: u16, val: u32) {
        Port::new(self.io_base + reg).write(val)
    }

    fn alloc_page() -> u64 {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let allocator = match guard.as_mut() {
            Some(a) => a,
            None => {
                crate::slog_nano!("RTL8139", "error", "GLOBAL_ALLOCATOR not initialized");
                return 0;
            }
        };
        let frame = allocator.allocate_contiguous(1);
        match frame {
            Some(f) => f.start_address().as_u64(),
            None => 0,
        }
    }

    fn alloc_pages(n: usize) -> u64 {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let allocator = match guard.as_mut() {
            Some(a) => a,
            None => {
                crate::slog_nano!("RTL8139", "error", "GLOBAL_ALLOCATOR not initialized");
                return 0;
            }
        };
        let frame = allocator.allocate_contiguous(n);
        match frame {
            Some(f) => f.start_address().as_u64(),
            None => 0,
        }
    }

    pub unsafe fn init(&mut self) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

        self.write8(REG_CR, CR_RST);
        for _ in 0..100_000 {
            if self.read8(REG_CR) & CR_RST == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        // Re-check PCI Bus Master after reset (CR_RST pode ter limpado)
        let cmd = crate::pci::read_config_word(self.pci_bus, self.pci_device, self.pci_func, 0x04);
        if cmd & 0x04 == 0 {
            crate::slog_nano!("Net", "rtl8139", "Bus Master lost after reset! Re-enabling...");
            crate::pci::enable_pci_bus_master_unsafe(self.pci_bus, self.pci_device, self.pci_func);
        }

        crate::slog_nano!("Net", "rtl8139", "Reset OK. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac_addr[0], self.mac_addr[1], self.mac_addr[2],
            self.mac_addr[3], self.mac_addr[4], self.mac_addr[5]);

        // Primeiro enable RE (Receiver MAC) + RXE (RX DMA) + TXE (TX DMA)
        // RE bit (0x01) é CRÍTICO — sem ele o receptor MAC fica desligado e rx=0 sempre
        self.write8(REG_CR, CR_RE | CR_RXE | CR_TXE);

        // Configura RCR: WRAP bit (0x80) é CRÍTICO para RX buffer funcionar corretamente
        // APM=1(PMatch) AB=1(Broadcast) MXDMA=111(unlimited) WRAP=1 RXFTH_NONE
        // 0b1110_0000_0000_0000 | 0b1000_0000 | 0b1010 | 0b1000 | 0b10
        // = RXFTH_NONE | WRAP | AB | AM | APM
        const APM: u32 = 0b10;
        const AB: u32 = 0b1000;
        const AM: u32 = 0b100;
        const WRAP: u32 = 0b1000_0000;
        const MXDMA_UNLIMITED: u32 = 0b111_0000_0000;
        const RXFTH_NONE: u32 = 0b1110_0000_0000_0000;
        let rcr_val = APM | AB | AM | WRAP | MXDMA_UNLIMITED | RXFTH_NONE;
        self.write32(REG_RCR, rcr_val);
        crate::slog_nano!("Net", "rtl8139", "RCR={:#010x} (WRAP=1 MXDMA=unlimited)", rcr_val);

        let rx_paddr = Self::alloc_pages((RX_BUF_SIZE + 4095) / 4096);
        if rx_paddr == 0 {
            crate::slog_nano!("Net", "rtl8139", "RX buffer alloc failed (size={})", RX_BUF_SIZE);
            return false;
        }
        self.rx_buf_paddr = rx_paddr;

        let rx_virt = (rx_paddr + pmoff) as *mut u8;
        for i in 0..RX_BUF_SIZE {
            rx_virt.add(i).write_volatile(0);
        }

        // Segundo enable + RBSTART (RE+RXE+TXE em todas as writes do CR)
        self.write8(REG_CR, CR_RE | CR_RXE | CR_TXE);
        self.write32(REG_RBSTART, rx_paddr as u32);
        self.write8(REG_CR, CR_RE | CR_RXE | CR_TXE);

        for i in 0..4 {
            let tx_paddr = Self::alloc_page();
            if tx_paddr == 0 {
                crate::slog_nano!("Net", "rtl8139", "TX buffer alloc failed at {}", i);
                return false;
            }
            self.tx_buf_paddrs[i] = tx_paddr;
            let tsad_reg = REG_TSAD0 + i as u16 * 4;
            self.write32(tsad_reg, tx_paddr as u32);
        }

        self.write16(REG_IMR, 0x0000);

        // CAPR inicial = RX_BUF_LEN - 16 = 8176 (datasheet RTL8139).
        // Se CAPR == CBR (ambos 0), o QEMU entende buffer cheio e descarta pacotes.
        // Com CAPR = 8176, CBR = 0, o NIC tem 8176 bytes livres para escrever.
        let capr_init = (RX_BUF_LEN as u16).wrapping_sub(16);
        self.write16(REG_CAPR, capr_init);
        self.rx_offset = 0;
        crate::slog_nano!("Net", "rtl8139", "RX init: CAPR={} RX_BUF_SIZE={}", capr_init, RX_BUF_SIZE);

        crate::slog_nano!("Net", "rtl8139", "Init OK. rx_buf=0x{:x} tx_bufs=[0x{:x},...]", self.rx_buf_paddr, self.tx_buf_paddrs[0]);
        true
    }

    pub unsafe fn send(&mut self, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > TX_BUF_SIZE {
            return false;
        }
        let idx = self.tx_cur;
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let tsd_reg = REG_TSD0 + idx as u16 * 4;

        for _ in 0..100_000 {
            let tsd = self.read32(tsd_reg);
            if tsd & (TSD_TOK | TSD_TABT | TSD_TUN) != 0 {
                break;
            }
            if tsd == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        let buf_virt = (self.tx_buf_paddrs[idx] + pmoff) as *mut u8;
        for i in 0..data.len() {
            buf_virt.add(i).write_volatile(data[i]);
        }

        let truncated_len = core::cmp::min(data.len(), u32::MAX as usize) as u32;
        self.write32(tsd_reg, truncated_len << TSD_SIZE_SHIFT);

        // Debug TX na primeira ocorrencia
        let tx_dbg = idx;
        if tx_dbg < 4 && self.tx_cur == 0 {
            let tsd_val = self.read32(tsd_reg);
            crate::slog_nano!("Net", "rtl8139", "TX{} len={} tsd={:#x} tsad={:#x}", tx_dbg, data.len(), tsd_val, self.tx_buf_paddrs[idx]);
        }

        for _ in 0..100_000 {
            let tsd = self.read32(tsd_reg);
            if tsd & (TSD_TOK | TSD_TABT | TSD_TUN) != 0 {
                self.tx_cur = (idx + 1) % 4;
                return tsd & TSD_TOK != 0;
            }
            core::hint::spin_loop();
        }
        crate::slog_nano!("Net", "rtl8139", "TX{} timeout tsd=0x{:x}", idx, self.read32(tsd_reg));
        false
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        self.debug_count += 1;
        // Verifica RX_BUF_EMPTY no CR antes de ler (padrão do driver ref)
        let cr = self.read8(REG_CR);
        if cr & CR_RE != 0 {
            return None; // Buffer empty
        }

        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let rx_virt = (self.rx_buf_paddr + pmoff) as *const u8;

        let off = self.rx_offset as usize;

        // SEGURANÇA: o buffer alocado tem RX_BUF_SIZE = RX_BUF_LEN (8192) + PAD
        // + WRAP bytes. Invariante: `off` é sempre um índice DENTRO desse buffer
        // e o header de 4 bytes lido em off..off+4 precisa caber — por isso o
        // guard é `off + 4 <= RX_BUF_SIZE`, não `off < RX_BUF_SIZE`.
        // O wrap de 1500 bytes garante que o corpo de um frame de 1514 bytes
        // iniciado perto de RX_BUF_LEN não ultrapasse o fim do buffer.
        if off + 4 > RX_BUF_SIZE {
            self.rx_offset = 0;
            return None;
        }

        // Header RX: 4 bytes (status=2, len=2)
        let status = u16::from_le_bytes([
            rx_virt.add(off).read_volatile(),
            rx_virt.add(off + 1).read_volatile(),
        ]);
        let pkt_len = u16::from_le_bytes([
            rx_virt.add(off + 2).read_volatile(),
            rx_virt.add(off + 3).read_volatile(),
        ]);

        // Frame length INCLUDES the 4-byte header
        let total_len = pkt_len as usize;

        // Debug primeira leitura (a cada 100 chamadas para evitar flooding)
        if self.rx_offset == 0 && self.debug_count % 100 == 0 {
            crate::slog_nano!("RTL8139", "RX", "first: rx_off={:#06x} status={:#06x} len={} cr={:#04x}", self.rx_offset, status, total_len, cr);
        }

        // Se status não tem bit 0 (ROK) ou len < 64 ou len inválido
        if status & 0x0001 == 0 || total_len < 64 || total_len > RX_BUF_WRAP + 14 {
            if status & 0x0001 == 0 && self.debug_count % 100 == 0 {
                crate::slog_nano!("RTL8139", "RX", "!ROK: rx_off={:#06x} status={:#06x} len={} cr={:#04x}", self.rx_offset, status, total_len, cr);
            }
            return None;
        }

        // Dados: 4 bytes header, total_len - 4 bytes dados, 4 bytes CRC (descartado)
        // Data length = total_len - 4 (header) - 4 (CRC) = total_len - 8? Não.
        // O header de 4 bytes já está incluso em total_len.
        // O frame Ethernet tem: header 4 + dados + CRC 4. total_len = header + dados + CRC.
        // Nós queremos os dados (ethernet frame) = total_len - 4 (CRC).
        let data_len = total_len.saturating_sub(4);
        if data_len < 14 || data_len > RX_BUF_WRAP {
            if self.debug_count % 100 == 0 {
                crate::slog_nano!("RTL8139", "RX", "bad data_len={} total_len={}", data_len, total_len);
            }
            return None;
        }

        let mut buf = Vec::with_capacity(data_len);
        let data_start = off + 4;
        for i in 0..data_len {
            if data_start + i >= RX_BUF_SIZE { break; }
            buf.push(rx_virt.add(data_start + i).read_volatile());
        }

        // Calcula próximo offset: alinhado a 32 bits (dwords)
        // total_len + 4 (CRC? não, total_len já inclui CRC) + 3 para alinhamento
        // O cursor usa RX_BUF_LEN porque é o tamanho do RING programado no RCR
        // — é onde o NIC dá wrap. RX_BUF_SIZE (LEN+PAD+WRAP) é só o tamanho da
        // ALOCAÇÃO (área de overflow onde o NIC pode terminar de escrever um
        // frame iniciado no fim do ring); é o bound de leitura, nunca o módulo.
        let consumed = ((total_len + 4 + 3) / 4) * 4;
        self.rx_offset = ((off + consumed) % RX_BUF_LEN) as u16;

        // Escreve CAPR: o NIC precisa do offset - 16 (segundo datasheet RTL8139)
        // CAPR = próximo offset a ler - 0x10, módulo RX_BUF_LEN
        let capr = if self.rx_offset >= 16 {
            self.rx_offset - 16
        } else {
            // Wrap around: RX_BUF_LEN + rx_offset - 16
            // Mas o buffer só tem RX_BUF_LEN bytes de dados reais
            (RX_BUF_LEN as u16).wrapping_add(self.rx_offset).wrapping_sub(16)
        };
        self.write16(REG_CAPR, capr);

        Some(buf)
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac_addr
    }

    pub fn debug_regs(&self) {
        unsafe {
            let cr = self.read8(REG_CR);
            let capr = self.read16(REG_CAPR);
            crate::slog_nano!("Net", "rtl8139", "CR=0x{:02x} CAPR=0x{:04x} rx_off=0x{:04x} tx_cur={}", cr, capr, self.rx_offset, self.tx_cur);
        }
    }
}
