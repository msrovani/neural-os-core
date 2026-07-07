use core::fmt;

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
        for &b in ts { self.buf[self.pos % self.buf.len()] = b; self.pos += 1; }
        for &b in data { self.buf[self.pos % self.buf.len()] = b; self.pos += 1; }
    }
    pub fn dump(&self) -> &[u8] {
        if self.pos < self.buf.len() { &self.buf[..self.pos] }
        else { &self.buf[self.pos % self.buf.len()..] }
    }
}

pub static BOOT_LOG: spin::Mutex<BootLog> = spin::Mutex::new(BootLog { buf: [0u8; 65536], pos: 0, start_tick: 0 });

/// Probes serial port: writes scratch reg, reads back. Returns true if port exists.
pub unsafe fn probe_port(port: u16) -> bool {
    let lsr: u8;
    core::arch::asm!("in al, dx", out("al") lsr, in("dx") (port + 5), options(nostack, preserves_flags, readonly));
    if lsr == 0xFF { return false; }
    core::arch::asm!("out dx, al", in("dx") (port + 7), in("al") 0x5Au8, options(nostack, preserves_flags));
    let mut check: u8;
    core::arch::asm!("in al, dx", out("al") check, in("dx") (port + 7), options(nostack, preserves_flags, readonly));
    check == 0x5A
}

use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    pub static ref SERIAL: Mutex<Option<SerialPort>> = {
        let mut port = None;
        unsafe {
            let addrs = [0x3F8u16, 0x2F8, 0x3E8, 0x2E8];
            for &addr in &addrs {
                if probe_port(addr) {
                    let mut s = SerialPort::new(addr);
                    s.init();
                    port = Some(s);
                    break;
                }
            }
        }
        Mutex::new(port)
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
    let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;

    // Tenta serial com timestamp
    let serial_avail = {
        let mut serial = SERIAL.lock();
        if let Some(ref mut s) = *serial {
            let mut ts_buf = [0u8; 24];
            let ts_len = format_timestamp_into(&mut ts_buf, tick);
            let _ = s.write_str(core::str::from_utf8(&ts_buf[..ts_len]).unwrap_or("[T+?] "));
            let _ = s.write_fmt(args);
            true
        } else { false }
    };

    // Registra no boot log (com timestamp proprio do BootLog)
    let mut log = BOOT_LOG.lock();
    if log.start_tick == 0 { log.start_tick = tick; }
    let mut buf = [0u8; 256];
    let _ = fmt::write(&mut LogBuf(&mut buf, 0), args);
    let n = buf.iter().position(|&b| b == 0).unwrap_or(256);
    log.write(&buf[..n], tick);

    if serial_avail {
        // Serial presente — log ja foi escrito acima
    } else {
        // Sem serial (HW real): escreve no disco FAT32 + fallback framebuffer
        let _ = crate::vga_buffer::fb_print(args);
        write_to_disk_journal(&buf[..n], tick);
    }
}

/// Tenta escrever no arquivo de sessao no disco FAT32 (HW real sem serial)
fn write_to_disk_journal(data: &[u8], tick: u64) {
    
    let sfn = {
        let g = crate::boot_logger::SESSION_FILENAME.lock();
        g.clone()
    };
    if let Some(ref name) = sfn {
        unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat32::read_mbr(ata);
                for part in &parts {
                    if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
                        let writer = crate::fat32::Fat32Writer::new(ata, part);
                        if let Some(w) = writer {
                            if let Some(existing) = w.reader.read_file(name) {
                                let mut new_data = existing;
                                let ts = alloc::format!("[T+{}] ", tick);
                                new_data.extend_from_slice(ts.as_bytes());
                                new_data.extend_from_slice(data);
                                if !data.ends_with(b"\n") { new_data.push(b'\n'); }
                                w.write_file(name, &new_data);
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
}

fn format_timestamp_into(buf: &mut [u8; 24], tick: u64) -> usize {
    buf[0] = b'['; buf[1] = b'T'; buf[2] = b'+';
    let mut pos = 3;
    let mut n = tick;
    let mut digits = [0u8; 20];
    let mut nd = 0;
    if n == 0 { digits[0] = b'0'; nd = 1; }
    else { while n > 0 { digits[nd] = (n % 10) as u8 + b'0'; nd += 1; n /= 10; } }
    // reverse digits
    let mut di = nd;
    while di > 0 { di -= 1; buf[pos] = digits[di]; pos += 1; }
    buf[pos] = b']'; pos += 1;
    buf[pos] = b' '; pos += 1;
    pos
}

pub fn serial_available() -> bool {
    SERIAL.lock().is_some()
}

#[macro_export] macro_rules! serial_print { ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*))); }
#[macro_export] macro_rules! serial_println { () => ($crate::serial_print!("\n")); ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*))); }

