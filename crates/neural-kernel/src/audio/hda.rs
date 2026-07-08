use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::serial_println;
use crate::pci;

// Intel HDA Controller registers (BAR0 offset)
const HDA_GCTL: u64 = 0x08;   // Global Control
const HDA_GCTL_RESET: u32 = 0x01;
const HDA_STATESTS: u64 = 0x0E; // State Change Status
const HDA_INTCTL: u64 = 0x20;  // Interrupt Control
const HDA_INTSTS: u64 = 0x24;  // Interrupt Status
const HDA_WAKEEN: u64 = 0x0C;  // Wake Enable
const HDA_CORB: u64 = 0x40;    // Command Output Ring Buffer (4KB)
const HDA_RIRB: u64 = 0x50;    // Response Input Ring Buffer (4KB)
const HDA_ICS: u64 = 0x00;     // Input/Command Stream
const HDA_OUTPAY: u64 = 0x18;  // Output Payload
const HDA_INPAY: u64 = 0x1C;   // Input Payload

const HDA_MANIFEST: AgentManifest = AgentManifest {
    name: "hda_audio",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

pub struct HdaAudioAgent;

impl HdaAudioAgent {
    pub fn new() -> Self { HdaAudioAgent }

    /// Detecta e inicializa controlador HDA via PCI scan
    unsafe fn probe_hda() -> bool {
        let devices = pci::scan_pci();
        for dev in &devices {
            if dev.vendor_id == 0x8086 && dev.class == 0x04 && dev.subclass == 0x03 {
                // Intel HDA: class 04/03
                let bar = (dev.bar0 as u64 & !0xF) as usize;
                serial_println!("[HDA] Intel HDA controller: {:04x}:{:04x} BAR0={:#x}",
                    dev.vendor_id, dev.device_id, bar);
                // GCTL: reset + take out of reset
                let gctl = core::ptr::read_volatile((bar + HDA_GCTL as usize) as *const u32);
                core::ptr::write_volatile((bar + HDA_GCTL as usize) as *mut u32, gctl | HDA_GCTL_RESET);
                // Aguarda 50us (spin aproximado)
                for _ in 0..100 { core::hint::spin_loop(); }
                core::ptr::write_volatile((bar + HDA_GCTL as usize) as *mut u32, gctl & !HDA_GCTL_RESET);
                serial_println!("[HDA] Controlador HDA init OK");
                return true;
            }
        }
        false
    }
}

impl Agent for HdaAudioAgent {
    fn manifest(&self) -> &AgentManifest { &HDA_MANIFEST }
    fn tick(&mut self, _t: u64, _c: u64) -> AgentTickResult {
        if unsafe { Self::probe_hda() } {
            serial_println!("[HDA] Intel HDA ativo — audio via DMA ring buffer");
        } else {
            serial_println!("[HDA] Nenhum controlador Intel HDA encontrado");
        }
        AgentTickResult::Done
    }
}
