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
const HDA_RIRB_CTL: u64 = 0x5C;
const HDA_ICW: u64 = 0x60;
const HDA_ICR: u64 = 0x64;

// SD0 = capture (microfone)
const SD0_CTL: u64 = 0x80;
const SD0_STS: u64 = 0x84;
const SD0_FIFOS: u64 = 0x94;
const SD0_LVI: u64 = 0x96;
const SD0_CBL: u64 = 0x98;
const SD0_FORMAT: u64 = 0x92;
const SD0_BDLPL: u64 = 0xA0;
const SD0_BDLPU: u64 = 0xA4;

// SD1 = playback (auto-falante)
const SD1_CTL: u64 = 0xA0;
const SD1_STS: u64 = 0xA4;
const SD1_FIFOS: u64 = 0xB4;
const SD1_LVI: u64 = 0xB6;
const SD1_CBL: u64 = 0xB8;
const SD1_FORMAT: u64 = 0xB2;
const SD1_BDLPL: u64 = 0xC0;
const SD1_BDLPU: u64 = 0xC4;

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
            k_nano::slog_jarbas!("Audio", "hda", "Intel HDA ativo — captura SD0 + playback SD1");
        } else {
            k_nano::slog_jarbas!("Audio", "hda", "Nenhum controlador Intel HDA encontrado");
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
            let bar = (dev.bar0 as u64 & !0xF) + k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
            k_nano::slog_jarbas!("Audio", "hda", "Intel HDA: {:04x}:{:04x} BAR0={:#x}", dev.vendor_id, dev.device_id, dev.bar0);

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

            for cad in 0..8u32 {
                w32(bar, HDA_ICW, (cad << 28) | (0x0F << 20) | 0xF00);
                for _ in 0..5000 { core::hint::spin_loop(); if r32(bar, HDA_ICW) & 0x80000000 != 0 { break; } }
                let resp = r32(bar, HDA_ICR);
                if resp != 0 && resp != 0xFFFFFFFF {
                    k_nano::slog_jarbas!("Audio", "hda", "Codec {}: vendor={:#08x}", cad, resp);

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

                    k_nano::slog_jarbas!("Audio", "hda", "Capture SD0 @ 0x{:x} + Playback SD1 @ 0x{:x}", CAPTURE_BUF, PLAYBACK_BUF);
                    HDA_BAR.store(bar, Ordering::Relaxed);
                }
            }
            HDA_INIT_DONE.store(true, Ordering::Relaxed);
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
/// Chamado pelo AudioMixerAgent quando ha dados no AUDIO_RING.
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
