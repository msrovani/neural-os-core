// C5: thin facade — canonical in k_nano::vga_buffer (text mode) + jarbas::display::fb (fb_print/_print)
// Bin delegates via `pub use`; print!/_print bridge lives in jarbas::display::fb (lesson 261).
pub use jarbas_crate::display::fb::{_print, fb_print};
pub use k_nano::vga_buffer::{clear_physical_buffer, clear_vga_buffer, disable_vga_plane, hide_cursor, init, Color, Writer, WRITER};
