use core::fmt;
use alloc::format;
use alloc::string::String;
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

/// Boot log buffer: circular buffer mantido em RAM para dump posterior.
pub struct BootLog {
    buf: [u8; 16384],
    pos: usize,
}

impl BootLog {
    pub fn write(&mut self, data: &[u8]) {
        for &b in data {
            self.buf[self.pos % self.buf.len()] = b;
            self.pos += 1;
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

pub static BOOT_LOG: Mutex<BootLog> = Mutex::new(BootLog { buf: [0u8; 16384], pos: 0 });

pub fn _print(args: fmt::Arguments) {
    use fmt::Write;
    let msg = alloc::format!("{}", args);
    SERIAL.lock().write_str(&msg).unwrap();
    BOOT_LOG.lock().write(msg.as_bytes());
}

#[macro_export]
macro_rules! serial_print { ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*))); }

#[macro_export]
macro_rules! serial_println { () => ($crate::serial_print!("\n")); ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*))); }
