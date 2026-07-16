//! Raw framebuffer — BGRA32 pixel writer + embedded-graphics DrawTarget.
//! Suporta UEFI GOP (hardware real) e VirtIO-GPU (QEMU).
//! Double buffering: todas as operacoes vao para back buffer, swap() copia para tela.

use core::ptr::write_volatile;
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
        let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
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
                    k_nano::serial_println!("[DISPLAY] Intel VGA plane DISABLED via VGACNTRL ({}:{}.{})",
                        igpu.pci_bus, igpu.pci_dev, igpu.pci_fn);
                } else {
                    k_nano::serial_println!("[DISPLAY] Intel VGA plane ja desligado");
                }
            }
        } else {
            k_nano::serial_println!("[DISPLAY] Intel GPU com display nao encontrada - VGACNTRL nao escrito");
        }

        // Sfence + barreira de escrita garantem visibilidade
        unsafe {
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }
        k_nano::serial_println!("[DISPLAY] FB sfence aplicado @{:x} ({}x{})",
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
            bootloader_api::info::PixelFormat::Bgr => { k_nano::serial_println!("[DISPLAY] PixelFormat: Bgr (B primeiro)"); 3u32 },
            bootloader_api::info::PixelFormat::Rgb => { k_nano::serial_println!("[DISPLAY] PixelFormat: Rgb (R primeiro) — invertendo canais!"); 3u32 },
            _ => { k_nano::serial_println!("[DISPLAY] PixelFormat: {} — assumindo 4 bytes/pixel", pixel_name); 4u32 },
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
            k_nano::serial_println!("[DISPLAY] ALERTA: stride {} pode exceder buffer ({} bytes, esperado min {})",
                fb_stride, fb_buf_len, expected_min);
        }

        // Log das resolucoes suportadas pelo hardware (Intel 6xx Gen9):
        // 1920x1080, 1366x768, 1280x720, 1024x768, 800x600
        k_nano::serial_println!("[DISPLAY] UEFI fb: {}x{} bpp={} stride={}({}px) buf={} @{:x}",
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

        k_nano::serial_println!("[DISPLAY] UEFI framebuffer configurado: {}x{} bpp={} stride={} @{:x}",
            gpu.fb_width, gpu.fb_height, bpp, gpu.fb_stride, gpu.fb_addr);
    } else {
        k_nano::serial_println!("[DISPLAY] Sem framebuffer UEFI — VGA text mode.");
    }
}

/// Pinta resposta TTS/LLM no FB (antes do scheduler / DisplayAgent).
pub fn paint_tts_response(text: &str) {
    let guard = GPU.lock();
    let Some(gpu) = guard.as_ref() else {
        k_nano::serial_println!("[JARBAS-TTS-FB] skip — sem FB");
        return;
    };
    if !gpu.present || gpu.fb_addr == 0 {
        k_nano::serial_println!("[JARBAS-TTS-FB] skip — FB nao present");
        return;
    }
    let bpp = gpu.fb_bpp as usize;
    let stride = gpu.fb_stride as usize;
    let w = gpu.fb_width as usize;
    let h = gpu.fb_height as usize;
    let addr = gpu.fb_addr as usize;
    let rgb = gpu.rgb_order;
    let (tr, tg, tb) = (10u8, 10u8, 15u8);
    let (c0, c1, c2) = if rgb { (tr, tg, tb) } else { (tb, tg, tr) };
    let clear_size = h.saturating_mul(stride);
    unsafe {
        let ptr = addr as *mut u8;
        if bpp == 4 {
            let pix = u32::from_le_bytes([c0, c1, c2, 0xFF]);
            let mut i = 0usize;
            while i + 4 <= clear_size {
                write_volatile(ptr.add(i) as *mut u32, pix);
                i += 4;
            }
        } else if bpp > 0 {
            for y in 0..h {
                for x in 0..w {
                    let off = y * stride + x * bpp;
                    if off + 2 >= clear_size {
                        continue;
                    }
                    write_volatile(ptr.add(off), c0);
                    write_volatile(ptr.add(off + 1), c1);
                    write_volatile(ptr.add(off + 2), c2);
                    if bpp > 3 {
                        write_volatile(ptr.add(off + 3), 0xFF);
                    }
                }
            }
        }
    }
    let fb_addr = addr;
    let fb_w = w;
    let fb_h = h;
    let fb_stride = stride;
    let fb_bpp = bpp;
    let rgb_order = gpu.rgb_order;
    drop(guard);

    splash_draw_text(fb_addr, fb_w, fb_h, fb_stride, fb_bpp, rgb_order, 16, 16, "[JARBAS TTS]");
    let max_cols = ((fb_w.saturating_sub(32)) / 8).min(72).max(8);
    let mut y = 40usize;
    let mut col = 0usize;
    let mut line = alloc::string::String::new();
    for c in text.chars() {
        let wrap = c == '\n' || col >= max_cols;
        if wrap {
            if !line.is_empty() {
                splash_draw_text(fb_addr, fb_w, fb_h, fb_stride, fb_bpp, rgb_order, 16, y, &line);
                line.clear();
            }
            y = y.saturating_add(18);
            col = 0;
            if y + 18 > fb_h {
                break;
            }
            if c == '\n' {
                continue;
            }
        }
        if c != '\n' {
            line.push(c);
            col += 1;
        }
    }
    if !line.is_empty() && y + 16 <= fb_h {
        splash_draw_text(fb_addr, fb_w, fb_h, fb_stride, fb_bpp, rgb_order, 16, y, &line);
    }
    k_nano::serial_println!(
        "[JARBAS-TTS-FB] painted len={} {}x{}",
        text.len(),
        w,
        h
    );
}

/// Splash pós-boot/demo: fundo temático (OrbBackground) + label (ex: "AIOS").
pub fn boot_splash(msg: &str) {
    let guard = GPU.lock();
    let Some(gpu) = guard.as_ref() else {
        k_nano::serial_println!("[DISPLAY] splash skip — sem FB");
        return;
    };
    if !gpu.present || gpu.fb_addr == 0 {
        return;
    }
    let bpp = gpu.fb_bpp as usize;
    let stride = gpu.fb_stride as usize;
    let w = gpu.fb_width as usize;
    let h = gpu.fb_height as usize;
    let addr = gpu.fb_addr as usize;
    let rgb = gpu.rgb_order;
    let (tr, tg, tb) = (10u8, 10u8, 15u8);
    let (c0, c1, c2) = if rgb { (tr, tg, tb) } else { (tb, tg, tr) };
    let clear_size = h.saturating_mul(stride);
    unsafe {
        let ptr = addr as *mut u8;
        if bpp == 4 {
            let pix = u32::from_le_bytes([c0, c1, c2, 0xFF]);
            let mut i = 0usize;
            while i + 4 <= clear_size {
                write_volatile(ptr.add(i) as *mut u32, pix);
                i += 4;
            }
        } else if bpp > 0 {
            for y in 0..h {
                for x in 0..w {
                    let off = y * stride + x * bpp;
                    if off + 2 >= clear_size {
                        continue;
                    }
                    write_volatile(ptr.add(off), c0);
                    write_volatile(ptr.add(off + 1), c1);
                    write_volatile(ptr.add(off + 2), c2);
                    if bpp > 3 {
                        write_volatile(ptr.add(off + 3), 0xFF);
                    }
                }
            }
        }
    }
    let fb_addr = addr;
    let fb_w = w;
    let fb_h = h;
    let fb_stride = stride;
    let fb_bpp = bpp;
    let rgb_order = gpu.rgb_order;
    drop(guard);
    splash_draw_text(fb_addr, fb_w, fb_h, fb_stride, fb_bpp, rgb_order, 16, 16, msg);
    k_nano::serial_println!("[DISPLAY] splash '{}' {}x{} bpp={}", msg, w, h, bpp);
}

fn splash_draw_text(
    fb_addr: usize,
    width: usize,
    height: usize,
    stride: usize,
    bpp: usize,
    rgb_order: bool,
    x0: usize,
    y0: usize,
    text: &str,
) {
    if bpp == 0 {
        return;
    }
    let ch = 16usize;
    let cw = 8usize;
    let mut x = x0;
    let y = y0;
    let (c0, c1, c2) = if rgb_order {
        (0u8, 200u8, 255u8)
    } else {
        (255u8, 200u8, 0u8)
    };
    for c in text.chars() {
        if x + cw > width || y + ch > height {
            break;
        }
        if let Some(bitmap) = crate::display::font::get_char_bitmap(c) {
            for dy in 0..ch.min(16) {
                let row = bitmap[dy];
                for dx in 0..cw.min(8) {
                    let off = (y + dy) * stride + (x + dx) * bpp;
                    if off + 2 >= height * stride {
                        continue;
                    }
                    if (row >> (7 - dx)) & 1 == 1 {
                        unsafe {
                            write_volatile((fb_addr + off) as *mut u8, c0);
                            write_volatile((fb_addr + off + 1) as *mut u8, c1);
                            write_volatile((fb_addr + off + 2) as *mut u8, c2);
                            if bpp > 3 {
                                write_volatile((fb_addr + off + 3) as *mut u8, 0xFF);
                            }
                        }
                    }
                }
            }
        }
        x += cw;
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


