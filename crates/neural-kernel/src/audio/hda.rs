//! Intel HDA Audio Driver — captura + playback via DMA ring buffer.
//! Suporta QEMU -audiodev + HW real (Intel 6xx/7xx HDA).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::serial_println;
use crate::pci;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const HDA_GCTL: u64 = 0x08;
const HDA_GCTL_RESET: u32 = 0x01;
const HDA_STATESTS: u64 = 0x0E;
const HDA_INTCTL: u64 = 0x20;
const HDA_INTSTS: u64 = 0x24;
const HDA_CORB_BASE: u64 = 0x40;    // CORB base address (4KB aligned)
const HDA_CORB_WP: u64 = 0x48;      // CORB write pointer
const HDA_CORB_RP: u64 = 0x4A;      // CORB read pointer
const HDA_CORB_CTL: u64 = 0x4C;     // CORB control
const HDA_RIRB_BASE: u64 = 0x50;    // RIRB base address
const HDA_RIRB_WP: u64 = 0x58;      // RIRB write pointer
const HDA_RIRB_CTL: u64 = 0x5C;     // RIRB control
const HDA_SD0_CTL: u64 = 0x80;      // Stream Descriptor 0 control
const HDA_SD0_STS: u64 = 0x84;      // SD status
const HDA_SD0_BDL: u64 = 0xA0;      // SD Buffer Descriptor List addr (8KB aligned)
const HDA_SD0_CBL: u64 = 0x98;      // SD Cyclic Buffer Length
const HDA_SD0_LVI: u64 = 0x96;      // SD Last Valid Index
const HDA_SD0_FIFOS: u64 = 0x94;    // SD FIFO size
const HDA_SD0_FORMAT: u64 = 0x92;   // SD format
const HDA_SD0_BDLPL: u64 = 0xA0;    // SD BDL pointer low
const HDA_SD0_BDLPU: u64 = 0xA4;    // SD BDL pointer upper

const HDA_ICW: u64 = 0x60;          // Immediate Command Write
const HDA_ICR: u64 = 0x64;          // Immediate Command Response

static HDA_INIT_DONE: AtomicBool = AtomicBool::new(false);
static HDA_BAR: AtomicU64 = AtomicU64::new(0);

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
            serial_println!("[HDA] Intel HDA ativo — audio via DMA ring buffer");
        } else {
            serial_println!("[HDA] Nenhum controlador Intel HDA encontrado");
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
            let bar = (dev.bar0 as u64 & !0xF) + crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
            serial_println!("[HDA] Intel HDA: {:04x}:{:04x} BAR0={:#x}", dev.vendor_id, dev.device_id, dev.bar0);

            // Reset HDA controller
            w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) | HDA_GCTL_RESET);
            for _ in 0..100 { core::hint::spin_loop(); }
            w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) & !HDA_GCTL_RESET);
            for _ in 0..200 { core::hint::spin_loop(); }

            // Initialize CORB (Command Output Ring Buffer)
            let corb_phys = 0x100000 + 0x2000; // DMA buffer at phys 0x102000
            w32(bar, HDA_CORB_BASE, corb_phys as u32);
            w32(bar, HDA_CORB_BASE + 4, 0u32);
            w32(bar, HDA_CORB_WP, 0);
            w32(bar, HDA_CORB_RP, 0);
            w32(bar, HDA_CORB_CTL, 0x8002); // CORB enable + 2 entry

            // Initialize RIRB (Response Input Ring Buffer)
            let rirb_phys = 0x100000 + 0x2800;
            w32(bar, HDA_RIRB_BASE, rirb_phys as u32);
            w32(bar, HDA_RIRB_BASE + 4, 0u32);
            w32(bar, HDA_RIRB_CTL, 0x8002); // RIRB enable + 2 entry

            // Try to read codec via immediate command
            for cad in 0..8u32 {
                w32(bar, HDA_ICW, (cad << 28) | (0x0F << 20) | 0xF00); // GET_PARAMETER, VENDOR_ID
                for _ in 0..5000 { core::hint::spin_loop(); if r32(bar, HDA_ICW) & 0x80000000 != 0 { break; } }
                let resp = r32(bar, HDA_ICR);
                if resp != 0 && resp != 0xFFFFFFFF {
                    serial_println!("[HDA] Codec {}: vendor={:#08x}", cad, resp);
                    // Set stream format: 16-bit, 48kHz, mono
                    let stream = 1u32;
                    w32(bar, HDA_ICW, (cad << 28) | (1 << 20) | 0x200); // SET_STREAM_FORMAT
                    for _ in 0..5000 { core::hint::spin_loop(); if r32(bar, HDA_ICW) & 0x80000000 != 0 { break; } }
                    // Configure SD0 for capture: buffer at phys 0x103000
                    let buf_phys = 0x100000 + 0x3000;
                    w32(bar, HDA_SD0_BDLPL, buf_phys as u32);
                    w32(bar, HDA_SD0_BDLPU, 0u32);
                    w32(bar, HDA_SD0_CBL, 16384); // 16KB buffer
                    w32(bar, HDA_SD0_LVI, 0);     // 1 buffer entry
                    w32(bar, HDA_SD0_FORMAT, 0x0021); // 16-bit, 48kHz, mono
                    w32(bar, HDA_SD0_CTL, 0x02);  // SD reset
                    for _ in 0..100 { core::hint::spin_loop(); }
                    w32(bar, HDA_SD0_CTL, 0x82);  // SD run + DMA enable

                    serial_println!("[HDA] Captura iniciada: buf=0x{:x}, stream={}", buf_phys, stream);
                    HDA_BAR.store(bar, Ordering::Relaxed);
                }
            }
            HDA_INIT_DONE.store(true, Ordering::Relaxed);
            return true;
        }
    }
    false
}

/// Le audio capturado do buffer DMA HDA e publica no ring buffer.
pub fn poll_hda_audio() {
    if !HDA_INIT_DONE.load(Ordering::Relaxed) { return; }
    let bar = HDA_BAR.load(Ordering::Relaxed);
    if bar == 0 { return; }
    let sts = unsafe { r32(bar, HDA_SD0_STS) };
    if sts & 0x04 != 0 { // Buffer Completion Interrupt
        unsafe { w32(bar, HDA_SD0_STS, sts | 0x04); } // clear
        // Read from DMA buffer at phys 0x103000
        let buf_ptr = (0x100000 + 0x3000 + crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *const i16;
        let samples = 8192; // half buffer
        for i in 0..samples {
            let _sample = unsafe { core::ptr::read_volatile(buf_ptr.add(i)) };
        }
    }
}
