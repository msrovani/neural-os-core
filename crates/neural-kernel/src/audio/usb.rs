use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::serial_println;

// USB Audio Class constants (UAC1/UAC2, USB-IF class 0x01)
const UAC_HEADER: u8 = 0x01;
const UAC_INPUT_TERMINAL: u8 = 0x02;
const UAC_OUTPUT_TERMINAL: u8 = 0x03;
const UAC_FEATURE_UNIT: u8 = 0x06;
/// bInterfaceClass 0x01 = Audio (USB-IF class code), usado em descritores de interface.
const USB_CLASS_AUDIO: u8 = 0x01;
/// PCI class 0x0C = Serial Bus Controller; subclasse 0x03 = USB
/// (0x00=UHCI/0x10=OHCI/0x20=EHCI/0x30=xHCI no prog_if).
const PCI_CLASS_SERIAL_BUS: u8 = 0x0C;
const PCI_SUBCLASS_USB: u8 = 0x03;

const UAC_MANIFEST: AgentManifest = AgentManifest {
    name: "usb_audio",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

/// Resultado do probe UAC — distingue "sem controlador USB" de
/// "controlador presente mas sem enumeracao de descritores de interface ainda".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UacProbeResult {
    /// Nenhum controlador USB (xHCI/EHCI/OHCI) encontrado no barramento PCI.
    NoUsbController,
    /// >=1 controlador USB encontrado via PCI, mas o driver xHCI atual
    /// (`crate::xhci`) so enumera 1 dispositivo HID por porta — nao le
    /// descritores de interface (bInterfaceClass) de dispositivos genericos.
    /// Deferred = nao e possivel confirmar/negar UAC sem essa enumeracao.
    ControllerPresentClassScanDeferred { count: u8 },
    /// Dispositivo de classe Audio (0x01) confirmado via descritor de interface.
    /// (Caminho futuro — requer `xhci::enumerate_interfaces()`, nao existe ainda.)
    AudioDeviceFound { vendor_id: u16, device_id: u16 },
}

pub struct UsbAudioAgent;

impl UsbAudioAgent {
    pub fn new() -> Self { UsbAudioAgent }

    /// Detecta dispositivos USB Audio Class.
    ///
    /// Estado real (Sprint 107 Part B #7): `crate::xhci` inicializa o
    /// controlador xHCI e enumera HID (teclado) em 1 porta/slot fixo —
    /// nao ha uma API generica de enumeracao de descritores de interface
    /// (`GET_DESCRIPTOR` recursivo por config/interface/endpoint) para
    /// varrer `bInterfaceClass == USB_CLASS_AUDIO` em todos os dispositivos
    /// conectados. Implementar isso e o trabalho de #84 (futuro).
    ///
    /// Passo honesto desta sprint: em vez do stub `false` incondicional,
    /// ao menos reporta se HA controlador USB (via PCI class 0x0C) — para
    /// diferenciar "sem hardware USB" de "hardware presente, scan pendente".
    fn probe_uac() -> UacProbeResult {
        let devices = unsafe { crate::pci::scan_pci() };
        let usb_controllers = devices
            .iter()
            .filter(|d| d.class == PCI_CLASS_SERIAL_BUS && d.subclass == PCI_SUBCLASS_USB)
            .count();
        if usb_controllers == 0 {
            return UacProbeResult::NoUsbController;
        }
        // xHCI presente (crate::xhci::init_xhci ja roda no boot); sem enumeracao
        // de interfaces genericas ainda, nao podemos confirmar classe Audio real.
        UacProbeResult::ControllerPresentClassScanDeferred { count: usb_controllers as u8 }
    }
}

impl Agent for UsbAudioAgent {
    fn manifest(&self) -> &AgentManifest { &UAC_MANIFEST }
    fn tick(&mut self, _t: u64, _c: u64) -> AgentTickResult {
        match Self::probe_uac() {
            UacProbeResult::AudioDeviceFound { vendor_id, device_id } => {
                serial_println!(
                    "[UAC] USB Audio Class device encontrado vid={:#06x} did={:#06x}",
                    vendor_id, device_id
                );
            }
            UacProbeResult::ControllerPresentClassScanDeferred { count } => {
                serial_println!(
                    "[UAC] xHCI/USB presente ({} controlador(es) PCI classe 0x0C) — \
                     class scan de interface deferido (sem enumeracao generica, ver #84)",
                    count
                );
            }
            UacProbeResult::NoUsbController => {
                serial_println!("[UAC] Nenhum controlador USB (PCI 0x0C) encontrado");
            }
        }
        AgentTickResult::Done
    }
}
