use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

unsafe fn set_cursor(pos: u16) {
    core::arch::asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Eu8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0x3D5u16, in("al") (pos >> 8) as u8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Fu8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0x3D5u16, in("al") (pos & 0xFF) as u8, options(nostack, preserves_flags));
}

/// Evita xuvisco em Intel 6xx: NÃO escrever CRTC/0xB8000 quando há GOP.
/// NÃO setar Sequencer bit5 (Screen Off) — em HW real com GOP blanka o painel.
/// Sem serial_println: path sem-COM aloca antes do heap e trava o boot.
pub fn disable_vga_plane() {
    // no-op deliberado
}

/// Limpa o buffer fisico VGA (0xB8000) escrevendo zeros diretamente,
/// sem acessar registros CRTC. Seguro para Intel 6xx com UEFI GOP.
pub fn clear_physical_buffer(phys_offset: u64) {
    let vga = (0xB8000 + phys_offset) as *mut u8;
    unsafe { core::ptr::write_bytes(vga, 0x00, 4000); }
}

/// Esconde o cursor de texto VGA (desativa a scanline do cursor).
/// Deve ser chamado quando o framebuffer esta ativo para evitar que
/// o cursor VGA apareca sobreposto ao framebuffer.
pub unsafe fn hide_cursor() {
    // Register 0x0A = Cursor Start (bit 5 desliga o cursor)
    core::arch::asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Au8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0x3D5u16, in("al") 0x20u8, options(nostack, preserves_flags));
}

/// Limpa o buffer VGA (escreve espacos pretos em todas as posicoes).
/// Quando o framebuffer esta ativo, a camada de texto VGA ainda pode
/// estar visivel (ex: QEMU -vga std mostra ambas as camadas).
/// Limpar o buffer VGA elimina o texto branco sobreposto ao framebuffer.
pub fn clear_vga_buffer() {
    let mut writer = WRITER.lock();
    if let Some(ref mut w) = *writer {
        let blank = ScreenChar { character: b' ', color_code: ColorCode::new(Color::Black, Color::Black) };
        for row in 0..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                unsafe { core::ptr::write_volatile(&mut w.buffer.chars[row][col], blank); }
            }
        }
    }
    drop(writer);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color { Black = 0, Blue = 1, Green = 2, Cyan = 3, Red = 4, Magenta = 5, Brown = 6, LightGray = 7, DarkGray = 8, LightBlue = 9, LightGreen = 10, LightCyan = 11, LightRed = 12, Pink = 13, Yellow = 14, White = 15 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);
impl ColorCode { fn new(fg: Color, bg: Color) -> Self { ColorCode((bg as u8) << 4 | (fg as u8)) } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar { character: u8, color_code: ColorCode }

#[repr(transparent)]
struct VgaBuffer { chars: [[ScreenChar; BUFFER_WIDTH]; BUFFER_HEIGHT] }

pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut VgaBuffer,
}

impl Writer {
    fn new(addr: *mut u8) -> Self {
        Writer { column_position: 0, color_code: ColorCode::new(Color::White, Color::Black), buffer: unsafe { &mut *addr.cast() } }
    }

    fn update_cursor(&self) {
        unsafe { set_cursor(((BUFFER_HEIGHT - 1) * BUFFER_WIDTH + self.column_position) as u16); }
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH { self.new_line(); }
                unsafe { core::ptr::write_volatile(&mut self.buffer.chars[BUFFER_HEIGHT - 1][self.column_position], ScreenChar { character: byte, color_code: self.color_code }); }
                self.column_position += 1;
            }
        }
        self.update_cursor();
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() { match byte { 0x20..=0x7E | b'\n' => self.write_byte(byte), _ => self.write_byte(0xFE) } }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                unsafe { core::ptr::write_volatile(&mut self.buffer.chars[row - 1][col], self.buffer.chars[row][col]); }
            }
        }
        let blank = ScreenChar { character: b' ', color_code: self.color_code };
        for col in 0..BUFFER_WIDTH {
            unsafe { core::ptr::write_volatile(&mut self.buffer.chars[BUFFER_HEIGHT - 1][col], blank); }
        }
        self.column_position = 0;
    }
}

impl fmt::Write for Writer { fn write_str(&mut self, s: &str) -> fmt::Result { self.write_string(s); Ok(()) } }

pub static WRITER: Mutex<Option<Writer>> = Mutex::new(None);

pub fn init(physical_memory_offset: u64) {
    if physical_memory_offset == 0 {
        return; // No HHDM mapping yet — VGA not available
    }
    let vga_addr = (0xB8000 + physical_memory_offset) as *mut u8;
    *WRITER.lock() = Some(Writer::new(vga_addr));
}

pub fn _print(args: fmt::Arguments) {
    // Try framebuffer first
    if fb_print(args) { return; }
    // Fallback: VGA text mode (se inicializado e sem framebuffer)
    use fmt::Write;
    let mut w = WRITER.lock();
    if let Some(ref mut w) = *w { let _ = w.write_fmt(args); }
}

/// Escreve no framebuffer. Retorna true se conseguiu.
pub fn fb_print(_args: fmt::Arguments) -> bool {
    // ponytail: framebuffer lives in jarvis crate. When k_nano compiles alone,
    // GPU.lock() is unavailable, so fb_print always returns false.
    // In full build, jarvis populates the framebuffer and this is called.
    false
}

/// Escreve texto no framebuffer (fallback quando serial inexistente)
// ponytail: built-in 8x16 VGA font for framebuffer text output
// (avoid dependency on jarvis/display crate during standalone build)
fn get_font_char(c: char, dest: &mut [u8; 16]) {
    // Minimal built-in font: returns solid blocks for printable ASCII
    // This is a placeholder until jarvis/display is linked in full build
    for row in 0..16 {
        if c as usize >= 32 && c as usize <= 126 {
            dest[row] = if row == 0 || row == 15 { 0x00 } else { 0xFF };
        } else {
            dest[row] = 0x00;
        }
    }
}

fn fb_write_text(fb_addr: usize, width: usize, height: usize, stride: usize, bpp: usize, text: &str) {
    static LINE: AtomicUsize = AtomicUsize::new(0);
    let ch = 16usize;
    let cw = 8usize;
    let max_lines = height / ch;
    if max_lines == 0 || bpp == 0 { return; }
    let line = LINE.fetch_update(
        Ordering::Relaxed,
        Ordering::Relaxed,
        |old| Some((old + 1) % max_lines),
    ).unwrap_or(0);
    let y = line * ch;
    let mut x = 2usize;
    for c in text.chars() {
        if x + cw > width { x = 2; }
        let mut font_row = [0u8; 16];
        get_font_char(c, &mut font_row);
        for dy in 0..ch.min(16) {
            let row_bits = font_row[dy];
            for dx in 0..cw.min(8) {
                let off = (y + dy) * stride + (x + dx) * bpp;
                if off + 2 >= height * stride { continue; }
                if (row_bits >> (7 - dx)) & 1 == 1 {
                    unsafe { core::ptr::write_volatile((fb_addr + off) as *mut u8, 0xCC); }
                    unsafe { core::ptr::write_volatile((fb_addr + off + 1) as *mut u8, 0xCC); }
                    unsafe { core::ptr::write_volatile((fb_addr + off + 2) as *mut u8, 0xFF); }
                }
            }
        }
        x += cw;
    }
}

struct LogBuf<'a>(&'a mut [u8], usize);
impl<'a> fmt::Write for LogBuf<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let n = s.as_bytes().len().min(self.0.len().saturating_sub(self.1));
        self.0[self.1..self.1 + n].copy_from_slice(&s.as_bytes()[..n]);
        self.1 += n;
        Ok(())
    }
}

#[macro_export] macro_rules! print { ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*))); }
#[macro_export] macro_rules! println { () => ($crate::print!("\n")); ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*))); }
