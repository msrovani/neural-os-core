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

pub static BOOT_LOG: crate::sync::IrqSafeLock<BootLog> = crate::sync::IrqSafeLock::new(BootLog { buf: [0u8; 65536], pos: 0, start_tick: 0 });

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
use crate::sync::IrqSafeLock;
use uart_16550::SerialPort;

lazy_static! {
    pub static ref SERIAL: IrqSafeLock<Option<SerialPort>> = {
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
        IrqSafeLock::new(port)
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

    // 1. Formata para buffer (sem locks)
    let mut buf = [0u8; 256];
    let _ = fmt::write(&mut LogBuf(&mut buf, 0), args);
    let n = buf.iter().position(|&b| b == 0).unwrap_or(256);
    let msg = &buf[..n];

    // 2. Serial port (lock, escreve, unlock) — nunca segura lock ao chamar outros destinos
    let serial_avail = {
        let mut serial = SERIAL.lock();
        if let Some(ref mut s) = *serial {
            let mut ts_buf = [0u8; 24];
            let ts_len = format_timestamp_into(&mut ts_buf, tick);
            let _ = s.write_str(core::str::from_utf8(&ts_buf[..ts_len]).unwrap_or("[T+?] "));
            let _ = s.write_str(core::str::from_utf8(msg).unwrap_or("(invalid utf8)"));
            true
        } else { false }
    }; // SERIAL lock dropped aqui

    // 3. Boot log (lock, escreve, unlock) — SERIAL já liberado
    {
        let mut log = BOOT_LOG.lock();
        if log.start_tick == 0 { log.start_tick = tick; }
        log.write(msg, tick);
    } // BOOT_LOG lock dropped aqui

    if !serial_avail {
        // Sem serial (HW real): fallback framebuffer + disco
        // Nenhum lock retido — write_to_disk_journal pega seus próprios locks sem cadeia
        let _ = crate::vga_buffer::fb_print(args);
        write_to_disk_journal(msg, tick);
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
            let ata_guard = crate::globals::ATA_DRIVER.lock();
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

/// Log estruturado para debug IA e re-aprendizado da LLM.
/// Formato legado: [T+123][NIVEL][AGENTE][EVENTO] mensagem
/// Preferir `slog!` / `slog_hal!` etc. (`slog.rs`): `[T+n] [Rn] [k-xxx] [Item] [sub] - …`
#[macro_export]
macro_rules! klog {
    ($agent:expr, $event:expr, $fmt:tt $(,$arg:expr)*) => {
        $crate::serial::_print(format_args!(
            concat!("[{}][{}] ", $fmt, "\n"),
            $agent, $event $(,$arg)*
        ))
    };
}

/// Compacto: [T+123][LVL][AGT][EVT] msg (cabe em 80 colunas)
#[macro_export]
macro_rules! klogc {
    ($lvl:expr, $agent:expr, $event:expr, $fmt:tt $(,$arg:expr)*) => {
        $crate::serial::_print(format_args!(
            concat!("[{}][{}][{}] ", $fmt, "\n"),
            $lvl, $agent, $event $(,$arg)*
        ))
    };
}

/// Log em JSON. Parseavel por scripts, jq, IDEs, e pela LLM.
/// Uso: kjson!("BOOT","DISPLAY","fb","w",1280,"h",720,"bpp",3)
/// Output: J{"t":0,"l":"BOOT","a":"DISPLAY","e":"fb","w":1280,"h":720,"bpp":3}
/// Prefixo "J" permite filtrar linhas JSON do resto do log.
#[macro_export]
macro_rules! kjson {
    ($lvl:expr, $agent:expr, $event:expr $(, $k:expr, $v:expr)*) => {{
        let tick = $crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        $crate::serial::_print(format_args!("J{{\"t\":{},\"l\":\"{}\",\"a\":\"{}\",\"e\":\"{}\"", tick, $lvl, $agent, $event));
        $($crate::serial::_print(format_args!(",\"{}\":{}", $k, $v));)*
        $crate::serial::_print(format_args!("}}\n"));
    }}
}

