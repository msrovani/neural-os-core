use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::instructions::port::Port;

const COM2: u16 = 0x2F8;
const MTU: usize = 1500;
const TX_TIMEOUT: u32 = 500_000;

static COM2_INIT: AtomicBool = AtomicBool::new(false);

unsafe fn ensure_init() {
    if COM2_INIT.load(Ordering::Acquire) { return; }
    let mut dlab = Port::<u8>::new(COM2 + 3);
    let old = dlab.read();
    dlab.write(old | 0x80);
    Port::<u8>::new(COM2).write(1);
    Port::<u8>::new(COM2 + 1).write(0);
    dlab.write(0x03);
    Port::<u8>::new(COM2 + 2).write(0xC7);
    Port::<u8>::new(COM2 + 1).write(0x00);
    COM2_INIT.store(true, Ordering::Release);
    crate::serial_println!("[SLIP] COM2 (0x2F8) inicializada 115200 8N1");
}

unsafe fn write_byte(b: u8) -> bool {
    ensure_init();
    for _ in 0..TX_TIMEOUT {
        if Port::<u8>::new(COM2 + 5).read() & 0x20 != 0 {
            Port::<u8>::new(COM2).write(b);
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

pub unsafe fn has_data() -> bool {
    ensure_init();
    Port::<u8>::new(COM2 + 5).read() & 0x01 != 0
}

static SLIP_TX: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static SLIP_RX: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub fn slip_tx_count() -> u64 { SLIP_TX.load(core::sync::atomic::Ordering::Relaxed) }
pub fn slip_rx_count() -> u64 { SLIP_RX.load(core::sync::atomic::Ordering::Relaxed) }

pub unsafe fn send(data: &[u8]) {
    ensure_init();
    let len = data.len().min(MTU) as u16;
    let hdr = len.to_be_bytes();
    if !write_byte(hdr[0]) { return; }
    if !write_byte(hdr[1]) { return; }
    for &b in &data[..len as usize] {
        if !write_byte(b) { return; }
    }
    SLIP_TX.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

pub unsafe fn recv() -> Option<alloc::vec::Vec<u8>> {
    ensure_init();
    if !has_data() { return None; }
    let hi = Port::<u8>::new(COM2).read();
    if !has_data() { return None; }
    let lo = Port::<u8>::new(COM2).read();
    let frame_len = u16::from_be_bytes([hi, lo]) as usize;
    if frame_len == 0 || frame_len > MTU { return None; }
    let mut buf = alloc::vec![0u8; frame_len];
    for i in 0..frame_len {
        if !has_data() { return None; }
        buf[i] = Port::<u8>::new(COM2).read();
    }
    SLIP_RX.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    Some(buf)
}
