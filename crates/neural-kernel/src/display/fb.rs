//! Raw framebuffer — BGRA32 pixel writer + embedded-graphics DrawTarget.
//! Suporta UEFI GOP (hardware real) e VirtIO-GPU (QEMU).

use core::ptr::write_volatile;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::{Rgb888, RgbColor},
    Pixel,
};

#[derive(Clone, Copy)]
pub struct GpuDevice {
    pub fb_addr: u64,
    pub fb_width: u32,
    pub fb_height: u32,
    pub fb_stride: u32,
    pub notify_addr: u64,
    pub present: bool,
}

impl GpuDevice {
    pub const fn empty() -> Self {
        GpuDevice { fb_addr: 0, fb_width: 0, fb_height: 0, fb_stride: 0, notify_addr: 0, present: false }
    }
}

pub static GPU: spin::Mutex<Option<GpuDevice>> = spin::Mutex::new(None);

pub fn probe_uefi_framebuffer(boot_info: &bootloader_api::BootInfo) {
    // bootloader 0.11+ expõe framebuffer UEFI via BootInfo
    // acesso direto via &boot_info.framebuffer (Option<&mut FrameBuffer>)
    if let Some(fb) = boot_info.framebuffer.as_ref().and_then(|f| Some(f)) {
        let info = fb.info();
        let gpu = GpuDevice {
            fb_addr: fb.buffer().as_ptr() as u64,
            fb_width: info.width as u32,
            fb_height: info.height as u32,
            fb_stride: info.stride as u32,
            notify_addr: 0,
            present: true,
        };
        *GPU.lock() = Some(gpu);
        crate::serial_println!("[DISPLAY] UEFI framebuffer: {}x{} stride={} @{:x}",
            gpu.fb_width, gpu.fb_height, gpu.fb_stride, gpu.fb_addr);
    } else {
        crate::serial_println!("[DISPLAY] Sem framebuffer UEFI — VGA text mode.");
    }
}

/// Informações do framebuffer obtidas do bootloader
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

/// Framebuffer BGRA32 — implementa DrawTarget<Rgb888>
pub struct Framebuffer {
    pub info: FramebufferInfo,
}

impl Framebuffer {
    pub fn new(addr: usize, width: usize, height: usize, stride: usize) -> Self {
        Framebuffer {
            info: FramebufferInfo { addr, width, height, stride },
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height { return; }
        let offset = y * self.info.stride + x * 4;
        unsafe {
            let ptr = self.info.addr as *mut u8;
            write_volatile(ptr.wrapping_add(offset + 0), b);
            write_volatile(ptr.wrapping_add(offset + 1), g);
            write_volatile(ptr.wrapping_add(offset + 2), r);
            write_volatile(ptr.wrapping_add(offset + 3), 0xFF);
        }
    }

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.set_pixel(x, y, r, g, b);
            }
        }
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
        for dy in 0..h {
            for dx in 0..w {
                self.set_pixel(x + dx, y + dy, r, g, b);
            }
        }
    }

    pub fn draw_char(&mut self, x: usize, y: usize, char_data: &[u8], w: usize, h: usize, fg: (u8, u8, u8), bg: (u8, u8, u8)) {
        for dy in 0..h {
            for dx in 0..w {
                let alpha = char_data[dy * w + dx];
                if alpha > 128 {
                    self.set_pixel(x + dx, y + dy, fg.0, fg.1, fg.2);
                } else if alpha > 0 {
                    let bg_alpha = 255 - alpha;
                    let rr = (fg.0 as u16 * alpha as u16 + bg.0 as u16 * bg_alpha as u16) / 255;
                    let gg = (fg.1 as u16 * alpha as u16 + bg.1 as u16 * bg_alpha as u16) / 255;
                    let bb = (fg.2 as u16 * alpha as u16 + bg.2 as u16 * bg_alpha as u16) / 255;
                    self.set_pixel(x + dx, y + dy, rr as u8, gg as u8, bb as u8);
                } else {
                    self.set_pixel(x + dx, y + dy, bg.0, bg.1, bg.2);
                }
            }
        }
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(self.info.width as u32, self.info.height as u32)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let addr = self.info.addr;
        let stride = self.info.stride;
        let w = self.info.width;
        let h = self.info.height;
        for Pixel(coord, color) in pixels.into_iter() {
            let x = coord.x as usize;
            let y = coord.y as usize;
            if x < w && y < h {
                let offset = y * stride + x * 4;
                unsafe {
                    let ptr = addr as *mut u8;
                    write_volatile(ptr.wrapping_add(offset + 0), color.b());
                    write_volatile(ptr.wrapping_add(offset + 1), color.g());
                    write_volatile(ptr.wrapping_add(offset + 2), color.r());
                    write_volatile(ptr.wrapping_add(offset + 3), 0xFF);
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &embedded_graphics::primitives::Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let addr = self.info.addr;
        let stride = self.info.stride;
        let width = self.info.width;
        let height = self.info.height;
        let x0 = area.top_left.x.max(0) as usize;
        let y0 = area.top_left.y.max(0) as usize;
        let x1 = ((area.top_left.x + area.size.width as i32).min(width as i32)).max(0) as usize;
        let y1 = ((area.top_left.y + area.size.height as i32).min(height as i32)).max(0) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = y * stride + x * 4;
                unsafe {
                    let ptr = addr as *mut u8;
                    write_volatile(ptr.wrapping_add(offset + 0), color.b());
                    write_volatile(ptr.wrapping_add(offset + 1), color.g());
                    write_volatile(ptr.wrapping_add(offset + 2), color.r());
                    write_volatile(ptr.wrapping_add(offset + 3), 0xFF);
                }
            }
        }
        Ok(())
    }
}
