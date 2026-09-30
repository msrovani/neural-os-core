//! Realtek RTL8168/8111 PCIe GbE ("Realtek PCIe GbE Family Controller") — polled.
//!
//! Família Linux `r8169`. Diferente do RTL8139 (register file I/O-port totalmente
//! outro): MMIO em BAR2 (64-bit, ~4 KiB), descritores de 16 bytes (256 por anel,
//! 256-byte alinhados), TxPoll NPQ para kickar TX.
//!
//! Fonte de referência: Linux `drivers/net/ethernet/realtek/r8169_main.c` +
//! u-boot `drivers/net/rtl8169.c`. no_std, polled only, timeouts limitados.
//! Firmware NÃO é necessário para bring-up (o r8169 só carrega firmware para
//! funções avançadas) — pulado de propósito.
//!
//! `classify_nic` (boot_bind) devolvia `NicKind::None` para 10EC:8168 → n=0 →
//! nenhum driver tocado. Este módulo fecha esse buraco.

use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;

use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;

pub const RTL8168_VENDOR: u16 = 0x10EC;

/// GbE family PCI IDs que usam MMIO em BAR2. u-boot `rtl8169.c` seleciona
/// `region = 2` apenas para 0x8125/0x8161/0x8168; 0x8162/0x8167 são partes PCI
/// que usam BAR1 — mapear/escrever BAR2 nelas é escrita em registrador errado.
/// NÃO inclui 0x8125 (2.5G, outro mapa) nem 0x8136 (10/100 8101E).
pub fn is_rtl8168_family(vendor: u16, device: u16) -> bool {
    vendor == RTL8168_VENDOR && matches!(device, 0x8161 | 0x8168)
}

/// Log-once de link down: o netstack re-tenta por frame — não spammar o serial.
static TX_LINK_DOWN_LOGGED: AtomicBool = AtomicBool::new(false);

// ── Register offsets (MMIO, BAR2) ──────────────────────────────────────────
const REG_IDR0: u64 = 0x00; // MAC[0..5] em 0x00..0x05 (byte i = mac[i], sem swap)
const REG_MAR0: u64 = 0x08; // Multicast filter (0x08..0x0F)
const REG_TNPDS: u64 = 0x20; // TX desc base low
const REG_TNPDS_HI: u64 = 0x24; // TX desc base high
const REG_CMD: u64 = 0x37; // Command
const REG_TXPOLL: u64 = 0x38; // TxPoll (NPQ)
const REG_IMR: u64 = 0x3C; // Interrupt Mask
const REG_TXCFG: u64 = 0x40; // Transmit Config
const REG_RXCFG: u64 = 0x44; // Receive Config
const REG_CFG9346: u64 = 0x50; // Cfg9346 (config lock)
const REG_CONFIG2: u64 = 0x53;
const REG_CONFIG3: u64 = 0x54;
const REG_CONFIG5: u64 = 0x56;
const REG_PHYAR: u64 = 0x60; // PHY Access
const REG_PHYSTATUS: u64 = 0x6C; // PHY Status
const REG_RXMAXSIZE: u64 = 0xDA; // RxMaxSize
const REG_CPLUSCMD: u64 = 0xE0; // C+ Command
const REG_RDSAR: u64 = 0xE4; // RX desc base low
const REG_RDSAR_HI: u64 = 0xE8; // RX desc base high
const REG_MTPS: u64 = 0xEC; // MaxTxPacketSize (mesmo offset do EarlyTxThres 8169)
const REG_RXDV_GATED: u64 = 0xF0; // RXDV gate (8168E+)

// Command bits
const CMD_RESET: u8 = 0x10;
const CMD_TX_ENB: u8 = 0x04;
const CMD_RX_ENB: u8 = 0x08;
const CMD_TXPOLL_NPQ: u8 = 0x40;

// Descriptor layout: 16 bytes LE { opts1: u32, opts2: u32, addr: u64 }
const DESC_SIZE: usize = 16;
const DESC_COUNT: usize = 256;
const DESC_ALIGN: usize = 256;
const RX_BUF_SIZE: usize = 16 * 1024;
const RX_BUF_PAGES: usize = RX_BUF_SIZE / 4096;

// opts1 bits (TX e RX compartilham OWN/FS/LS/EOR)
const DESC_OWN: u32 = 1 << 31;
const DESC_EOR: u32 = 1 << 30;
const DESC_FS: u32 = 1 << 29;
const DESC_LS: u32 = 1 << 28;
const TX_LEN_MASK: u32 = 0xFFFF; // TX len bits 15:0
const RX_LEN_MASK: u32 = 0x3FFF; // RX len bits 13:0
const RX_RES: u32 = 1 << 21; // RX error summary

// ── Pure logic (host-testável) ─────────────────────────────────────────────

/// Decodifica a versão de chip do registrador TxConfig (u-boot rtl8169.c):
/// `ver = ((v & 0x7c000000) + ((v & 0x00800000) << 2)) >> 24`.
pub fn decode_chip_version(txcfg: u32) -> u32 {
    ((txcfg & 0x7c000000) + ((txcfg & 0x00800000) << 2)) >> 24
}

/// Versões "evl ou mais novas" (8168E-vl/8168G/EP/H) que usam AUTO_FIFO e
/// MaxTxPacketSize=0x27 e limpam o gate RXDV.
fn is_evl_or_newer(ver: u32) -> bool {
    matches!(ver, 0x2e | 0x4c | 0x50 | 0x54)
}

/// Base do RxConfig por versão (r8169 `rtl_set_rx_config`). Valor de 32 bits —
/// Linux sempre escreve RxConfig com `RTL_W32`.
fn rx_config_base(ver: u32) -> u32 {
    match ver {
        0x30 | 0x38 => 0xE700,        // 8168B
        0x2e | 0x3c | 0x3d => 0xC700, // 8168E-vl (VER_34) / 8168C / CP
        0x4c | 0x50 | 0x54 => 0xCF00, // 8168G / EP / H
        // 0x28 (8168D) não tem entrada explícita no Linux — cai no default
        // RX_DMA_BURST (0x8700). Mantido até verificação em fonte primária.
        _ => 0x8700,
    }
}

/// Pack de `opts1` para um frame TX (len = bytes após padding).
pub fn tx_opts1(len: u16, ring_end: bool) -> u32 {
    let mut o = DESC_OWN | DESC_FS | DESC_LS | (len as u32 & TX_LEN_MASK);
    if ring_end {
        o |= DESC_EOR;
    }
    o
}

/// Pack de `opts1` para (re)armar um descritor RX.
pub fn rx_opts1(ring_end: bool) -> u32 {
    let mut o = DESC_OWN | RX_LEN_MASK;
    if ring_end {
        o |= DESC_EOR;
    }
    o
}

/// Bytes de payload de um descritor RX concluído (desconta os 4 bytes de FCS).
pub fn rx_payload_len(opts1: u32) -> usize {
    ((opts1 & RX_LEN_MASK) as usize).saturating_sub(4)
}

pub struct Rtl8168Driver {
    mmio_virt: u64,
    mac_addr: [u8; 6],
    chip_ver: u32,
    pci_bus: u8,
    pci_device: u8,
    pci_func: u8,
    tx_ring_paddr: u64,
    rx_ring_paddr: u64,
    tx_buf_paddrs: [u64; DESC_COUNT],
    rx_buf_paddrs: [u64; DESC_COUNT],
    tx_cur: usize,
    rx_cur: usize,
}

impl Rtl8168Driver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if !is_rtl8168_family(dev.vendor_id, dev.device_id) {
            return None;
        }
        // 8168/8111 usam BAR2 (64-bit, ~4 KiB). 8169 usava BAR1 — não é a família GbE.
        let mmio_base = dev.bar2 & !0xF;
        if mmio_base == 0 {
            crate::slog_nano!("Net", "warn", "rtl8168 BAR2 ausente — skip");
            return None;
        }
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let bar_sz = crate::pci::read_bar_size(dev.bus, dev.device, dev.function, 2);
        crate::slog_nano!(
            "Net",
            "rtl8168",
            "detectado {:02x}:{:02x}.{:x} DID={:#06x} BAR2={:#x} size={}",
            dev.bus,
            dev.device,
            dev.function,
            dev.device_id,
            mmio_base,
            bar_sz
        );
        // MMIO = 4 KiB (registradores 0x00..0xF0). Mapear a página via helper
        // CRIADOR (map_page_uc), nunca flags-only (set_page_uc) — #PF conhecido.
        crate::apic::map_page_uc(mmio_base, pmoff);
        // Bus Master + Memory Space + I/O Space antes de tocar MMIO.
        crate::pci::enable_pci_bus_master(dev);

        Some(Rtl8168Driver {
            mmio_virt: mmio_base + pmoff,
            mac_addr: [0; 6],
            chip_ver: 0,
            pci_bus: dev.bus,
            pci_device: dev.device,
            pci_func: dev.function,
            tx_ring_paddr: 0,
            rx_ring_paddr: 0,
            tx_buf_paddrs: [0; DESC_COUNT],
            rx_buf_paddrs: [0; DESC_COUNT],
            tx_cur: 0,
            rx_cur: 0,
        })
    }

    unsafe fn r8(&self, reg: u64) -> u8 {
        core::ptr::read_volatile((self.mmio_virt + reg) as *const u8)
    }
    unsafe fn r16(&self, reg: u64) -> u16 {
        core::ptr::read_volatile((self.mmio_virt + reg) as *const u16)
    }
    unsafe fn r32(&self, reg: u64) -> u32 {
        core::ptr::read_volatile((self.mmio_virt + reg) as *const u32)
    }
    unsafe fn w8(&self, reg: u64, v: u8) {
        core::ptr::write_volatile((self.mmio_virt + reg) as *mut u8, v);
    }
    unsafe fn w16(&self, reg: u64, v: u16) {
        core::ptr::write_volatile((self.mmio_virt + reg) as *mut u16, v);
    }
    unsafe fn w32(&self, reg: u64, v: u32) {
        core::ptr::write_volatile((self.mmio_virt + reg) as *mut u32, v);
    }

    fn alloc_pages(n: usize) -> u64 {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let alloc = match guard.as_mut() {
            Some(a) => a,
            None => {
                crate::slog_nano!("Net", "error", "rtl8168 GLOBAL_ALLOCATOR not initialized");
                return 0;
            }
        };
        match alloc.allocate_contiguous(n) {
            Some(f) => f.start_address().as_u64(),
            None => 0,
        }
    }

    /// Poll `(read_volatile(ptr) & mask) == want`, limitado por TSC (ou spin count).
    unsafe fn poll_u32(ptr: *const u32, mask: u32, want: u32, timeout_us: u64, spins: u32) -> bool {
        if crate::tsc::tsc_hz() != 0 {
            let t0 = crate::tsc::now_us();
            loop {
                if (core::ptr::read_volatile(ptr) & mask) == want {
                    return true;
                }
                if crate::tsc::now_us().saturating_sub(t0) > timeout_us {
                    return false;
                }
                core::hint::spin_loop();
            }
        } else {
            for _ in 0..spins {
                if (core::ptr::read_volatile(ptr) & mask) == want {
                    return true;
                }
                core::hint::spin_loop();
            }
            false
        }
    }

    unsafe fn fence() {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    pub unsafe fn init(&mut self) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

        // 1. Reset: W8(0x37, CmdReset) + poll com timeout ~100×100µs.
        self.w8(REG_CMD, CMD_RESET);
        let mut rst_ok = false;
        for _ in 0..100 {
            if self.r8(REG_CMD) & CMD_RESET == 0 {
                rst_ok = true;
                break;
            }
            crate::tsc::sleep_us(100);
        }
        if !rst_ok {
            crate::slog_nano!("Net", "warn", "rtl8168 reset TIMEOUT — CmdReset stuck");
            return false;
        }
        // CmdReset pode ter limpado Bus Master.
        let cmd = crate::pci::read_config_word(self.pci_bus, self.pci_device, self.pci_func, 0x04);
        if cmd & 0x04 == 0 {
            crate::pci::enable_pci_bus_master_unsafe(self.pci_bus, self.pci_device, self.pci_func);
        }

        // 2. MAC (IDR0..5, byte i = mac[i]).
        for i in 0..6 {
            self.mac_addr[i] = self.r8(REG_IDR0 + i as u64);
        }

        // 3. Chip version (ler TxConfig ANTES de sobrescrevê-lo).
        self.chip_ver = decode_chip_version(self.r32(REG_TXCFG));
        crate::slog_nano!(
            "Net",
            "rtl8168",
            "reset OK MAC={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} ver={:#04x}",
            self.mac_addr[0],
            self.mac_addr[1],
            self.mac_addr[2],
            self.mac_addr[3],
            self.mac_addr[4],
            self.mac_addr[5],
            self.chip_ver
        );

        // 4. Alocar anéis (256 desc, 256-byte alinhado = 1 página) + buffers.
        if !self.alloc_rings(pmoff) {
            return false;
        }

        // 5. rtl_hw_start (ordem r8169_main.c).
        self.w8(REG_CFG9346, 0xC0); // unlock config

        // clear ASPM / clock-request
        self.w8(REG_CONFIG5, self.r8(REG_CONFIG5) & !1);
        self.w8(REG_CONFIG2, self.r8(REG_CONFIG2) & !0x80);
        self.w8(REG_CONFIG3, self.r8(REG_CONFIG3) & !1);

        // CPlusCmd: manter só bits de controle (NÃO setar o bit 14 do 8169)
        self.w16(REG_CPLUSCMD, self.r16(REG_CPLUSCMD) & 0x003F);

        self.w16(REG_RXMAXSIZE, 0x4000); // RxMaxSize

        // desc addr: High ANTES de Low (contrato do 8168)
        self.w32(REG_TNPDS_HI, (self.tx_ring_paddr >> 32) as u32);
        self.w32(REG_TNPDS, self.tx_ring_paddr as u32);
        self.w32(REG_RDSAR_HI, (self.rx_ring_paddr >> 32) as u32);
        self.w32(REG_RDSAR, self.rx_ring_paddr as u32);

        self.w8(REG_CFG9346, 0x00); // lock

        // habilitar TX/RX
        self.w8(REG_CMD, CMD_TX_ENB | CMD_RX_ENB);

        // RxConfig: base por versão + AcceptBroadcast|AcceptMulticast|AcceptMyPhys
        // (MANDATÓRIO — mesh UDP broadcast/ARP dependem disso). W32: Linux sempre
        // escreve RxConfig 32-bit; um write de 16 bits deixaria bits 31:16 no reset.
        let rxcfg = rx_config_base(self.chip_ver) | 0x08 | 0x04 | 0x02;
        self.w32(REG_RXCFG, rxcfg);

        // MAR0: aceitar multicast/broadcast completo
        self.w32(REG_MAR0, 0xFFFF_FFFF);
        self.w32(REG_MAR0 + 4, 0xFFFF_FFFF);

        // TxConfig: IFG (7<<8) | DMA burst (3<<24) [+ AUTO_FIFO em evl+]
        let mut txcfg = (7u32 << 8) | (3u32 << 24);
        if is_evl_or_newer(self.chip_ver) {
            txcfg |= 1 << 7;
        }
        self.w32(REG_TXCFG, txcfg);

        // MaxTxPacketSize (mesmo offset do EarlyTxThres do 8169 — semântica outra)
        self.w8(REG_MTPS, if is_evl_or_newer(self.chip_ver) { 0x27 } else { 0x3F });

        // limpar gate RXDV em 8168E+
        if is_evl_or_newer(self.chip_ver) {
            let v = self.r32(REG_RXDV_GATED);
            self.w32(REG_RXDV_GATED, v & !(1 << 19));
        }

        // polled: mascarar todas as interrupções
        self.w16(REG_IMR, 0);
        Self::fence();

        // Link: se ficou down, tenta autoneg (não-fatal). PHYAR (0x60) só alcança
        // o PHY interno para chip_ver < 0x4c (até 8168E); 8168G/EP/H usam OCP
        // (0xb0/0xb4) — sem caminho OCP o autoneg não roda e o link fica down.
        let link = self.r8(REG_PHYSTATUS) & 0x02 != 0;
        if !link {
            if self.chip_ver < 0x4c {
                self.start_autoneg();
            } else {
                crate::slog_nano!(
                    "Net",
                    "warn",
                    "rtl8168 link down ver={:#04x} >= 0x4c: autoneg OCP (0xb0/0xb4) NAO implementado — link fica down",
                    self.chip_ver
                );
            }
        }
        crate::slog_nano!(
            "Net",
            "rtl8168",
            "init OK link={} speed={}Mbps tx_ring={:#x} rx_ring={:#x}",
            if self.r8(REG_PHYSTATUS) & 0x02 != 0 { 1 } else { 0 },
            self.link_speed_mbps(),
            self.tx_ring_paddr,
            self.rx_ring_paddr
        );
        true
    }

    unsafe fn alloc_rings(&mut self, pmoff: u64) -> bool {
        // TX ring (1 página = 256 × 16B, 256-byte alinhado).
        let tx_ring = Self::alloc_pages(1);
        if tx_ring == 0 || tx_ring & (DESC_ALIGN as u64 - 1) != 0 {
            crate::slog_nano!("Net", "error", "rtl8168 TX ring alloc fail");
            return false;
        }
        self.tx_ring_paddr = tx_ring;
        crate::apic::map_page_uc(tx_ring, pmoff);

        let rx_ring = Self::alloc_pages(1);
        if rx_ring == 0 || rx_ring & (DESC_ALIGN as u64 - 1) != 0 {
            crate::slog_nano!("Net", "error", "rtl8168 RX ring alloc fail");
            return false;
        }
        self.rx_ring_paddr = rx_ring;
        crate::apic::map_page_uc(rx_ring, pmoff);

        // TX buffers (1 página cada) + descritores.
        let tx_virt = (tx_ring + pmoff) as *mut u8;
        for i in 0..DESC_COUNT {
            let b = Self::alloc_pages(1);
            if b == 0 {
                crate::slog_nano!("Net", "error", "rtl8168 TX buf alloc fail @{}", i);
                return false;
            }
            self.tx_buf_paddrs[i] = b;
            crate::apic::map_page_uc(b, pmoff);
            let d = tx_virt.add(i * DESC_SIZE);
            core::ptr::write_volatile(d as *mut u32, 0); // opts1
            core::ptr::write_volatile(d.add(4) as *mut u32, 0); // opts2
            core::ptr::write_volatile(d.add(8) as *mut u64, b); // addr
        }

        // RX buffers (16 KiB cada) + descritores já armados com OWN.
        let rx_virt = (rx_ring + pmoff) as *mut u8;
        for i in 0..DESC_COUNT {
            let b = Self::alloc_pages(RX_BUF_PAGES);
            if b == 0 {
                crate::slog_nano!("Net", "error", "rtl8168 RX buf alloc fail @{}", i);
                return false;
            }
            self.rx_buf_paddrs[i] = b;
            for p in 0..RX_BUF_PAGES {
                crate::apic::map_page_uc(b + (p as u64) * 0x1000, pmoff);
            }
            let d = rx_virt.add(i * DESC_SIZE);
            core::ptr::write_volatile(d.add(4) as *mut u32, 0); // opts2
            core::ptr::write_volatile(d.add(8) as *mut u64, b); // addr
            core::ptr::write_volatile(d as *mut u32, rx_opts1(i == DESC_COUNT - 1));
        }
        Self::fence();
        true
    }

    pub unsafe fn send(&mut self, data: &[u8]) -> bool {
        // Frames Ethernet com FCS: máx 1514 + 4 = 1518; aceitar até 1536.
        if data.is_empty() || data.len() > 1536 {
            return false;
        }
        // Link down: com o link caído o descritor nunca completa → send() ficaria
        // ~50 ms preso por frame e o chamador (netstack::nic_send) descarta o bool.
        // Retornar cedo; logar UMA vez (o retry loop do netstack não deve inundar).
        if !self.link_up() {
            if !TX_LINK_DOWN_LOGGED.swap(true, core::sync::atomic::Ordering::Relaxed) {
                crate::slog_nano!("Net", "warn", "rtl8168 link down — TX descartado (log unico)");
            }
            return false;
        }
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        if pmoff == 0 {
            return false;
        }
        let idx = self.tx_cur;
        let base = (self.tx_ring_paddr + pmoff) as *mut u8;
        let d = base.add(idx * DESC_SIZE);

        // Esperar o descritor liberar (OWN limpo) — bounded.
        if !Self::poll_u32(d as *const u32, DESC_OWN, 0, 50_000, 200_000) {
            crate::slog_nano!("Net", "warn", "rtl8168 TX{} busy (OWN stuck)", idx);
            return false;
        }

        // Copiar + pad até 60 bytes (mínimo Ethernet).
        let n = data.len().max(60);
        let buf = (self.tx_buf_paddrs[idx] + pmoff) as *mut u8;
        for i in 0..data.len() {
            core::ptr::write_volatile(buf.add(i), data[i]);
        }
        for i in data.len()..n {
            core::ptr::write_volatile(buf.add(i), 0);
        }

        let ring_end = idx == DESC_COUNT - 1;
        let opts1 = tx_opts1(n as u16, ring_end);
        core::ptr::write_volatile(d.add(4) as *mut u32, 0); // opts2
        core::ptr::write_volatile(d.add(8) as *mut u64, self.tx_buf_paddrs[idx]);
        Self::fence();
        // publicar OWN por último
        core::ptr::write_volatile(d as *mut u32, opts1);
        Self::fence();

        // Kick obrigatório (escrever o descritor sozinho não dispara TX).
        self.w8(REG_TXPOLL, CMD_TXPOLL_NPQ);

        if !Self::poll_u32(d as *const u32, DESC_OWN, 0, 50_000, 200_000) {
            crate::slog_nano!("Net", "warn", "rtl8168 TX{} timeout opts1={:#010x}", idx, self.r32(REG_TXCFG));
            return false;
        }
        self.tx_cur = (idx + 1) % DESC_COUNT;
        true
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        if pmoff == 0 {
            return None;
        }
        let idx = self.rx_cur;
        let base = (self.rx_ring_paddr + pmoff) as *mut u8;
        let d = base.add(idx * DESC_SIZE);

        let opts1 = core::ptr::read_volatile(d as *const u32);
        if opts1 & DESC_OWN != 0 {
            return None; // NIC ainda dono
        }

        let ring_end = idx == DESC_COUNT - 1;
        let len = rx_payload_len(opts1);
        let valid = opts1 & RX_RES == 0 && len >= 14 && len <= RX_BUF_SIZE;

        // Copiar os dados ANTES de devolver o descritor ao NIC — re-armar antes
        // deixaria a NIC DMA um frame novo no mesmo buffer durante o memcpy.
        let pkt = if valid {
            let data = (self.rx_buf_paddrs[idx] + pmoff) as *const u8;
            let mut buf = Vec::with_capacity(len);
            for i in 0..len {
                buf.push(core::ptr::read_volatile(data.add(i)));
            }
            Some(buf)
        } else {
            None
        };

        // Re-armar (OWN por último) e avançar o cursor.
        core::ptr::write_volatile(d.add(4) as *mut u32, 0);
        core::ptr::write_volatile(d.add(8) as *mut u64, self.rx_buf_paddrs[idx]);
        Self::fence();
        core::ptr::write_volatile(d as *mut u32, rx_opts1(ring_end));
        Self::fence();
        self.rx_cur = (idx + 1) % DESC_COUNT;

        pkt
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac_addr
    }

    pub fn chip_version(&self) -> u32 {
        self.chip_ver
    }

    /// PHYstatus 0x6C: 0x02 = link, 0x01 = full-dup, 0x10=1000M/0x08=100M/0x04=10M.
    pub unsafe fn link_up(&self) -> bool {
        self.r8(REG_PHYSTATUS) & 0x02 != 0
    }

    pub unsafe fn link_speed_mbps(&self) -> u16 {
        let s = self.r8(REG_PHYSTATUS);
        if s & 0x10 != 0 {
            1000
        } else if s & 0x08 != 0 {
            100
        } else if s & 0x04 != 0 {
            10
        } else {
            0
        }
    }

    /// PHY write via PHYAR (0x60): flag 0x80000000 = busy.
    // ponytail: PHYAR só alcança o PHY interno para chip_ver < 0x4c (até 8168E);
    // 8168G/EP/H (chip_ver >= 0x4c) usam OCP (0xb0/0xb4) — não implementado no
    // primeiro corte. `init()` só chama `start_autoneg()` quando `chip_ver < 0x4c`
    // e emite `warn` quando `>= 0x4c` com link down (visível, não silencioso).
    // Migrar para OCP se esses chips precisarem de autoneg em HW real.
    unsafe fn phy_write(&self, reg: u8, val: u16) -> bool {
        self.w32(
            REG_PHYAR,
            0x8000_0000 | ((reg as u32 & 0x1f) << 16) | (val as u32 & 0xffff),
        );
        Self::poll_u32(
            (self.mmio_virt + REG_PHYAR) as *const u32,
            0x8000_0000,
            0,
            1_000,
            10_000,
        )
    }

    unsafe fn phy_read(&self, reg: u8) -> Option<u16> {
        self.w32(REG_PHYAR, (reg as u32 & 0x1f) << 16);
        if Self::poll_u32(
            (self.mmio_virt + REG_PHYAR) as *const u32,
            0x8000_0000,
            0x8000_0000,
            1_000,
            10_000,
        ) {
            Some((self.r32(REG_PHYAR) & 0xffff) as u16)
        } else {
            None
        }
    }

    /// Reinicia autonegociação (ANAR=0x01e1, reg9=0x0300, BMCR=AN_ENABLE|RESTART_AN).
    unsafe fn start_autoneg(&self) {
        self.phy_write(4, 0x01e1); // ANAR: 10/100 half+full
        self.phy_write(9, 0x0300); // 1000BASE-T control: advertise
        self.phy_write(0, 0x1000 | 0x0200); // BMCR: AN_ENABLE | RESTART_AN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_version_decode() {
        // Vetores derivados da fórmula u-boot (v & 0x7c000000) + ((v & 0x00800000) << 2).
        assert_eq!(decode_chip_version(0x5400_0000), 0x54); // 8168H
        assert_eq!(decode_chip_version(0x4c00_0000), 0x4c); // 8168G
        assert_eq!(decode_chip_version(0x3c00_0000), 0x3c); // 8168C
        assert_eq!(decode_chip_version(0x2800_0000), 0x28); // 8168D
        // bit 23 sobe via <<2 → 0x2c + 0x02 = 0x2e (8168E-vl)
        assert_eq!(decode_chip_version(0x2c80_0000), 0x2e);
    }

    #[test]
    fn tx_opts1_pack() {
        let o = tx_opts1(1514, false);
        assert_eq!(o & DESC_OWN, DESC_OWN);
        assert_eq!(o & DESC_FS, DESC_FS);
        assert_eq!(o & DESC_LS, DESC_LS);
        assert_eq!(o & DESC_EOR, 0);
        assert_eq!(o & TX_LEN_MASK, 1514);

        let last = tx_opts1(60, true);
        assert_eq!(last & DESC_EOR, DESC_EOR);
        assert_eq!(last & TX_LEN_MASK, 60);
    }

    #[test]
    fn rx_opts1_and_payload_len() {
        let armed = rx_opts1(true);
        assert_eq!(armed & DESC_OWN, DESC_OWN);
        assert_eq!(armed & DESC_EOR, DESC_EOR);
        assert_eq!(armed & RX_LEN_MASK, RX_LEN_MASK);

        // NIC concluiu um frame de 1518 bytes com FCS → payload 1514.
        let done = 1518u32; // OWN limpo, len no campo 13:0
        assert_eq!(rx_payload_len(done), 1514);
        // Frame sem FCS suficiente → saturating_sub protege.
        assert_eq!(rx_payload_len(2), 0);
    }

    #[test]
    fn family_ids() {
        for did in [0x8161u16, 0x8168] {
            assert!(is_rtl8168_family(0x10EC, did));
        }
        // 0x8162/0x8167 são partes PCI com MMIO em BAR1, não BAR2 — rejeitar
        // evita mapear/escrever o BAR errado.
        assert!(!is_rtl8168_family(0x10EC, 0x8162));
        assert!(!is_rtl8168_family(0x10EC, 0x8167));
        assert!(!is_rtl8168_family(0x10EC, 0x8139)); // RTL8139 é outro mapa
        assert!(!is_rtl8168_family(0x10EC, 0x8125)); // 2.5G
        assert!(!is_rtl8168_family(0x10EC, 0x8136)); // 8101E 10/100
        assert!(!is_rtl8168_family(0x8086, 0x8168));
    }
}
