use core::fmt;
use spin::Mutex;

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

unsafe fn set_cursor(pos: u16) {
    core::arch::asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Eu8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0x3D5u16, in("al") (pos >> 8) as u8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Fu8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0x3D5u16, in("al") (pos & 0xFF) as u8, options(nostack, preserves_flags));
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
    let vga_addr = (0xB8000 + physical_memory_offset) as *mut u8;
    *WRITER.lock() = Some(Writer::new(vga_addr));
}

pub fn _print(args: fmt::Arguments) {
    // Se o compositor esta ativo, nao escreve no framebuffer
    // (o compositor gerencia a tela via DoubleBuffer::swap).
    // So escreve via fb_print quando nao ha compositor.
    let comp_active = crate::display::compositor::COMPOSITOR.lock().is_some();
    if !comp_active {
        if fb_print(args) { return; }
    }
    // Fallback: VGA text mode (se inicializado)
    use fmt::Write;
    let mut w = WRITER.lock();
    if let Some(ref mut w) = *w { let _ = w.write_fmt(args); }
}

/// Escreve no framebuffer. Retorna true se conseguiu.
pub fn fb_print(args: fmt::Arguments) -> bool {
    
    let gpu = crate::display::fb::GPU.lock();
    if let Some(ref fb_gpu) = *gpu {
        let w = fb_gpu.fb_width as usize;
        let h = fb_gpu.fb_height as usize;
        let stride = fb_gpu.fb_stride as usize;
        let bpp = fb_gpu.fb_bpp as usize;
        let addr = fb_gpu.fb_addr as usize;
        if addr > 0 && w > 0 && h > 0 && bpp > 0 {
            let mut buf = [0u8; 128];
            let _ = fmt::write(&mut LogBuf(&mut buf, 0), args);
            let n = buf.iter().position(|&b| b == 0).unwrap_or(128);
            if n > 0 {
                let text = core::str::from_utf8(&buf[..n]).unwrap_or("");
                fb_write_text(addr, w, h, stride, bpp, text);
                return true;
            }
        }
    }
    false
}

/// Escreve texto no framebuffer (fallback quando serial inexistente)
fn fb_write_text(fb_addr: usize, width: usize, height: usize, stride: usize, bpp: usize, text: &str) {
    static mut LINE: usize = 0;
    let ch = 16usize; // font height
    let cw = 8usize;  // font width
    let max_lines = height / ch;
    if max_lines == 0 || bpp == 0 { return; }
    let line = unsafe { let l = LINE; LINE = (LINE + 1) % max_lines; l };
    let y = line * ch;
    let mut x = 2usize;
    for c in text.chars() {
        if x + cw > width { x = 2; }
        if let Some(bitmap) = crate::display::font::get_char_bitmap(c) {
            for dy in 0..ch.min(16) {
                let row = bitmap[dy];
                for dx in 0..cw.min(8) {
                    // Stride ja esta em bytes. Usar bpp para offset horizontal.
                    let off = (y + dy) * stride + (x + dx) * bpp;
                    if off + 2 >= height * stride { continue; }
                    if (row >> (7 - dx)) & 1 == 1 {
                        unsafe { core::ptr::write_volatile((fb_addr + off) as *mut u8, 0xCC); }
                        unsafe { core::ptr::write_volatile((fb_addr + off + 1) as *mut u8, 0xCC); }
                        unsafe { core::ptr::write_volatile((fb_addr + off + 2) as *mut u8, 0xFF); }
                    }
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
