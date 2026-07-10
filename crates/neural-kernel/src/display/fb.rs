//! Raw framebuffer — BGRA32 pixel writer + embedded-graphics DrawTarget.
//! Suporta UEFI GOP (hardware real) e VirtIO-GPU (QEMU).
//! Double buffering: todas as operacoes vao para back buffer, swap() copia para tela.

use core::ptr::write_volatile;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::{Rgb888, RgbColor},
    Pixel,
};
use alloc::vec::Vec;

#[derive(Clone, Copy)]
pub struct GpuDevice {
    pub fb_addr: u64,
    pub fb_width: u32,
    pub fb_height: u32,
    pub fb_stride: u32,
    pub fb_bpp: u32,
    pub notify_addr: u64,
    pub present: bool,
    pub rgb_order: bool,
}

impl GpuDevice {
    pub const fn empty() -> Self {
        GpuDevice { fb_addr: 0, fb_width: 0, fb_height: 0, fb_stride: 0, fb_bpp: 4, notify_addr: 0, present: false, rgb_order: false }
    }
}

pub static GPU: spin::Mutex<Option<GpuDevice>> = spin::Mutex::new(None);

/// Força coerência de cache no framebuffer + desliga VGA plane Intel corretamente.
/// Em Intel Skylake+ (6xx), o VGA plane NÃO é completamente desligado pelo
/// sequenciador (0x3C4/0x3C5) — precisa escrever no register VGACNTRL (0x71400).
/// Usa o resultado de detect_all() em vez de re-escanear PCI manualmente.
/// NOTA: map_bars_uc() já mapeou BAR0 inteiro como UC antes desta função ser chamada.
/// Também aplica sfence para garantir que writes cheguem ao display controller.
pub fn fb_remap_uc() {
    let gpu = GPU.lock();
    if let Some(ref gpu_dev) = *gpu {
        if gpu_dev.fb_addr == 0 { return; }

        // Usa o detect module para encontrar Intel GPU com display engine
        // (em vez de re-escanear PCI via CF8/CFC manualmente)
        let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let intel_gpu = unsafe {
            crate::gpu::detect::detect_all().into_iter()
                .find(|g| g.vendor == crate::gpu::detect::GpuVendor::Intel && g.has_display_engine)
        };

        if let Some(ref igpu) = intel_gpu {
            // VGACNTRL = offset 0x71400 no BAR0 da GPU Intel
            // Bit 31 (VGA_DISABLE) = 1 desliga o plano VGA corretamente
            let vga_cntrl = (igpu.bar0 + 0x71400 + pmoff) as *mut u32;
            unsafe {
                let val = vga_cntrl.read_volatile();
                if val & 0x80000000 == 0 {
                    vga_cntrl.write_volatile(val | 0x80000000);
                    crate::serial_println!("[DISPLAY] Intel VGA plane DISABLED via VGACNTRL ({}:{}.{})",
                        igpu.pci_bus, igpu.pci_dev, igpu.pci_fn);
                } else {
                    crate::serial_println!("[DISPLAY] Intel VGA plane ja desligado");
                }
            }
        } else {
            crate::serial_println!("[DISPLAY] Intel GPU com display nao encontrada - VGACNTRL nao escrito");
        }

        // Sfence + barreira de escrita garantem visibilidade
        unsafe {
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }
        crate::serial_println!("[DISPLAY] FB sfence aplicado @{:x} ({}x{})",
            gpu_dev.fb_addr, gpu_dev.fb_width, gpu_dev.fb_height);
    }
}

pub fn probe_uefi_framebuffer(boot_info: &bootloader_api::BootInfo) {
    if let Some(fb) = boot_info.framebuffer.as_ref().and_then(|f| Some(f)) {
        let info = fb.info();
        let pixel_name = match info.pixel_format {
            bootloader_api::info::PixelFormat::Bgr => "BGR",
            bootloader_api::info::PixelFormat::Rgb => "RGB",
            _ => "OTHER",
        };
        let bpp = match info.pixel_format {
            bootloader_api::info::PixelFormat::Bgr => { crate::serial_println!("[DISPLAY] PixelFormat: Bgr (B primeiro)"); 3u32 },
            bootloader_api::info::PixelFormat::Rgb => { crate::serial_println!("[DISPLAY] PixelFormat: Rgb (R primeiro) — invertendo canais!"); 3u32 },
            _ => { crate::serial_println!("[DISPLAY] PixelFormat: {} — assumindo 4 bytes/pixel", pixel_name); 4u32 },
        };
        // Stride da UEFI em bytes (= pixels por linha * bytes por pixel)
        let fb_stride = info.stride as u32 * bpp;
        let fb_width = info.width as u32;
        let fb_height = info.height as u32;
        let fb_buf_len = fb.buffer().len();

        // Validacao: stride original vs tamanho real do buffer
        let expected_min = (fb_height as usize).saturating_sub(1) * fb_stride as usize
            + fb_width as usize * bpp as usize;
        if expected_min > fb_buf_len {
            crate::serial_println!("[DISPLAY] ALERTA: stride {} pode exceder buffer ({} bytes, esperado min {})",
                fb_stride, fb_buf_len, expected_min);
        }

        // Log das resolucoes suportadas pelo hardware (Intel 6xx Gen9):
        // 1920x1080, 1366x768, 1280x720, 1024x768, 800x600
        crate::serial_println!("[DISPLAY] UEFI fb: {}x{} bpp={} stride={}({}px) buf={} @{:x}",
            fb_width, fb_height, bpp, fb_stride, info.stride, fb_buf_len, fb.buffer().as_ptr() as u64);

        // Detecta ordem de bytes: PixelFormat::Rgb = R primeiro, Bgr = B primeiro
        let rgb_order = match info.pixel_format {
            bootloader_api::info::PixelFormat::Rgb => true,
            _ => false,
        };

        // NOTA: NAO remapear como UC aqui — map_page_uc() aloca frames para
        // page tables, mas o frame allocator e a IDT ainda nao foram init.
        // O remapeamento UC sera feito em fb_remap_uc(), chamado apos memory init.
        let gpu = GpuDevice {
            fb_addr: fb.buffer().as_ptr() as u64,
            fb_width,
            fb_height,
            fb_stride,
            fb_bpp: bpp,
            notify_addr: 0,
            present: true,
            rgb_order,
        };
        *GPU.lock() = Some(gpu);

        // Limpa framebuffer para preto — elimina artefatos do bootloader
        // Usando write_volatile para garantir que o UC mapping funcione
        let fb_size = gpu.fb_height as usize * gpu.fb_stride as usize;
        if fb_size > 0 {
            unsafe {
                let ptr = gpu.fb_addr as *mut u8;
                let clear_size = fb_size.min(1024 * 1024); // limpa ate 1MB (1920x1080x4 = 8.3MB)
                // Preenche com preto usando write_volatile em bursts de 8 bytes
                let mut i = 0usize;
                while i + 8 <= clear_size {
                    core::ptr::write_volatile(ptr.add(i) as *mut u64, 0);
                    i += 8;
                }
                while i < clear_size {
                    core::ptr::write_volatile(ptr.add(i), 0);
                    i += 1;
                }
            }
        }

        crate::serial_println!("[DISPLAY] UEFI framebuffer configurado: {}x{} bpp={} stride={} @{:x}",
            gpu.fb_width, gpu.fb_height, bpp, gpu.fb_stride, gpu.fb_addr);
    } else {
        crate::serial_println!("[DISPLAY] Sem framebuffer UEFI — VGA text mode.");
    }
}

#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub bpp: usize,
    pub rgb_order: bool, // true = R em offset+0 (PixelFormat::Rgb), false = B em offset+0 (Bgr)
}

/// Framebuffer com double buffering interno.
/// Todas as operacoes de pixel vao para o back buffer (Vec<u8> em heap).
/// swap() copia back → front em um unico loop, eliminando cintilacao.
pub struct DoubleBuffer {
    pub info: FramebufferInfo,
    back: Vec<u8>,
}

impl DoubleBuffer {
    pub fn new(addr: usize, width: usize, height: usize, stride: usize, bpp: usize, rgb_order: bool) -> Self {
        let size = height * stride;
        DoubleBuffer {
            info: FramebufferInfo { addr, width, height, stride, bpp, rgb_order },
            back: alloc::vec![0u8; size],
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height { return; }
        let bpp = self.info.bpp;
        let offset = y * self.info.stride + x * bpp;
        if offset + (bpp - 1) >= self.back.len() { return; }
        if self.info.rgb_order {
            self.back[offset + 0] = r;
            self.back[offset + 1] = g;
            self.back[offset + 2] = b;
        } else {
            self.back[offset + 0] = b;
            self.back[offset + 1] = g;
            self.back[offset + 2] = r;
        }
        if bpp > 3 { self.back[offset + 3] = 0xFF; }
    }

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        let bpp = self.info.bpp;
        let stride = self.info.stride;
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                let offset = y * stride + x * bpp;
                if offset + (bpp - 1) >= self.back.len() { continue; }
                if self.info.rgb_order {
                    self.back[offset + 0] = r;
                    self.back[offset + 1] = g;
                    self.back[offset + 2] = b;
                } else {
                    self.back[offset + 0] = b;
                    self.back[offset + 1] = g;
                    self.back[offset + 2] = r;
                }
                if bpp > 3 { self.back[offset + 3] = 0xFF; }
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

    pub fn draw_char(&mut self, x: usize, y: usize, char_data: &[u8], cw: usize, ch: usize, fg: (u8, u8, u8), bg: (u8, u8, u8)) {
        for dy in 0..ch {
            for dx in 0..cw {
                let alpha = char_data[dy * cw + dx];
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

    /// Copia back buffer para o framebuffer fisico (sem cintilacao).
    /// Otimizado: escreve apenas linhas modificadas se dirty tracking for implementado.
    pub fn swap(&mut self) {
        let addr = self.info.addr;
        let len = self.back.len();
        unsafe {
            let ptr = addr as *mut u8;
            for i in 0..len {
                write_volatile(ptr.add(i), self.back[i]);
            }
        }
    }
}

/// Framebuffer BGRA32 (single buffer, legado) — implementa DrawTarget<Rgb888>
pub struct Framebuffer {
    pub info: FramebufferInfo,
}

impl Framebuffer {
    pub fn new(addr: usize, width: usize, height: usize, stride: usize, bpp: usize) -> Self {
        Framebuffer { info: FramebufferInfo { addr, width, height, stride, bpp, rgb_order: false } }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height { return; }
        let bpp = self.info.bpp;
        let offset = y * self.info.stride + x * bpp;
        unsafe {
            let ptr = self.info.addr as *mut u8;
            write_volatile(ptr.wrapping_add(offset + 0), b);
            write_volatile(ptr.wrapping_add(offset + 1), g);
            write_volatile(ptr.wrapping_add(offset + 2), r);
            if bpp > 3 { write_volatile(ptr.wrapping_add(offset + 3), 0xFF); }
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
        let bpp = self.info.bpp;
        let w = self.info.width;
        let h = self.info.height;
        for Pixel(coord, color) in pixels.into_iter() {
            let x = coord.x as usize;
            let y = coord.y as usize;
            if x < w && y < h {
                let offset = y * stride + x * bpp;
                unsafe {
                    let ptr = addr as *mut u8;
                    write_volatile(ptr.wrapping_add(offset + 0), color.b());
                    write_volatile(ptr.wrapping_add(offset + 1), color.g());
                    write_volatile(ptr.wrapping_add(offset + 2), color.r());
                    if bpp > 3 { write_volatile(ptr.wrapping_add(offset + 3), 0xFF); }
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &embedded_graphics::primitives::Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let addr = self.info.addr;
        let stride = self.info.stride;
        let bpp = self.info.bpp;
        let width = self.info.width;
        let height = self.info.height;
        let x0 = area.top_left.x.max(0) as usize;
        let y0 = area.top_left.y.max(0) as usize;
        let x1 = ((area.top_left.x + area.size.width as i32).min(width as i32)).max(0) as usize;
        let y1 = ((area.top_left.y + area.size.height as i32).min(height as i32)).max(0) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = y * stride + x * bpp;
                unsafe {
                    let ptr = addr as *mut u8;
                    write_volatile(ptr.wrapping_add(offset + 0), color.b());
                    write_volatile(ptr.wrapping_add(offset + 1), color.g());
                    write_volatile(ptr.wrapping_add(offset + 2), color.r());
                    if bpp > 3 { write_volatile(ptr.wrapping_add(offset + 3), 0xFF); }
                }
            }
        }
        Ok(())
    }
}
