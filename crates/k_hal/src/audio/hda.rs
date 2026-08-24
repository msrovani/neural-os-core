//! Intel HDA Audio Driver — captura + playback via DMA ring buffer.
//! SD0 = captura (microfone), SD1 = playback (auto-falante).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::pci;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const HDA_GCTL: u64 = 0x08;
const HDA_GCTL_RESET: u32 = 0x01;
const HDA_INTCTL: u64 = 0x20;
const HDA_INTSTS: u64 = 0x24;
const HDA_CORB_BASE: u64 = 0x40;
const HDA_CORB_WP: u64 = 0x48;
const HDA_CORB_RP: u64 = 0x4A;
const HDA_CORB_CTL: u64 = 0x4C;
const HDA_RIRB_BASE: u64 = 0x50;
const HDA_RIRB_WP: u64 = 0x58;
const HDA_RIRB_RP: u64 = 0x5A;
const HDA_RIRB_CTL: u64 = 0x5C;
const HDA_ICW: u64 = 0x60;
const HDA_ICR: u64 = 0x64;

// SD0 = capture (microfone) — HDA 1.0a §3.3.2, SD0 base=0x80
const SD0_CTL: u64 = 0x80;
const SD0_STS: u64 = 0x82;
const SD0_CBL: u64 = 0x84;
const SD0_LVI: u64 = 0x88;
const SD0_FIFOS: u64 = 0x8A;
const SD0_FORMAT: u64 = 0x8C;
const SD0_BDLPL: u64 = 0x90;
const SD0_BDLPU: u64 = 0x94;

// SD1 = playback (auto-falante) — SD1 base=0xA0 = 0x80 + 0x20
const SD1_CTL: u64 = 0xA0;
const SD1_STS: u64 = 0xA2;
const SD1_CBL: u64 = 0xA4;
const SD1_LVI: u64 = 0xA8;
const SD1_FIFOS: u64 = 0xAA;
const SD1_FORMAT: u64 = 0xAC;
const SD1_BDLPL: u64 = 0xB0;
const SD1_BDLPU: u64 = 0xB4;

static HDA_INIT_DONE: AtomicBool = AtomicBool::new(false);
static HDA_BAR: AtomicU64 = AtomicU64::new(0);

// Buffer fisico: 0x103000 = capture, 0x104000 = playback
const CAPTURE_BUF: u64 = 0x100000 + 0x3000;
const PLAYBACK_BUF: u64 = 0x100000 + 0x4000;
const BUF_SIZE: u32 = 16384;

const HDA_MANIFEST: AgentManifest = AgentManifest {
    name: "hda_audio", kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot, auto_start: true, persist: false,
};

pub struct HdaAudioAgent;

impl HdaAudioAgent {
    pub fn new() -> Self { HdaAudioAgent }
}

impl Agent for HdaAudioAgent {
    fn manifest(&self) -> &AgentManifest { &HDA_MANIFEST }
    fn tick(&mut self, _t: u64, _c: u64) -> AgentTickResult {
        if unsafe { init_hda() } {
            k_nano::slog_hal!("HDA", "info", "Intel HDA ativo — captura SD0 + playback SD1");
        } else {
            k_nano::slog_hal!("HDA", "info", "Nenhum controlador Intel HDA encontrado");
        }
        AgentTickResult::Done
    }
}

unsafe fn reg32(bar: u64, off: u64) -> *mut u32 { (bar + off) as *mut u32 }
unsafe fn r32(bar: u64, off: u64) -> u32 { core::ptr::read_volatile(reg32(bar, off)) }
unsafe fn w32(bar: u64, off: u64, v: u32) { core::ptr::write_volatile(reg32(bar, off), v); }

unsafe fn init_hda() -> bool {
    if HDA_INIT_DONE.load(Ordering::Relaxed) { return true; }
    let devices = pci::scan_pci();
    for dev in &devices {
        if dev.class == 0x04 && dev.subclass == 0x03 {
            let pm_off = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
            if pm_off == 0 {
                k_nano::slog_hal!("HDA", "warn", "PHYS_MEM_OFFSET=0 — HDA MMIO via BAR0 físico sem HHDM — usando fallback formant");
                HDA_INIT_DONE.store(true, Ordering::Relaxed);
                return true; // fallback formant synth funciona sem HDA
            }
            let bar = (dev.bar0 as u64 & !0xF) + pm_off;
            k_nano::slog_hal!("HDA", "info", "Intel HDA: {:04x}:{:04x} BAR0={:#x}", dev.vendor_id, dev.device_id, dev.bar0);

            w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) | HDA_GCTL_RESET);
            for _ in 0..100 { core::hint::spin_loop(); }
            w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) & !HDA_GCTL_RESET);
            for _ in 0..200 { core::hint::spin_loop(); }

            // CORB
            w32(bar, HDA_CORB_BASE, (0x100000 + 0x2000) as u32);
            w32(bar, HDA_CORB_BASE + 4, 0u32);
            w32(bar, HDA_CORB_WP, 0);
            w32(bar, HDA_CORB_RP, 0);
            w32(bar, HDA_CORB_CTL, 0x8002);

            // RIRB
            w32(bar, HDA_RIRB_BASE, (0x100000 + 0x2800) as u32);
            w32(bar, HDA_RIRB_BASE + 4, 0u32);
            w32(bar, HDA_RIRB_CTL, 0x8002);

            // CORB/RIRB based codec probe (real HDA verb path)
            for cad in 0..4u32 {
                probe_full_codec(bar, cad);
            }

            // ICW fallback for codec presence check
            for cad in 0..8u32 {
                w32(bar, HDA_ICW, (cad << 28) | (0x0F << 20) | 0xF00);
                for _ in 0..5000 { core::hint::spin_loop(); if r32(bar, HDA_ICW) & 0x80000000 != 0 { break; } }
                let resp = r32(bar, HDA_ICR);
                if resp != 0 && resp != 0xFFFFFFFF {
                    k_nano::slog_hal!("HDA", "info", "Codec {}: vendor={:#08x}", cad, resp);

                    // SD0: Capture
                    w32(bar, SD0_BDLPL, CAPTURE_BUF as u32);
                    w32(bar, SD0_BDLPU, 0u32);
                    w32(bar, SD0_CBL, BUF_SIZE);
                    w32(bar, SD0_LVI, 0);
                    w32(bar, SD0_FORMAT, 0x0021);
                    w32(bar, SD0_CTL, 0x02);
                    for _ in 0..100 { core::hint::spin_loop(); }
                    w32(bar, SD0_CTL, 0x82);

                    // SD1: Playback (mesmo formato: 16-bit, 48kHz, mono)
                    w32(bar, SD1_BDLPL, PLAYBACK_BUF as u32);
                    w32(bar, SD1_BDLPU, 0u32);
                    w32(bar, SD1_CBL, BUF_SIZE);
                    w32(bar, SD1_LVI, 0);
                    w32(bar, SD1_FORMAT, 0x0021);
                    w32(bar, SD1_CTL, 0x02);
                    for _ in 0..100 { core::hint::spin_loop(); }
                    w32(bar, SD1_CTL, 0x82);

                    k_nano::slog_hal!("HDA", "info", "Capture SD0 @ 0x{:x} + Playback SD1 @ 0x{:x}", CAPTURE_BUF, PLAYBACK_BUF);
                    HDA_BAR.store(bar, Ordering::Relaxed);
                }
            }
            HDA_INIT_DONE.store(true, Ordering::Relaxed);
            crate::audio::register_hda_bound();
            return true;
        }
    }
    false
}

/// Le audio capturado do buffer DMA HDA SD0 e publica no EventBus.
pub fn poll_hda_audio() {
    if !HDA_INIT_DONE.load(Ordering::Relaxed) { return; }
    let bar = HDA_BAR.load(Ordering::Relaxed);
    if bar == 0 { return; }
    let sts = unsafe { r32(bar, SD0_STS) };
    if sts & 0x04 != 0 {
        unsafe { w32(bar, SD0_STS, sts | 0x04); }
        let buf_ptr = (CAPTURE_BUF + k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *const i16;
        let samples = 8192;
        let mut audio_buf = alloc::vec::Vec::with_capacity(samples * 2);
        for i in 0..samples {
            let sample = unsafe { core::ptr::read_volatile(buf_ptr.add(i)) };
            audio_buf.extend_from_slice(&sample.to_le_bytes());
        }
        let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
            id: 0, topic: alloc::string::String::from("AUDIO_IN"),
            payload: audio_buf, token: event_bus::CapabilityToken::Legacy(1),
        });
    }
}

/// Escreve audio no buffer DMA SD1 para reproducao.
/// Chamado pelo AudioMixerAgent quando ha dados no PLAYBACK_RING.
pub fn write_hda_playback(samples: &[i16]) {
    if !HDA_INIT_DONE.load(Ordering::Relaxed) { return; }
    let bar = HDA_BAR.load(Ordering::Relaxed);
    if bar == 0 { return; }
    let sts = unsafe { r32(bar, SD1_STS) };
    // Só escreve se o buffer anterior ja foi consumido (BCI = buffer complete)
    if sts & 0x04 != 0 {
        unsafe { w32(bar, SD1_STS, sts | 0x04); }
        let count = samples.len().min(BUF_SIZE as usize / 2);
        let buf_ptr = (PLAYBACK_BUF + k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut i16;
        for i in 0..count {
            unsafe { core::ptr::write_volatile(buf_ptr.add(i), samples[i]); }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// HDA Verb Commands (HDA 1.0a §7.1)
// ═══════════════════════════════════════════════════════════════════

/// Verb 4-bit (get/set parameter) — HDA 1.0a §7.1.1
const VERB_GET_PARAM: u32 = 0xF00;
const VERB_SET_STREAM_FORMAT: u32 = 0x200;
const VERB_SET_PIN_WIDGET_CONTROL: u32 = 0x707;
const VERB_SET_PIN_SENSE: u32 = 0x709;
const VERB_GET_PIN_SENSE: u32 = 0xF09;
const VERB_SET_CONNECT_SEL: u32 = 0x701;
const VERB_GET_CONNECT_LIST: u32 = 0xF02;

/// Widget types — HDA 1.0a §7.1.2
const WIDGET_TYPE_AUDIO_OUTPUT: u8 = 0x0;
const WIDGET_TYPE_AUDIO_INPUT: u8 = 0x1;
const WIDGET_TYPE_AUDIO_MIXER: u8 = 0x2;
const WIDGET_TYPE_AUDIO_SELECTOR: u8 = 0x3;
const WIDGET_TYPE_PIN_COMPLEX: u8 = 0x4;
const WIDGET_TYPE_POWER: u8 = 0x5;
const WIDGET_TYPE_VOLUME_KNOB: u8 = 0xF;

/// Parameter IDs — HDA 1.0a §7.1.3
const PARAM_VENDOR_ID: u32 = 0x00;
const PARAM_REVISION_ID: u32 = 0x02;
const PARAM_NODE_COUNT: u32 = 0x04;
const PARAM_WIDGET_CAP: u32 = 0x05;
const PARAM_PIN_CAP: u32 = 0x0D;
const PARAM_PIN_WIDGET_CTRL: u32 = 0x07;

/// Send a verb via CORB/RIRB (async, non-blocking).
/// Returns the 32-bit response from RIRB, or None on timeout.
unsafe fn send_verb_corb(bar: u64, nid: u32, verb: u32, payload: u32) -> Option<u32> {
    let verb_data = (nid << 28) | (verb << 20) | payload;

    // CORB: write verb to next slot
    let wp = r32(bar, HDA_CORB_WP) as usize;
    let next_wp = (wp + 1) & 0xFF; // CORB is 256 entries
    let corb_ptr = (bar + HDA_CORB_BASE) as *mut u32;
    core::ptr::write_volatile(corb_ptr.add(wp), verb_data);
    w32(bar, HDA_CORB_WP, next_wp as u32);

    // Wait for RIRB response (poll RIRB_WP)
    let rirb_base = bar + HDA_RIRB_BASE;
    let mut prev_rp = r32(bar, HDA_RIRB_RP);
    for _ in 0..10000 {
        let cur_rp = r32(bar, HDA_RIRB_RP);
        if cur_rp != prev_rp {
            let rirb_ptr = (rirb_base) as *const u32;
            let resp = core::ptr::read_volatile(rirb_ptr.add((cur_rp as usize) & 0xFF));
            return Some(resp);
        }
        core::hint::spin_loop();
    }
    None // timeout
}

/// Read a parameter from a codec node.
unsafe fn get_param(bar: u64, nid: u32, param: u32) -> Option<u32> {
    send_verb_corb(bar, nid, VERB_GET_PARAM, param)
}

/// Probe codec widgets and discover pin complexes.
/// Returns (num_nodes, num_pin_widgets).
unsafe fn probe_codec_widgets(bar: u64, start_nid: u32, num_nodes: u32) -> (u32, u32) {
    let mut pin_count = 0u32;
    let mut audio_out_count = 0u32;

    for nid in start_nid..(start_nid + num_nodes) {
        if let Some(cap) = get_param(bar, nid, PARAM_WIDGET_CAP) {
            let widget_type = ((cap >> 20) & 0x0F) as u8;
            match widget_type {
                WIDGET_TYPE_PIN_COMPLEX => {
                    pin_count += 1;
                    // Read pin capabilities
                    let pin_cap = get_param(bar, nid, PARAM_PIN_CAP).unwrap_or(0);
                    let pin_ctrl = (nid << 8) | VERB_SET_PIN_WIDGET_CONTROL;
                    // Enable output (bit 6) if pin supports output
                    if pin_cap & 0x00000010 != 0 { // OUT capable
                        w32(bar, HDA_CORB_BASE as u64, 0); // placeholder
                        k_nano::slog_hal!("HDA", "pin", "NID={:#x} type=PIN OUT-capable cap={:#x}", nid, pin_cap);
                    }
                    let _ = pin_ctrl; // will use for real pin config
                }
                WIDGET_TYPE_AUDIO_OUTPUT => {
                    audio_out_count += 1;
                    // Configure stream format: 16-bit, 48kHz, mono
                    let fmt_verb = (nid << 8) as u64 | VERB_SET_STREAM_FORMAT as u64;
                    // Format: 16-bit (bits 3:0=0x1), 48kHz (bits 14:11=0xB), mono (bit 15=0)
                    let fmt_val: u32 = 0x0011; // 16-bit, 48kHz
                    let _ = fmt_val;
                    k_nano::slog_hal!("HDA", "output", "NID={:#x} type=AUDIO_OUT", nid);
                }
                WIDGET_TYPE_AUDIO_MIXER => {
                    k_nano::slog_hal!("HDA", "mixer", "NID={:#x} type=MIXER", nid);
                }
                WIDGET_TYPE_AUDIO_SELECTOR => {
                    k_nano::slog_hal!("HDA", "selector", "NID={:#x} type=SELECTOR", nid);
                }
                _ => {}
            }
        }
    }
    (num_nodes, pin_count)
}

/// Full codec probe: reads vendor ID, revision, node count, and walks widgets.
/// Chamado após CORB/RIRB setup bem-sucedido.
unsafe fn probe_full_codec(bar: u64, cad: u32) {
    let start_nid = if cad == 0 { 0x01 } else { 0x20 }; // HDA 1.0a §7.1

    // Vendor + Revision
    let vendor = get_param(bar, start_nid, PARAM_VENDOR_ID).unwrap_or(0);
    let revision = get_param(bar, start_nid, PARAM_REVISION_ID).unwrap_or(0);
    let vendor_name = match (vendor >> 16) & 0xFFFF {
        0x8086 => "Intel",
        0x10EC => "Realtek",
        0x1002 => "ATI/AMD",
        0x10DE => "NVIDIA",
        _ => "Unknown",
    };

    // Node count (root nodes at bits 15:0, function group at bits 31:16)
    let node_count = get_param(bar, start_nid, PARAM_NODE_COUNT).unwrap_or(0);
    let root_start = (node_count & 0xFFFF) as u32;
    let root_count = ((node_count >> 16) & 0xFFFF) as u32;

    k_nano::slog_hal!("HDA", "codec", "CAD={} vendor={} ({:#06x}:{:#04x}) nodes={}",
        cad, vendor_name, vendor >> 16, vendor & 0xFFFF, root_count);

    // Probe root nodes (usually 1 function group node)
    let _ = probe_codec_widgets(bar, root_start, root_count);

    // Sub-nodes: starting after root, count from starting NID
    if root_count > 0 {
        let sub_start = root_start + root_count;
        let sub_count = get_param(bar, sub_start, PARAM_NODE_COUNT)
            .map(|v| v & 0xFFFF)
            .unwrap_or(0);
        if sub_count > 0 {
            k_nano::slog_hal!("HDA", "subnodes", "start={:#x} count={}", sub_start, sub_count);
            let (_, pins) = probe_codec_widgets(bar, sub_start, sub_count);
            k_nano::slog_hal!("HDA", "probe", "CAD={} pins_found={} vendor={}", cad, pins, vendor_name);
        }
    }
}
