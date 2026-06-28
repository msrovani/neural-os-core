use core::fmt;

/// Boot log circular buffer — com timestamps
pub struct BootLog {
    buf: [u8; 65536],
    pos: usize,
    start_tick: u64,
}

impl BootLog {
    pub fn write(&mut self, data: &[u8], tick: u64) {
        let elapsed = if self.start_tick == 0 { 0 } else { tick.saturating_sub(self.start_tick) };
        let secs = elapsed / 1000;
        let millis = elapsed % 1000;
        let ts: &[u8] = &[
            b'[', b'T', b'+',
            (b'0' + ((secs / 100000) % 10) as u8),
            (b'0' + ((secs / 10000) % 10) as u8),
            (b'0' + ((secs / 1000) % 10) as u8),
            (b'0' + ((secs / 100) % 10) as u8),
            (b'0' + ((secs / 10) % 10) as u8),
            (b'0' + (secs % 10) as u8),
            b'.',
            (b'0' + ((millis / 100) % 10) as u8),
            (b'0' + ((millis / 10) % 10) as u8),
            (b'0' + (millis % 10) as u8),
            b']', b' ',
        ];
        for &b in ts {
            self.buf[self.pos % self.buf.len()] = b; self.pos += 1;
        }
        for &b in data {
            self.buf[self.pos % self.buf.len()] = b; self.pos += 1;
        }
    }
    pub fn dump(&self) -> &[u8] {
        if self.pos < self.buf.len() { &self.buf[..self.pos] }
        else {
            let start = self.pos % self.buf.len();
            &self.buf[start..]
        }
    }
}

pub static BOOT_LOG: spin::Mutex<BootLog> = spin::Mutex::new(BootLog { buf: [0u8; 65536], pos: 0, start_tick: 0 });

use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    pub static ref SERIAL: Mutex<SerialPort> = {
        let mut serial = unsafe { SerialPort::new(0x3F8) };
        serial.init();
        Mutex::new(serial)
    };
}

struct LogBuf<'a>(&'a mut [u8], usize);

impl<'a> fmt::Write for LogBuf<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.0.len().saturating_sub(self.1);
        let n = bytes.len().min(remaining);
        self.0[self.1..self.1 + n].copy_from_slice(&bytes[..n]);
        self.1 += n;
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    use fmt::Write;
    let _ = SERIAL.lock().write_fmt(args);
    let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    let mut log = BOOT_LOG.lock();
    if log.start_tick == 0 { log.start_tick = tick; }
    let mut buf = [0u8; 256];
    let _ = fmt::write(&mut LogBuf(&mut buf, 0), args);
    let n = buf.iter().position(|&b| b == 0).unwrap_or(256);
    log.write(&buf[..n], tick);
}

#[macro_export]
macro_rules! serial_print { ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*))); }
#[macro_export]
macro_rules! serial_println { () => ($crate::serial_print!("\n")); ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*))); }
