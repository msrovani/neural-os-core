//! Intel HDA Audio Driver — Capture (SD0 input stream).
//! Implements CORB/RIRB command interface, codec enumeration, SD0 DMA ring (BDL), IRQ handler.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};
use crate::pci;
use crate::memory::PHYS_MEM_OFFSET;
use crate::dma::{dma_alloc, DmaBuf};
use crate::apic::map_page_uc;
use crate::slog_nano;
use event_bus::{Event, CapabilityToken};

// ============================================================================
// HDA Controller Register Map (Intel HDA 1.0a Spec)
// ============================================================================

// Global Controller Registers
const HDA_GCAP: u64 = 0x00;       // Global Capabilities
const HDA_VMIN: u64 = 0x02;       // Minor Version
const HDA_VMAJ: u64 = 0x03;       // Major Version
const HDA_OUTPAY: u64 = 0x04;     // Output Payload Capability
const HDA_INPAY: u64 = 0x06;      // Input Payload Capability
const HDA_GCTL: u64 = 0x08;       // Global Control
const HDA_WAKEEN: u64 = 0x0C;     // Wake Enable
const HDA_STATESTS: u64 = 0x0E;   // State Change Status
const HDA_GSTS: u64 = 0x10;       // Global Status
const HDA_INTCTL: u64 = 0x20;     // Interrupt Control
const HDA_INTSTS: u64 = 0x24;     // Interrupt Status
const HDA_WALCLK: u64 = 0x30;     // Wall Clock Counter

// CORB (Command Output Ring Buffer)
const HDA_CORBLBASE: u64 = 0x40;  // CORB Lower Base Address
const HDA_CORBUBASE: u64 = 0x44;  // CORB Upper Base Address
const HDA_CORBWP: u64 = 0x48;     // CORB Write Pointer
const HDA_CORBRP: u64 = 0x4A;     // CORB Read Pointer
const HDA_CORBCTL: u64 = 0x4C;    // CORB Control
const HDA_CORBSTS: u64 = 0x4D;    // CORB Status
const HDA_CORBSIZE: u64 = 0x4E;   // CORB Size

// RIRB (Response Input Ring Buffer)
const HDA_RIRBLBASE: u64 = 0x50;  // RIRB Lower Base Address
const HDA_RIRBUBASE: u64 = 0x54;  // RIRB Upper Base Address
const HDA_RIRBWP: u64 = 0x58;     // RIRB Write Pointer
const HDA_RIRBRP: u64 = 0x5A;     // RIRB Read Pointer
const HDA_RIRBCTL: u64 = 0x5C;    // RIRB Control
const HDA_RIRBSTS: u64 = 0x5D;    // RIRB Status
const HDA_RIRBSIZE: u64 = 0x5E;   // RIRB Size

// Immediate Command
const HDA_ICW: u64 = 0x60;        // Immediate Command Write
const HDA_ICR: u64 = 0x64;        // Immediate Command Read
const HDA_ICS: u64 = 0x68;        // Immediate Command Status

// Stream Descriptor 0 (SD0) - Capture (Microphone)
// Base offset 0x80, each SD is 0x20 bytes
const SD0_BASE: u64 = 0x80;
const SDX_CTL: u64 = 0x00;        // Stream Descriptor Control (1 byte)
const SDX_STS: u64 = 0x03;        // Stream Descriptor Status (1 byte)
const SDX_LPIB: u64 = 0x04;       // Link Position in Buffer (4 bytes)
const SDX_CBL: u64 = 0x08;        // Cyclic Buffer Length (4 bytes)
const SDX_LVI: u64 = 0x0C;        // Last Valid Index (2 bytes)
const SDX_FMT: u64 = 0x0E;        // Stream Format (2 bytes)
const SDX_BDPL: u64 = 0x10;       // Buffer Descriptor List Pointer Lower (4 bytes)
const SDX_BDPU: u64 = 0x14;       // Buffer Descriptor List Pointer Upper (4 bytes)

// Stream Descriptor 1 (SD1) - Playback (Speaker)
const SD1_BASE: u64 = 0xA0;

// ============================================================================
// Register Bit Definitions
// ============================================================================

// GCTL
const GCTL_CRST: u32 = 1 << 0;    // Controller Reset
const GCTL_FCNTRL: u32 = 1 << 1;  // Flush Control
const GCTL_UNSOL: u32 = 1 << 8;   // Accept Unsolicited Response Enable

// CORBCTL / RIRBCTL
const CORB_RUN: u32 = 1 << 1;     // Run
const CORB_CMEIE: u32 = 1 << 0;   // CORB Memory Error Interrupt Enable
const RIRB_RINTCTL: u32 = 1 << 0; // Response Interrupt Control
const RIRB_DMA_EN: u32 = 1 << 1;  // DMA Enable

// SDx_CTL
const SD_CTL_RUN: u32 = 1 << 0;   // Run
const SD_CTL_SRST: u32 = 1 << 1;  // Stream Reset
const SD_CTL_IOCE: u32 = 1 << 2;  // Interrupt on Completion Enable
const SD_CTL_FEIE: u32 = 1 << 3;  // FIFO Error Interrupt Enable
const SD_CTL_DEIE: u32 = 1 << 4;  // Descriptor Error Interrupt Enable
const SD_CTL_STRIPE_MASK: u32 = 0x7 << 16; // Stripe control

// SDx_STS
const SD_STS_FIFORDY: u32 = 1 << 0; // FIFO Ready
const SD_STS_BCIS: u32 = 1 << 2;    // Buffer Completion Interrupt Status
const SD_STS_FIFOE: u32 = 1 << 3;   // FIFO Error
const SD_STS_DESE: u32 = 1 << 4;    // Descriptor Error

// ============================================================================
// Codec Verbs (HDA Spec)
// ============================================================================

const VERB_GET_PARAMETER: u32 = 0xF00;
const VERB_GET_CONNECTION_LIST: u32 = 0xF02;
const VERB_GET_CONNECTION_SELECT: u32 = 0xF01;
const VERB_SET_CONNECTION_SELECT: u32 = 0x701;
const VERB_GET_PIN_WIDGET_CONTROL: u32 = 0xF07;
const VERB_SET_PIN_WIDGET_CONTROL: u32 = 0x707;
const VERB_GET_CONVERTER_FORMAT: u32 = 0xA00;
const VERB_SET_CONVERTER_FORMAT: u32 = 0x200;
const VERB_GET_STREAM_FORMAT: u32 = 0xA00;
const VERB_SET_STREAM_FORMAT: u32 = 0x200;
const VERB_GET_AMP_GAIN_MUTE: u32 = 0xB00;
const VERB_SET_AMP_GAIN_MUTE: u32 = 0x300;
const VERB_GET_CONVERTER_STREAM_CHANNEL: u32 = 0xF06;
const VERB_SET_CONVERTER_STREAM_CHANNEL: u32 = 0x706;
const VERB_GET_PIN_SENSE: u32 = 0xF09;
const VERB_GET_CONFIG_DEFAULT: u32 = 0xF1C;
const VERB_GET_SUBSYSTEM_ID: u32 = 0xF20;

// Parameter IDs
const PARAM_VENDOR_ID: u32 = 0x00;
const PARAM_REVISION_ID: u32 = 0x02;
const PARAM_SUB_NODE_COUNT: u32 = 0x04;
const PARAM_FUNCTION_GROUP_TYPE: u32 = 0x05;
const PARAM_AUDIO_FG_CAP: u32 = 0x08;
const PARAM_AUDIO_WIDGET_CAP: u32 = 0x09;
const PARAM_PCAP: u32 = 0x0A;
const PARAM_IN_AMP_CAP: u32 = 0x0B;
const PARAM_OUT_AMP_CAP: u32 = 0x0C;
const PARAM_CONNLIST_LEN: u32 = 0x0E;
const PARAM_POWER_STATE: u32 = 0x0F;
const PARAM_PROC_WIDGET_CAP: u32 = 0x10;
const PARAM_GPIO_CAP: u32 = 0x11;
const PARAM_VOLUME_KNOB_CAP: u32 = 0x12;

// Widget Types
const WIDGET_TYPE_AUDIO_OUTPUT: u32 = 0x0;
const WIDGET_TYPE_AUDIO_INPUT: u32 = 0x1;
const WIDGET_TYPE_AUDIO_MIXER: u32 = 0x2;
const WIDGET_TYPE_AUDIO_SELECTOR: u32 = 0x3;
const WIDGET_TYPE_PIN_COMPLEX: u32 = 0x4;
const WIDGET_TYPE_POWER_WIDGET: u32 = 0x5;
const WIDGET_TYPE_VOLUME_KNOB: u32 = 0x6;
const WIDGET_TYPE_BEEP_GENERATOR: u32 = 0x7;
const WIDGET_TYPE_VENDOR_DEFINED: u32 = 0xF;

// Pin Widget Control bits
const PIN_VREF_HIZ: u32 = 0x00;
const PIN_VREF_50: u32 = 0x01;
const PIN_VREF_GND: u32 = 0x02;
const PIN_VREF_80: u32 = 0x04;    // 80% = 0x24 (VREF_EN=1, VREF=80%)
const PIN_VREF_100: u32 = 0x05;
const PIN_IN_EN: u32 = 0x10;
const PIN_OUT_EN: u32 = 0x20;
const PIN_HP_EN: u32 = 0x40;

// ============================================================================
// Audio Format (16-bit, 48kHz, stereo)
// ============================================================================
const FMT_16BIT_48KHZ_STEREO: u32 = 0x0000_0021; // Type=PCM(0), 16-bit(2), 48kHz(0), stereo(1)

// ============================================================================
// Global State
// ============================================================================

static HDA_INIT_DONE: AtomicBool = AtomicBool::new(false);
static HDA_BAR: AtomicU64 = AtomicU64::new(0);
static HDA_IRQ: AtomicU32 = AtomicU32::new(0);
static HDA_CODEC_MASK: AtomicU32 = AtomicU32::new(0);
static HDA_CORB_BUF: AtomicU64 = AtomicU64::new(0);
static HDA_RIRB_BUF: AtomicU64 = AtomicU64::new(0);
static HDA_SD0_BDL: AtomicU64 = AtomicU64::new(0);
static HDA_SD0_BUF: AtomicU64 = AtomicU64::new(0);
static HDA_CORB_WP: AtomicU32 = AtomicU32::new(0);
static HDA_RIRB_RP: AtomicU32 = AtomicU32::new(0);
static HDA_SD0_RPI: AtomicU32 = AtomicU32::new(0); // Read Pointer Index for BDL

// DMA buffers (kept alive)
static mut CORB_DMA: Option<DmaBuf> = None;
static mut RIRB_DMA: Option<DmaBuf> = None;
static mut SD0_BDL_DMA: Option<DmaBuf> = None;
static mut SD0_AUDIO_DMA: Option<DmaBuf> = None;

// ============================================================================
// MMIO Access Helpers
// ============================================================================

#[inline]
unsafe fn reg32(bar: u64, off: u64) -> *mut u32 {
    (bar + off) as *mut u32
}

#[inline]
unsafe fn r32(bar: u64, off: u64) -> u32 {
    core::ptr::read_volatile(reg32(bar, off))
}

#[inline]
unsafe fn w32(bar: u64, off: u64, v: u32) {
    core::ptr::write_volatile(reg32(bar, off), v);
}

#[inline]
unsafe fn r16(bar: u64, off: u64) -> u16 {
    core::ptr::read_volatile((bar + off) as *const u16)
}

#[inline]
unsafe fn w16(bar: u64, off: u64, v: u16) {
    core::ptr::write_volatile((bar + off) as *mut u16, v);
}

#[inline]
unsafe fn r8(bar: u64, off: u64) -> u8 {
    core::ptr::read_volatile((bar + off) as *const u8)
}

#[inline]
unsafe fn w8(bar: u64, off: u64, v: u8) {
    core::ptr::write_volatile((bar + off) as *mut u8, v);
}

// ============================================================================
// CORB/RIRB Command Interface
// ============================================================================

/// Allocate and initialize CORB (256 entries) and RIRB (256 entries) in uncached memory.
unsafe fn init_corb_rirb(bar: u64) -> bool {
    // CORB: 256 entries × 4 bytes = 1024 bytes, aligned to 128 bytes
    let corb_size = 256 * 4;
    let corb_dma = match dma_alloc(corb_size) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate CORB DMA buffer");
            return false;
        }
    };
    let corb_phys = corb_dma.phys;
    HDA_CORB_BUF.store(corb_phys, Ordering::Release);
    
    // RIRB: 256 entries × 8 bytes = 2048 bytes, aligned to 128 bytes
    let rirb_size = 256 * 8;
    let rirb_dma = match dma_alloc(rirb_size) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate RIRB DMA buffer");
            return false;
        }
    };
    let rirb_phys = rirb_dma.phys;
    HDA_RIRB_BUF.store(rirb_phys, Ordering::Release);
    
    // Store DMA buffers to keep them alive
    CORB_DMA = Some(corb_dma);
    RIRB_DMA = Some(rirb_dma);
    
    // Program CORB base address
    w32(bar, HDA_CORBLBASE, corb_phys as u32);
    w32(bar, HDA_CORBUBASE, (corb_phys >> 32) as u32);
    w16(bar, HDA_CORBWP, 0);
    w16(bar, HDA_CORBRP, 0);
    w8(bar, HDA_CORBCTL, CORB_RUN as u8 | CORB_CMEIE as u8);
    w8(bar, HDA_CORBSIZE, 0x02); // 256 entries
    
    // Program RIRB base address
    w32(bar, HDA_RIRBLBASE, rirb_phys as u32);
    w32(bar, HDA_RIRBUBASE, (rirb_phys >> 32) as u32);
    w16(bar, HDA_RIRBWP, 0);
    w16(bar, HDA_RIRBRP, 0);
    w8(bar, HDA_RIRBCTL, RIRB_RINTCTL as u8 | RIRB_DMA_EN as u8);
    w8(bar, HDA_RIRBSIZE, 0x02); // 256 entries
    
    // Reset pointers
    HDA_CORB_WP.store(0, Ordering::Release);
    HDA_RIRB_RP.store(0, Ordering::Release);
    
    slog_nano!("HDA", "info", "CORB @ 0x{:x} RIRB @ 0x{:x}", corb_phys, rirb_phys);
    true
}

/// Write a verb to CORB and wait for response in RIRB.
/// Returns (response, success).
unsafe fn corb_write_and_wait(bar: u64, cad: u8, node: u8, verb: u32) -> (u32, bool) {
    let wp = HDA_CORB_WP.load(Ordering::Acquire);
    let next_wp = (wp + 1) % 256;
    
    // Check if CORB is full (next_wp == rp)
    let rp = r16(bar, HDA_CORBRP) as u32;
    if next_wp == rp {
        return (0, false); // CORB full
    }
    
    // Build command: [31:28] = CAD, [27:20] = NodeID, [19:0] = Verb
    let cmd = ((cad as u32) << 28) | ((node as u32) << 20) | (verb & 0xFFFFF);
    
    // Write to CORB
    let corb_phys = HDA_CORB_BUF.load(Ordering::Acquire);
    let corb_virt = (corb_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *mut u32;
    core::ptr::write_volatile(corb_virt.add(wp as usize), cmd);
    
    // Update write pointer
    w16(bar, HDA_CORBWP, next_wp as u16);
    HDA_CORB_WP.store(next_wp, Ordering::Release);
    
    // Wait for response in RIRB (poll with timeout)
    for _ in 0..10000 {
        let rirb_rp = HDA_RIRB_RP.load(Ordering::Acquire);
        let rirb_wp = r16(bar, HDA_RIRBWP) as u32;
        
        if rirb_rp != rirb_wp {
            // Response available
            let rirb_phys = HDA_RIRB_BUF.load(Ordering::Acquire);
            let rirb_virt = (rirb_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *mut u64;
            let response = core::ptr::read_volatile(rirb_virt.add(rirb_rp as usize));
            
            // Advance RIRB read pointer
            let next_rp = (rirb_rp + 1) % 256;
            w16(bar, HDA_RIRBRP, next_rp as u16);
            HDA_RIRB_RP.store(next_rp, Ordering::Release);
            
            // Response format: [63:32] = response, [31:0] = unsolicited tag (ignore)
            let resp = (response >> 32) as u32;
            return (resp, true);
        }
        core::hint::spin_loop();
    }
    
    (0, false) // Timeout
}

/// Send a verb via Immediate Command interface (fallback for init).
unsafe fn icw_send(bar: u64, cad: u8, node: u8, verb: u32) -> Option<u32> {
    let icw = ((cad as u32) << 28) | ((node as u32) << 20) | (verb & 0xFFFFF);
    w32(bar, HDA_ICW, icw);
    
    // Wait for completion (bit 31 = ICB - Immediate Command Busy)
    for _ in 0..50000 {
        let status = r32(bar, HDA_ICW);
        if status & 0x8000_0000 == 0 {
            let resp = r32(bar, HDA_ICR);
            if resp != 0 && resp != 0xFFFF_FFFF {
                return Some(resp);
            }
            return None;
        }
        core::hint::spin_loop();
    }
    None
}

// ============================================================================
// Codec Enumeration & Widget Discovery
// ============================================================================

#[derive(Debug, Clone, Copy)]
struct WidgetInfo {
    nid: u8,
    widget_type: u32,
    caps: u32,
    connections: [u8; 8],
    num_connections: u8,
}

#[derive(Debug, Clone, Copy)]
struct CodecInfo {
    cad: u8,
    vendor_id: u32,
    revision_id: u32,
    widgets: [WidgetInfo; 32],
    num_widgets: u8,
    audio_fg_nid: u8,
    mic_pin_nid: u8,
    adc_nid: u8,
}

static mut CODECS: [CodecInfo; 8] = [CodecInfo {
    cad: 0,
    vendor_id: 0,
    revision_id: 0,
    widgets: [WidgetInfo { nid: 0, widget_type: 0, caps: 0, connections: [0; 8], num_connections: 0 }; 32],
    num_widgets: 0,
    audio_fg_nid: 0,
    mic_pin_nid: 0,
    adc_nid: 0,
}; 8];

/// Enumerate codecs and discover widgets.
unsafe fn enumerate_codecs(bar: u64) -> bool {
    let mut found_codec = false;
    
    for cad in 0..8u8 {
        // Get vendor ID via Immediate Command (simpler for init)
        let resp = icw_send(bar, cad, 0x00, VERB_GET_PARAMETER | PARAM_VENDOR_ID);
        let vendor_id = match resp {
            Some(v) => v,
            None => continue,
        };
        
        let rev_resp = icw_send(bar, cad, 0x00, VERB_GET_PARAMETER | PARAM_REVISION_ID);
        let revision_id = rev_resp.unwrap_or(0);
        
        slog_nano!("HDA", "info", "Codec {}: vendor={:#08x} rev={:#08x}", cad, vendor_id, revision_id);
        
        // Get sub-node count (widgets)
        let sub_resp = icw_send(bar, cad, 0x00, VERB_GET_PARAMETER | PARAM_SUB_NODE_COUNT);
        let (start_nid, total_widgets) = match sub_resp {
            Some(v) => ((v >> 16) as u8, (v & 0xFF) as u8),
            None => continue,
        };
        
        let mut codec = CodecInfo {
            cad,
            vendor_id,
            revision_id,
            widgets: [WidgetInfo { nid: 0, widget_type: 0, caps: 0, connections: [0; 8], num_connections: 0 }; 32],
            num_widgets: 0,
            audio_fg_nid: 0,
            mic_pin_nid: 0,
            adc_nid: 0,
        };
        
        // Enumerate widgets
        let mut widget_idx = 0;
        for nid in start_nid..(start_nid + total_widgets) {
            if widget_idx >= 32 { break; }
            
            // Get widget capabilities
            let caps_resp = icw_send(bar, cad, nid, VERB_GET_PARAMETER | PARAM_AUDIO_WIDGET_CAP);
            let caps = caps_resp.unwrap_or(0);
            let widget_type = (caps >> 20) & 0xF;
            
            let mut widget = WidgetInfo {
                nid,
                widget_type,
                caps,
                connections: [0; 8],
                num_connections: 0,
            };
            
            // Get connection list for input widgets
            if widget_type == WIDGET_TYPE_AUDIO_INPUT || widget_type == WIDGET_TYPE_PIN_COMPLEX {
                let conn_resp = icw_send(bar, cad, nid, VERB_GET_PARAMETER | PARAM_CONNLIST_LEN);
                if let Some(conn_len) = conn_resp {
                    let num_conns = (conn_len & 0x7F) as u8;
                    widget.num_connections = num_conns.min(8);
                    
                    // Read connection list (long form if > 8)
                    if num_conns > 0 {
                        let list_resp = icw_send(bar, cad, nid, VERB_GET_CONNECTION_LIST);
                        if let Some(list) = list_resp {
                            for i in 0..widget.num_connections as usize {
                                widget.connections[i] = ((list >> (i * 4)) & 0xF) as u8;
                            }
                        }
                    }
                }
            }
            
            // Check for Audio Function Group
            if widget_type == 0x1 { // Function group
                let fg_type_resp = icw_send(bar, cad, nid, VERB_GET_PARAMETER | PARAM_FUNCTION_GROUP_TYPE);
                if let Some(fg_type) = fg_type_resp {
                    if fg_type & 0xFF == 0x01 { // Audio Function Group
                        codec.audio_fg_nid = nid;
                    }
                }
            }
            
            // Check for microphone pin (Pin Complex with input capability)
            if widget_type == WIDGET_TYPE_PIN_COMPLEX {
                let pin_cap_resp = icw_send(bar, cad, nid, VERB_GET_PARAMETER | PARAM_PCAP);
                if let Some(pin_cap) = pin_cap_resp {
                    let location = (pin_cap >> 30) & 0x3;
                    let is_input = (pin_cap >> 24) & 0x1;
                    // Location: 0=external, 1=internal, 2=separate, 3=other
                    // Look for internal mic (location=1) or external mic (location=0) with input capability
                    if is_input == 1 && (location == 0 || location == 1) {
                        codec.mic_pin_nid = nid;
                    }
                }
            }
            
            // Check for ADC (Audio Input widget)
            if widget_type == WIDGET_TYPE_AUDIO_INPUT {
                // Prefer ADC connected to our mic pin
                if codec.mic_pin_nid != 0 {
                    for i in 0..widget.num_connections as usize {
                        if widget.connections[i] == codec.mic_pin_nid {
                            codec.adc_nid = nid;
                            break;
                        }
                    }
                }
                if codec.adc_nid == 0 {
                    codec.adc_nid = nid; // fallback to first ADC
                }
            }
            
            codec.widgets[widget_idx] = widget;
            widget_idx += 1;
        }
        
        codec.num_widgets = widget_idx as u8;
        CODECS[cad as usize] = codec;
        HDA_CODEC_MASK.fetch_or(1 << cad, Ordering::Release);
        found_codec = true;
    }
    
    found_codec
}

/// Configure the microphone pin and ADC for capture.
unsafe fn configure_capture_path(bar: u64) -> bool {
    // Find first codec with valid mic pin and ADC
    for cad in 0..8u8 {
        if HDA_CODEC_MASK.load(Ordering::Acquire) & (1 << cad) == 0 {
            continue;
        }
        
        let codec = CODECS[cad as usize];
        if codec.mic_pin_nid == 0 || codec.adc_nid == 0 {
            continue;
        }
        
        slog_nano!("HDA", "info", "Configuring capture: CAD={} PIN={} ADC={}", cad, codec.mic_pin_nid, codec.adc_nid);
        
        // 1. Set Pin Widget Control: VREF_EN=80% (0x24) + IN_EN (0x10) = 0x34
        let pin_ctl = PIN_VREF_80 | PIN_IN_EN; // 0x24 | 0x10 = 0x34
        let _ = corb_write_and_wait(bar, cad, codec.mic_pin_nid, VERB_SET_PIN_WIDGET_CONTROL | pin_ctl);
        
        // 2. Set Connection Select on ADC to connect to mic pin
        let _ = corb_write_and_wait(bar, cad, codec.adc_nid, VERB_SET_CONNECTION_SELECT | (codec.mic_pin_nid as u32));
        
        // 3. Set Converter Format: 16-bit, 48kHz, stereo
        let _ = corb_write_and_wait(bar, cad, codec.adc_nid, VERB_SET_CONVERTER_FORMAT | FMT_16BIT_48KHZ_STEREO);
        
        // 4. Set Stream/Channel on ADC (stream tag 1, channel 0)
        let stream_channel = (1u32 << 4) | 0u32; // stream=1, channel=0
        let _ = corb_write_and_wait(bar, cad, codec.adc_nid, VERB_SET_CONVERTER_STREAM_CHANNEL | stream_channel);
        
        // 5. Set Amplifier Gain/Mute on mic pin (unmute, 0dB gain)
        let amp_gain = (1u32 << 7) | (0u32 << 8); // output amp, mute=0, gain=0
        let _ = corb_write_and_wait(bar, cad, codec.mic_pin_nid, VERB_SET_AMP_GAIN_MUTE | amp_gain);
        
        // 6. Set Amplifier on ADC input (unmute, 0dB)
        let amp_gain_adc = (0u32 << 7) | (0u32 << 8); // input amp, mute=0, gain=0
        let _ = corb_write_and_wait(bar, cad, codec.adc_nid, VERB_SET_AMP_GAIN_MUTE | amp_gain_adc);
        
        slog_nano!("HDA", "info", "Capture path configured for CAD {}", cad);
        return true;
    }
    
    false
}

// ============================================================================
// SD0 Input Stream Setup (BDL - Buffer Descriptor List)
// ============================================================================

/// Allocate and configure SD0 BDL and audio buffer.
/// BDL: 16 entries × 16 bytes = 256 bytes (each entry: 8-byte addr + 4-byte len + 4-byte IOC)
/// Audio buffer: 16 × 4KB = 64KB ring
unsafe fn init_sd0_capture(bar: u64) -> bool {
    // Allocate BDL (16 entries, 16 bytes each = 256 bytes, aligned to 128 bytes)
    let bdl_dma = match dma_alloc(256) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate SD0 BDL");
            return false;
        }
    };
    let bdl_phys = bdl_dma.phys;
    HDA_SD0_BDL.store(bdl_phys, Ordering::Release);
    SD0_BDL_DMA = Some(bdl_dma);
    
    // Allocate audio capture buffer: 16 pages × 4KB = 64KB
    let audio_size = 16 * 4096;
    let audio_dma = match dma_alloc(audio_size) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate SD0 audio buffer");
            return false;
        }
    };
    let audio_phys = audio_dma.phys;
    HDA_SD0_BUF.store(audio_phys, Ordering::Release);
    SD0_AUDIO_DMA = Some(audio_dma);
    
    // Build BDL entries: 16 entries of 4KB each
    // BDL entry format (16 bytes): u64 addr + u32 len + u32 ioc
    let bdl_virt = (bdl_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *mut u8;
    for i in 0..16 {
        let entry_phys = audio_phys + (i as u64 * 4096);
        let entry_base = bdl_virt.add(i * 16);
        // Buffer address (8 bytes)
        unsafe { core::ptr::write_volatile(entry_base as *mut u64, entry_phys); }
        // Buffer length (4 bytes) - 4096 bytes per entry
        unsafe { core::ptr::write_volatile(entry_base.add(8) as *mut u32, 4096u32); }
        // IOC (4 bytes) - bit 0 = interrupt on completion
        unsafe { core::ptr::write_volatile(entry_base.add(12) as *mut u32, 1u32); }
    }
    
    // Program SD0 registers
    let sd0_ctl = SD0_BASE + SDX_CTL;
    let sd0_sts = SD0_BASE + SDX_STS;
    let sd0_cbl = SD0_BASE + SDX_CBL;
    let sd0_lvi = SD0_BASE + SDX_LVI;
    let sd0_fmt = SD0_BASE + SDX_FMT;
    let sd0_bdpl = SD0_BASE + SDX_BDPL;
    let sd0_bdpu = SD0_BASE + SDX_BDPU;
    
    // Stop stream first
    w8(bar, sd0_ctl, 0);
    for _ in 0..1000 { core::hint::spin_loop(); }
    
    // Reset stream
    w8(bar, sd0_ctl, SD_CTL_SRST as u8);
    for _ in 0..1000 { core::hint::spin_loop(); }
    w8(bar, sd0_ctl, 0);
    for _ in 0..1000 { core::hint::spin_loop(); }
    
    // Program BDL address
    w32(bar, sd0_bdpl, bdl_phys as u32);
    w32(bar, sd0_bdpu, (bdl_phys >> 32) as u32);
    
    // Program Cyclic Buffer Length (total ring size = 64KB)
    w32(bar, sd0_cbl, audio_size as u32);
    
    // Program Last Valid Index (15 = 16 entries, 0-based)
    w16(bar, sd0_lvi, 15);
    
    // Program Format (16-bit, 48kHz, stereo)
    w16(bar, sd0_fmt, FMT_16BIT_48KHZ_STEREO as u16);
    
    // Clear status
    w16(bar, sd0_sts, 0xFFFF); // Write 1 to clear
    
    // Enable stream: RUN + IOCE (interrupt on completion)
    w8(bar, sd0_ctl, SD_CTL_RUN as u8 | SD_CTL_IOCE as u8);
    
    // Wait for FIFO ready
    for _ in 0..10000 {
        let sts = r8(bar, sd0_sts);
        if sts & SD_STS_FIFORDY as u8 != 0 {
            break;
        }
        core::hint::spin_loop();
    }
    
    // Reset read pointer index
    HDA_SD0_RPI.store(0, Ordering::Release);
    
    slog_nano!("HDA", "info", "SD0 capture: BDL @ 0x{:x} buf @ 0x{:x} size={}KB", bdl_phys, audio_phys, audio_size / 1024);
    true
}

// ============================================================================
// IRQ Handler
// ============================================================================

/// HDA interrupt handler - called from interrupts.rs
/// Minimal work: copy completed BDL entries to MIC_CAPTURE_RING, advance RPI
pub unsafe fn hda_irq_handler() {
    let bar = HDA_BAR.load(Ordering::Acquire);
    if bar == 0 || !HDA_INIT_DONE.load(Ordering::Acquire) {
        return;
    }
    
    // Check global interrupt status
    let intsts = r32(bar, HDA_INTSTS);
    if intsts == 0 {
        return;
    }
    
    // Check SD0 interrupt status
    let sd0_sts_off = SD0_BASE + SDX_STS;
    let sd0_sts = r16(bar, sd0_sts_off);
    
    // Clear interrupt (write 1 to clear BCIS)
    if sd0_sts & SD_STS_BCIS as u16 != 0 {
        w16(bar, sd0_sts_off, sd0_sts | SD_STS_BCIS as u16);
    }
    
    // Check for FIFO error or descriptor error
    if sd0_sts & (SD_STS_FIFOE | SD_STS_DESE) as u16 != 0 {
        slog_nano!("HDA", "warn", "SD0 error: sts={:#06x}", sd0_sts);
        // Clear errors
        w16(bar, sd0_sts_off, sd0_sts | SD_STS_FIFOE as u16 | SD_STS_DESE as u16);
    }
    
    // Process completed BDL entries
    let rpi = HDA_SD0_RPI.load(Ordering::Acquire);
    let audio_phys = HDA_SD0_BUF.load(Ordering::Acquire);
    let audio_virt = (audio_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *const i16;
    
    // Each BDL entry is 4KB = 2048 i16 samples
    const SAMPLES_PER_ENTRY: usize = 2048;
    
    // Process up to 16 entries (full ring)
    for _ in 0..16 {
        let entry_idx = rpi % 16;
        let entry_offset = (entry_idx as usize) * SAMPLES_PER_ENTRY;
        
        // Copy samples to MIC_CAPTURE_RING (via EventBus publish)
        // We'll publish in chunks to avoid huge allocations
        const CHUNK_SIZE: usize = 512;
        let mut remaining = SAMPLES_PER_ENTRY;
        let mut offset = entry_offset;
        
        while remaining > 0 {
            let chunk = remaining.min(CHUNK_SIZE);
            let samples = core::slice::from_raw_parts(audio_virt.add(offset), chunk);
            
            // Convert i16 to bytes for EventBus
            let mut audio_buf = alloc::vec::Vec::with_capacity(chunk * 2);
            for &s in samples {
                audio_buf.extend_from_slice(&s.to_le_bytes());
            }
            
            let _ = crate::globals::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from("AUDIO_IN"),
                payload: audio_buf,
                token: CapabilityToken::Legacy(1),
            });
            
            offset += chunk;
            remaining -= chunk;
        }
        
        // Advance RPI
        let next_rpi = (rpi + 1) % 16;
        HDA_SD0_RPI.store(next_rpi, Ordering::Release);
    }
    
    // Acknowledge global interrupt (write 1 to clear)
    w32(bar, HDA_INTSTS, intsts);
}

// ============================================================================
// Public API
// ============================================================================

/// Initialize HDA controller: PCI discovery, CORB/RIRB, codec enumeration, SD0 setup.
/// Returns true on success.
pub fn init_hda() -> bool {
    if HDA_INIT_DONE.load(Ordering::Acquire) {
        return true;
    }
    
    slog_nano!("HDA", "info", "Initializing Intel HDA capture driver...");
    
    // Scan PCI for HDA controller (class 0x04, subclass 0x03)
    let devices = unsafe { pci::scan_pci() };
    let mut hda_dev = None;
    
    for dev in &devices {
        if dev.class == 0x04 && dev.subclass == 0x03 {
            hda_dev = Some(*dev);
            break;
        }
    }
    
    let dev = match hda_dev {
        Some(d) => d,
        None => {
            slog_nano!("HDA", "warn", "No Intel HDA controller found");
            return false;
        }
    };
    
    slog_nano!("HDA", "info", "Found HDA: {:04x}:{:04x} bus={} dev={} fn={} BAR0={:#x} IRQ={}",
        dev.vendor_id, dev.device_id, dev.bus, dev.device, dev.function, dev.bar0, dev.prog_if);
    
    // Enable PCI Bus Master + Memory Space
    unsafe { pci::enable_pci_bus_master(&dev); }
    
    // Get physical memory offset
    let pm_off = PHYS_MEM_OFFSET.load(Ordering::Acquire);
    if pm_off == 0 {
        slog_nano!("HDA", "error", "PHYS_MEM_OFFSET not set");
        return false;
    }
    
    // Map BAR0 MMIO as uncacheable
    let bar_phys = dev.bar0 & !0xF;
    let bar = bar_phys + pm_off;
    
    // Map all pages of BAR0 (typically 16KB = 4 pages)
    for i in 0..4 {
        unsafe { map_page_uc(bar_phys + i * 4096, pm_off); }
    }
    
    HDA_BAR.store(bar, Ordering::Release);
    
    // Reset controller
    unsafe {
        w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) | GCTL_CRST);
        for _ in 0..10000 { core::hint::spin_loop(); }
        w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) & !GCTL_CRST);
        for _ in 0..20000 { core::hint::spin_loop(); }
        
        // Verify controller is out of reset
        let gctl = r32(bar, HDA_GCTL);
        if gctl & GCTL_CRST != 0 {
            slog_nano!("HDA", "error", "Controller reset failed");
            return false;
        }
        
        // Enable unsolicited responses
        w32(bar, HDA_GCTL, gctl | GCTL_UNSOL);
        
        // Initialize CORB/RIRB
        if !init_corb_rirb(bar) {
            return false;
        }
        
        // Enumerate codecs and discover widgets
        if !enumerate_codecs(bar) {
            slog_nano!("HDA", "warn", "No codecs found");
            return false;
        }
        
        // Configure capture path (mic pin + ADC)
        if !configure_capture_path(bar) {
            slog_nano!("HDA", "warn", "Failed to configure capture path");
            return false;
        }
        
        // Initialize SD0 capture stream
        if !init_sd0_capture(bar) {
            return false;
        }
        
        // Enable global interrupts
        w32(bar, HDA_INTCTL, 0xFFFF_FFFF); // Enable all stream interrupts
        w32(bar, HDA_INTCTL, r32(bar, HDA_INTCTL) | 1); // Global interrupt enable
    }
    
    HDA_INIT_DONE.store(true, Ordering::Release);
    slog_nano!("HDA", "info", "Intel HDA capture driver initialized successfully");
    true
}

/// Poll HDA audio (compatibility function for non-IRQ path).
/// Reads completed BDL entries and publishes to EventBus.
pub fn poll_hda_audio() {
    if !HDA_INIT_DONE.load(Ordering::Acquire) {
        return;
    }
    
    let bar = HDA_BAR.load(Ordering::Acquire);
    if bar == 0 {
        return;
    }
    
    unsafe {
        let sd0_sts_off = SD0_BASE + SDX_STS;
        let sd0_sts = r16(bar, sd0_sts_off);
        
        if sd0_sts & SD_STS_BCIS as u16 != 0 {
            // Clear interrupt
            w16(bar, sd0_sts_off, sd0_sts | SD_STS_BCIS as u16);
            
            // Process completed entries (same as IRQ handler but without global INTSTS)
            let rpi = HDA_SD0_RPI.load(Ordering::Acquire);
            let audio_phys = HDA_SD0_BUF.load(Ordering::Acquire);
            let audio_virt = (audio_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *const i16;
            
            const SAMPLES_PER_ENTRY: usize = 2048;
            const CHUNK_SIZE: usize = 512;
            
            for _ in 0..16 {
                let entry_idx = rpi % 16;
                let entry_offset = (entry_idx as usize) * SAMPLES_PER_ENTRY;
                
                let mut remaining = SAMPLES_PER_ENTRY;
                let mut offset = entry_offset;
                
                while remaining > 0 {
                    let chunk = remaining.min(CHUNK_SIZE);
                    let samples = core::slice::from_raw_parts(audio_virt.add(offset), chunk);
                    
                    let mut audio_buf = alloc::vec::Vec::with_capacity(chunk * 2);
                    for &s in samples {
                        audio_buf.extend_from_slice(&s.to_le_bytes());
                    }
                    
                    let _ = crate::globals::EVENT_BUS.publish(Event {
                        id: 0,
                        topic: alloc::string::String::from("AUDIO_IN"),
                        payload: audio_buf,
                        token: CapabilityToken::Legacy(1),
                    });
                    
                    offset += chunk;
                    remaining -= chunk;
                }
                
                let next_rpi = (rpi + 1) % 16;
                HDA_SD0_RPI.store(next_rpi, Ordering::Release);
            }
        }
    }
}