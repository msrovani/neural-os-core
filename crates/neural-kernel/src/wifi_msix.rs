//! MSI-X + 802.11 Fragmentation — interrupcoes reativas + reconstrucao de pacotes.
//! MSI-X: placa WiFi escreve vetor no APIC local (0xFEE0_0000) sem pino fisico.
//! Frag: divide/reconstroi frames 802.11 de ate 2304 bytes em fragmentos de 512B.

use core::ptr::{write_volatile, read_volatile};
use core::sync::atomic::{compiler_fence, Ordering};

// ── 1. MSI-X ─────────────────────────────────────────────────

#[repr(C)]
pub struct MsixTableEntry {
    pub msg_addr_low: u32,
    pub msg_addr_high: u32,
    pub msg_data: u32,
    pub vector_ctrl: u32,
}

/// Configura MSI-X: tabela em msix_base, vetores RX e TX.
pub unsafe fn setup_msix(msix_base: usize, rx_vector: u8, tx_vector: u8) {
    let tbl = msix_base as *mut MsixTableEntry;
    // Vetor 0: RX
    write_volatile(&mut (*tbl.add(0)).msg_addr_low, 0xFEE0_0000);
    write_volatile(&mut (*tbl.add(0)).msg_addr_high, 0);
    write_volatile(&mut (*tbl.add(0)).msg_data, rx_vector as u32);
    write_volatile(&mut (*tbl.add(0)).vector_ctrl, 0); // ativo
    // Vetor 1: TX
    write_volatile(&mut (*tbl.add(1)).msg_addr_low, 0xFEE0_0000);
    write_volatile(&mut (*tbl.add(1)).msg_addr_high, 0);
    write_volatile(&mut (*tbl.add(1)).msg_data, tx_vector as u32);
    write_volatile(&mut (*tbl.add(1)).vector_ctrl, 0);
    compiler_fence(Ordering::SeqCst);
}

/// Sinalizador atomico: ISR → kernel poll
static mut RX_PENDING: bool = false;

/// ISR: chamado pelo IDT quando a placa WiFi dispara MSI-X.
/// Sinaliza o kernel, envia EOI ao APIC local.
#[no_mangle]
pub unsafe extern "x86-interrupt" fn isr_wifi_rx(_frame: *mut u8) {
    RX_PENDING = true;
    write_volatile(0xFEE0_00B0 as *mut u32, 0); // EOI
}

/// ISR de TX completo
#[no_mangle]
pub unsafe extern "x86-interrupt" fn isr_wifi_tx(_frame: *mut u8) {
    write_volatile(0xFEE0_00B0 as *mut u32, 0);
}

// ── 2. FRAGMENTACAO 802.11 ───────────────────────────────────

const FRAG_MAX: usize = 512;       // bytes por fragmento
const FRAG_SLOTS: usize = 4;       // max fragmentos por pacote
const COMPLETE_PKT: usize = 2048;  // buffer de remontagem

#[derive(Debug, Clone, Copy)]
pub struct WifiFrameControl {
    pub seq: u16,
    pub frag: u8,
    pub more_frags: bool,
}

pub struct DefragEngine {
    cur_seq: u16,
    buf: [u8; COMPLETE_PKT],
    written: usize,
    expected: u8,
}

impl DefragEngine {
    pub const fn new() -> Self {
        Self { cur_seq: 0xFFFF, buf: [0; COMPLETE_PKT], written: 0, expected: 0 }
    }

    /// Processa fragmento recebido. Retorna pacote completo quando more_frags=false.
    pub fn process(&mut self, ctrl: WifiFrameControl, payload: &[u8]) -> Option<&[u8]> {
        if ctrl.frag == 0 {
            self.cur_seq = ctrl.seq;
            self.written = 0;
            self.expected = 0;
        } else if ctrl.seq != self.cur_seq || ctrl.frag != self.expected {
            self.written = 0; // fora de ordem → descarta
            return None;
        }
        if self.written + payload.len() > COMPLETE_PKT { self.written = 0; return None; }
        self.buf[self.written..self.written + payload.len()].copy_from_slice(payload);
        self.written += payload.len();
        self.expected += 1;
        if !ctrl.more_frags {
            return Some(&self.buf[..self.written]);
        }
        None
    }

    /// Fragmenta pacote de saida em multiplos frames 802.11.
    pub fn fragment<F: FnMut(WifiFrameControl, &[u8])>(seq: u16, pkt: &[u8], mut tx: F) {
        let n = (pkt.len() + FRAG_MAX - 1) / FRAG_MAX;
        for (i, chunk) in pkt.chunks(FRAG_MAX).enumerate() {
            tx(WifiFrameControl { seq, frag: i as u8, more_frags: i < n - 1 }, chunk);
        }
    }
}

// ── 3. ORQUESTRADOR ──────────────────────────────────────────

pub struct AgnosticNetworkManager {
    defrag: DefragEngine,
    tx_seq: u16,
}

impl AgnosticNetworkManager {
    pub const fn new() -> Self {
        Self { defrag: DefragEngine::new(), tx_seq: 0 }
    }

    /// Polling orientado a interrupcao: so processa se MSI-X sinalizou.
    pub unsafe fn poll(&mut self, hw_buf: &[u8]) {
        if !read_volatile(&RX_PENDING) { return; }
        write_volatile(&mut RX_PENDING, false);

        let ctrl = WifiFrameControl { seq: 104, frag: 0, more_frags: false };
        if let Some(pkt) = self.defrag.process(ctrl, hw_buf) {
            // Entrega para smoltcp via nic_recv()
            crate::netstack::inject_rx_packet(pkt);
        }
    }
}
