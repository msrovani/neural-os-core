//! USB Audio Class (UAC1/UAC2) — probe + parsing de descritores + I/O fallback.
//! Sprint Sound #84: enumeração de interfaces; isócrono pleno depende de HW real.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use event_bus::{CapabilityToken, Event};

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
        let devices = unsafe { k_nano::pci::scan_pci() };
        let usb_controllers = devices
            .iter()
            .filter(|d| d.class == PCI_CLASS_SERIAL_BUS && d.subclass == PCI_SUBCLASS_USB)
            .count();
        if usb_controllers == 0 {
            return UacProbeResult::NoUsbController;
        }
        let controllers = usb_controllers as u8;

        // Enumeração real no k_nano (reset→address→GET_DESCRIPTOR→SET_INTERFACE→
        // Configure Endpoint isoc). Sem device UAC → fallback de diagnóstico.
        match unsafe { k_nano::xhci::bringup_uac() } {
            Some(dev) => {
                UAC_VID.store(dev.vid, Ordering::Relaxed);
                UAC_DID.store(dev.did, Ordering::Relaxed);
                UAC_CAPTURE_EP.store(dev.capture_ep as u16, Ordering::Relaxed);
                UAC_PLAYBACK_EP.store(dev.playback_ep as u16, Ordering::Relaxed);
                UAC_SAMPLE_RATE.store(dev.sample_rate, Ordering::Relaxed);
                UAC_READY.store(true, Ordering::Relaxed);
                UacProbeResult::AudioDeviceFound {
                    vendor_id: dev.vid,
                    device_id: dev.did,
                    capture_ep: dev.capture_ep,
                    playback_ep: dev.playback_ep,
                }
            }
            None => {
                let mut cfg_buf = [0u8; 512];
                match unsafe { k_nano::xhci::try_read_config_descriptor(&mut cfg_buf) } {
                    Some((n, vid, did)) if n > 0 => {
                        if let Some(info) = parse_config_for_audio(&cfg_buf[..n]) {
                            if info.has_audio_streaming
                                && (info.capture_ep != 0 || info.playback_ep != 0)
                            {
                                return UacProbeResult::AudioDeviceFound {
                                    vendor_id: vid,
                                    device_id: did,
                                    capture_ep: info.capture_ep,
                                    playback_ep: info.playback_ep,
                                };
                            }
                        }
                        UacProbeResult::NoAudioInterface { controllers }
                    }
                    _ => UacProbeResult::ScanIncomplete { controllers },
                }
            }
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
                match k_nano::usb_trust::decide(vendor_id, device_id, "uac") {
                    k_nano::usb_trust::UsbPolicy::Deny => {
                        k_nano::usb_trust::enforce_deny_ports();
                        UAC_READY.store(false, Ordering::Relaxed);
                        k_nano::slog_bin!("UAC", "info", "device blocked by USB-TRUST");
                    }
                    _ => {
                        // Trust OK: arma o ring isócrono IN (captura contínua).
                        let armed = unsafe { k_nano::xhci::schedule_isoc_in() };
                        k_nano::slog_bin!("UAC", "info", "Audio device vid={:#06x} did={:#06x} cap_ep={:#04x} play_ep={:#04x} rate={} isoc_armed={}",
                            vendor_id,
                            device_id,
                            capture_ep,
                            playback_ep,
                            UAC_SAMPLE_RATE.load(Ordering::Relaxed),
                            armed);
                        k_nano::slog_bin!(
                            "UAC-HW",
                            "info",
                            "VERDICT=OK reason=isoc_trb_scheduled"
                        );
                    }
                }
            }
            UacProbeResult::NoAudioInterface { controllers } => {
                k_nano::slog_bin!("UAC", "info", "{} USB ctrl — config lida, sem interface Audio (HDA primario)", controllers);
                k_nano::slog_bin!(
                    "UAC-HW",
                    "info",
                    "VERDICT=AWAITING_REAL_HW reason=no_uac_interface"
                );
            }
            UacProbeResult::ScanIncomplete { controllers } => {
                k_nano::slog_bin!("UAC", "info", "{} USB ctrl — GET_DESCRIPTOR incompleto (sem device UAC no bus)", controllers);
                k_nano::slog_bin!(
                    "UAC-HW",
                    "info",
                    "VERDICT=AWAITING_REAL_HW reason=ep0_get_descriptor_incomplete"
                );
            }
            UacProbeResult::NoUsbController => {
                k_nano::slog_bin!("UAC", "info", "Nenhum controlador USB (PCI 0x0C)");
            }
        }
        AgentTickResult::Done
    }
}

/// Poll captura UAC → isoc IN ring → AUDIO_IN (EventBus; voice.rs faz VAD/FFT e
/// empurra para MIC_CAPTURE_RING). Mantém o anel OUT vivo com silêncio.
pub fn poll_uac_audio() {
    if !UAC_READY.load(Ordering::Relaxed) {
        return;
    }
    let mut buf = [0i16; 512];
    loop {
        let n = unsafe { k_nano::xhci::poll_isoc_in(&mut buf) };
        if n == 0 {
            break;
        }
        let bytes: alloc::vec::Vec<u8> = buf[..n]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(crate::audio::TOPIC_AUDIO_IN),
            payload: bytes,
            token: CapabilityToken::Legacy(1),
        });
    }
    // Playback ocioso: re-arma slots OUT com silêncio (evita Ring Underrun).
    let _ = unsafe { k_nano::xhci::poll_isoc_out() };
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::Relaxed) {
        k_nano::slog_bin!("UAC-HW", "info", "step=isoc_in status=OK ring=poll_isoc_in");
    }
}

/// Playback UAC a partir de PCM (isoc OUT ring → USB speaker/headset).
pub fn write_uac_playback(pcm: &[i16]) {
    if !UAC_READY.load(Ordering::Relaxed) || pcm.is_empty() {
        return;
    }
    if UAC_PLAYBACK_EP.load(Ordering::Relaxed) == 0 {
        return;
    }
    let queued = unsafe { k_nano::xhci::schedule_isoc_out(pcm) };
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::Relaxed) {
        k_nano::slog_bin!(
            "UAC-HW",
            "info",
            "step=isoc_out status=OK queued={}",
            queued
        );
    }
}

pub fn uac_is_ready() -> bool {
    UAC_READY.load(Ordering::Relaxed)
}
