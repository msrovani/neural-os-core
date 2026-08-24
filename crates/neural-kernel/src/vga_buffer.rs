// C5: thin facade — canonical in k_nano::vga_buffer (text mode) + jarbas::display::fb (fb_print)
// Bin delegates to canônicos via `pub use`; fb_print bridge uses jarbas console_print.

pub use k_nano::vga_buffer::{clear_physical_buffer, clear_vga_buffer, disable_vga_plane, hide_cursor, init, WRITER, Color, Writer};

// Re-export macros from k_nano (print!/println! via #[macro_export] already at crate root)
// but keep $crate resolution pointing to neural-kernel's vga_buffer::_print below.

// Override _print to keep bin's compositor-aware logic (lesson 261: avoid double-paint)
pub fn _print(args: core::fmt::Arguments) {
    let fb_active = jarbas_crate::display::fb::GPU.lock().as_ref().map(|g| g.present).unwrap_or(false);
    if fb_active {
        return;
    }
    let comp_active = jarbas_crate::display::compositor::COMPOSITOR.lock().is_some();
    if comp_active {
        return;
    }
    // Fallback: VGA text mode via k_nano WRITER
    use core::fmt::Write;
    let mut w = k_nano::vga_buffer::WRITER.lock();
    if let Some(ref mut w) = *w {
        let _ = w.write_fmt(args);
    }
}

/// Escreve no framebuffer via jarbas::display::fb::console_print (C5).
/// Retorna true se conseguiu — evita duplicar lógica de FB em dois crates.
pub fn fb_print(args: core::fmt::Arguments) -> bool {
    let has_fb = {
        let gpu = jarbas_crate::display::fb::GPU.lock();
        matches!(
            gpu.as_ref(),
            Some(g) if g.present && g.fb_addr != 0 && g.fb_width > 0 && g.fb_height > 0
        )
    };
    if !has_fb {
        return false;
    }
    // Use LogBuf pattern (160B stack) to format without alloc before heap
    use core::fmt::Write;
    struct LogBuf<'a>(&'a mut [u8], usize);
    impl<'a> Write for LogBuf<'a> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let n = s.as_bytes().len().min(self.0.len().saturating_sub(self.1));
            self.0[self.1..self.1 + n].copy_from_slice(&s.as_bytes()[..n]);
            self.1 += n;
            Ok(())
        }
    }
    let mut buf = [0u8; 160];
    let _ = core::fmt::write(&mut LogBuf(&mut buf, 0), args);
    let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    if n == 0 {
        return false;
    }
    let text = core::str::from_utf8(&buf[..n]).unwrap_or("");
    jarbas_crate::display::fb::console_print(text);
    true
}

#[macro_export]
macro_rules! print { ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*))); }

#[macro_export]
macro_rules! println { () => ($crate::print!("\n")); ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*))); }
