//! USB Audio Class (UAC1/UAC2) — probe + parsing de descritores + I/O fallback.
//! Sprint Sound #84: enumeração de interfaces; isócrono pleno depende de HW real.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

const USB_CLASS_AUDIO: u8 = 0x01;
const USB_SUBCLASS_AUDIOCONTROL: u8 = 0x01;
const USB_SUBCLASS_AUDIOSTREAMING: u8 = 0x02;
const PCI_CLASS_SERIAL_BUS: u8 = 0x0C;
const PCI_SUBCLASS_USB: u8 = 0x03;

/// Endpoint isócrono OUT (playback) / IN (capture) descobertos.
static UAC_READY: AtomicBool = AtomicBool::new(false);
static UAC_VID: AtomicU16 = AtomicU16::new(0);
static UAC_DID: AtomicU16 = AtomicU16::new(0);
static UAC_CAPTURE_EP: AtomicU16 = AtomicU16::new(0);
static UAC_PLAYBACK_EP: AtomicU16 = AtomicU16::new(0);
static UAC_SAMPLE_RATE: AtomicU16 = AtomicU16::new(16000);

const UAC_MANIFEST: AgentManifest = AgentManifest {
    name: "usb_audio",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UacProbeResult {
    NoUsbController,
    /// Controlador presente; scan de config feito, sem interface Audio.
    NoAudioInterface { controllers: u8 },
    /// Interface Audio encontrada (AC+AS e endpoints isócronos).
    AudioDeviceFound {
        vendor_id: u16,
        device_id: u16,
        capture_ep: u8,
        playback_ep: u8,
    },
    /// Scan de descritores não pôde completar (xHCI GET_DESCRIPTOR falhou).
    ScanIncomplete { controllers: u8 },
}

/// Info extraída de um Configuration Descriptor USB.
#[derive(Debug, Clone, Copy, Default)]
pub struct UacInterfaceInfo {
    pub has_audio_control: bool,
    pub has_audio_streaming: bool,
    pub capture_ep: u8,
    pub playback_ep: u8,
    pub max_packet: u16,
}

/// Parseia blob de Configuration Descriptor (USB 2.0 §9.6.3).
/// Retorna info UAC se encontrar bInterfaceClass == 0x01.
pub fn parse_config_for_audio(cfg: &[u8]) -> Option<UacInterfaceInfo> {
    if cfg.len() < 9 {
        return None;
    }
    // bLength, bDescriptorType=0x02 (CONFIGURATION)
    if cfg[1] != 0x02 {
        return None;
    }
    let total = u16::from_le_bytes([cfg[2], cfg[3]]) as usize;
    let end = total.min(cfg.len());
    let mut info = UacInterfaceInfo::default();
    let mut i = 0usize;
    let mut in_audio_streaming = false;
    while i + 2 <= end {
        let blen = cfg[i] as usize;
        if blen < 2 || i + blen > end {
            break;
        }
        let dtype = cfg[i + 1];
        match dtype {
            0x04 if blen >= 9 => {
                // INTERFACE
                let if_class = cfg[i + 5];
                let if_sub = cfg[i + 6];
                in_audio_streaming = false;
                if if_class == USB_CLASS_AUDIO {
                    if if_sub == USB_SUBCLASS_AUDIOCONTROL {
                        info.has_audio_control = true;
                    } else if if_sub == USB_SUBCLASS_AUDIOSTREAMING {
                        info.has_audio_streaming = true;
                        in_audio_streaming = true;
                    }
                }
            }
            0x05 if blen >= 7 && in_audio_streaming => {
                // ENDPOINT
                let addr = cfg[i + 2];
                let attr = cfg[i + 3];
                let maxp = u16::from_le_bytes([cfg[i + 4], cfg[i + 5]]);
                let is_iso = (attr & 0x03) == 0x01;
                if is_iso {
                    info.max_packet = info.max_packet.max(maxp);
                    if addr & 0x80 != 0 {
                        info.capture_ep = addr;
                    } else {
                        info.playback_ep = addr;
                    }
                }
            }
            _ => {}
        }
        i += blen;
    }
    if info.has_audio_control || info.has_audio_streaming {
        Some(info)
    } else {
        None
    }
}

pub struct UsbAudioAgent;

impl UsbAudioAgent {
    pub fn new() -> Self {
        UsbAudioAgent
    }

    fn probe_uac() -> UacProbeResult {
        let devices = unsafe { crate::pci::scan_pci() };
        let usb_controllers = devices
            .iter()
            .filter(|d| d.class == PCI_CLASS_SERIAL_BUS && d.subclass == PCI_SUBCLASS_USB)
            .count();
        if usb_controllers == 0 {
            return UacProbeResult::NoUsbController;
        }
        let controllers = usb_controllers as u8;

        // Tenta obter config descriptor via xHCI (pode falhar sem device Audio).
        let mut cfg_buf = [0u8; 512];
        match unsafe { crate::xhci::try_read_config_descriptor(&mut cfg_buf) } {
            Some((n, vid, did)) if n > 0 => {
                if let Some(info) = parse_config_for_audio(&cfg_buf[..n]) {
                    if info.has_audio_streaming
                        && (info.capture_ep != 0 || info.playback_ep != 0)
                    {
                        UAC_VID.store(vid, Ordering::Relaxed);
                        UAC_DID.store(did, Ordering::Relaxed);
                        UAC_CAPTURE_EP.store(info.capture_ep as u16, Ordering::Relaxed);
                        UAC_PLAYBACK_EP.store(info.playback_ep as u16, Ordering::Relaxed);
                        UAC_READY.store(true, Ordering::Relaxed);
                        return UacProbeResult::AudioDeviceFound {
                            vendor_id: vid,
                            device_id: did,
                            capture_ep: info.capture_ep,
                            playback_ep: info.playback_ep,
                        };
                    }
                    return UacProbeResult::NoAudioInterface { controllers };
                }
                UacProbeResult::NoAudioInterface { controllers }
            }
            _ => UacProbeResult::ScanIncomplete { controllers },
        }
    }
}

impl Agent for UsbAudioAgent {
    fn manifest(&self) -> &AgentManifest {
        &UAC_MANIFEST
    }
    fn tick(&mut self, _t: u64, _c: u64) -> AgentTickResult {
        match Self::probe_uac() {
            UacProbeResult::AudioDeviceFound {
                vendor_id,
                device_id,
                capture_ep,
                playback_ep,
            } => {
                k_nano::slog_bin!("UAC", "info", "Audio device vid={:#06x} did={:#06x} cap_ep={:#04x} play_ep={:#04x}",
                    vendor_id,
                    device_id,
                    capture_ep,
                    playback_ep);
            }
            UacProbeResult::NoAudioInterface { controllers } => {
                k_nano::slog_bin!("UAC", "info", "{} USB ctrl — config lida, sem interface Audio (HDA primario)", controllers);
            }
            UacProbeResult::ScanIncomplete { controllers } => {
                k_nano::slog_bin!("UAC", "info", "{} USB ctrl — GET_DESCRIPTOR incompleto (sem device UAC no bus)", controllers);
            }
            UacProbeResult::NoUsbController => {
                k_nano::slog_bin!("UAC", "info", "Nenhum controlador USB (PCI 0x0C)");
            }
        }
        AgentTickResult::Done
    }
}

/// Poll captura UAC → AUDIO_IN (no-op se device ausente).
pub fn poll_uac_audio() {
    if !UAC_READY.load(Ordering::Relaxed) {
        return;
    }
    // Isochronous IN ainda requer TRB periódico xHCI — stub honesto:
    // quando buffer de captura estiver wired, publicar AUDIO_IN aqui.
    let _ = (UAC_CAPTURE_EP.load(Ordering::Relaxed), UAC_SAMPLE_RATE.load(Ordering::Relaxed));
}

/// Playback UAC a partir de PCM (no-op se device ausente / sem EP OUT).
pub fn write_uac_playback(pcm: &[i16]) {
    if !UAC_READY.load(Ordering::Relaxed) || pcm.is_empty() {
        return;
    }
    if UAC_PLAYBACK_EP.load(Ordering::Relaxed) == 0 {
        return;
    }
    // Placeholder: isócrono OUT exigirá ring dedicado; HDA permanece primario.
    let _ = pcm;
}

pub fn uac_is_ready() -> bool {
    UAC_READY.load(Ordering::Relaxed)
}
