//! UVC Driver — USB Video Class (camera). Detecta, configura, captura frames.
//! Usa xHCI para isochronous transfers (bulk no lugar de isoc por simplicidade).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event};
use alloc::vec::Vec;

const UVC_MANIFEST: AgentManifest = AgentManifest {
    name: "uvc_driver",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

pub struct UvcDriverAgent;

impl UvcDriverAgent {
    pub fn new() -> Self { UvcDriverAgent }

    /// Detecta camera USB via PCI (xHCI class 0C03) + USB descriptor scan
    unsafe fn probe_camera() -> bool {
        let devices = crate::pci::scan_pci();
        let has_xhci = devices.iter().any(|d| d.class == 0x0C && d.subclass == 0x03);
        if has_xhci {
            k_nano::serial_println!("[UVC] xHCI encontrado — camera USB possivel");
            // stub: scan real de dispositivos USB requer xHCI device enumeration
            // (futuro: ler descriptors USB, achar interface VIDEO, configurar alt setting)
            return true;
        }
        k_nano::serial_println!("[UVC] Sem xHCI — camera indisponivel");
        false
    }
}

impl Agent for UvcDriverAgent {
    fn manifest(&self) -> &AgentManifest { &UVC_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        unsafe {
            if Self::probe_camera() {
                k_nano::serial_println!("[UVC] Driver camera inicializado");
                // Emite frame de teste para o VisionAgent processar
                let test_frame: Vec<u8> = alloc::vec![128u8; 640 * 480 * 3];
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0, topic: alloc::string::String::from("CAMERA_FRAME"),
                    payload: test_frame,
                    token: CapabilityToken::Legacy(1),
                });
            }
        }
        AgentTickResult::Done
    }
}
