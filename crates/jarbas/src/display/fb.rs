//! Raw framebuffer — pixel writer + double buffer.
//! Suporta UEFI GOP (hardware real) e VirtIO-GPU (QEMU).
//! Contrato: bpp/stride/rgb_order vêm da leitura do bootloader/GOP (ou do
//! protocolo VirtIO). Consumidores NÃO devem hardcodar 3 nem 4 — leem GpuDevice.

use core::ptr::write_volatile;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use alloc::vec::Vec;

// FASE 2: LUT Alpha Blending -- shift >>8 em vez de /255
#[inline(always)]
fn blend_lut(fg: u8, bg: u8, alpha: u8) -> u8 {
    if alpha == 0 { return bg; }
    if alpha == 255 { return fg; }
    let a = alpha as u16;
    let inv = 255 - a;
    ((fg as u16 * a + bg as u16 * inv + 128) >> 8) as u8
}

// FASE 3.3: SIN_LUT -- 256 entradas, elimina sinf/cosf em particles
const SIN_LUT: [i8; 256] = [
    0, 3, 6, 9, 12, 16, 19, 22, 25, 28, 31, 34, 37, 40, 43, 46,
    49, 51, 54, 57, 60, 63, 65, 68, 71, 73, 76, 78, 81, 83, 85, 88,
    90, 92, 94, 96, 98, 100, 102, 104, 106, 107, 109, 111, 112, 113, 115, 116,
    117, 118, 120, 121, 122, 122, 123, 124, 125, 125, 126, 126, 126, 127, 127, 127,
    127, 127, 127, 127, 126, 126, 126, 125, 125, 124, 123, 122, 122, 121, 120, 118,
    117, 116, 115, 113, 112, 111, 109, 107, 106, 104, 102, 100, 98, 96, 94, 92,
    90, 88, 85, 83, 81, 78, 76, 73, 71, 68, 65, 63, 60, 57, 54, 51,
    49, 46, 43, 40, 37, 34, 31, 28, 25, 22, 19, 16, 12, 9, 6, 3,
    0, -3, -6, -9, -12, -16, -19, -22, -25, -28, -31, -34, -37, -40, -43, -46,
    -49, -51, -54, -57, -60, -63, -65, -68, -71, -73, -76, -78, -81, -83, -85, -88,
    -90, -92, -94, -96, -98, -100, -102, -104, -106, -107, -109, -111, -112, -113, -115, -116,
    -117, -118, -120, -121, -122, -122, -123, -124, -125, -125, -126, -126, -126, -127, -127, -127,
    -127, -127, -127, -127, -126, -126, -126, -125, -125, -124, -123, -122, -122, -121, -120, -118,
    -117, -116, -115, -113, -112, -111, -109, -107, -106, -104, -102, -100, -98, -96, -94, -92,
    -90, -88, -85, -83, -81, -78, -76, -73, -71, -68, -65, -63, -60, -57, -54, -51,
    -49, -46, -43, -40, -37, -34, -31, -28, -25, -22, -19, -16, -12, -9, -6, -3,
];

#[inline(always)]
pub fn sin_lut(tick: u64) -> f32 { SIN_LUT[(tick & 0xFF) as usize] as f32 / 127.0 }
#[inline(always)]
pub fn cos_lut(tick: u64) -> f32 { SIN_LUT[((tick + 64) & 0xFF) as usize] as f32 / 127.0 }

static CONSOLE_LINE: AtomicUsize = AtomicUsize::new(0);
static CONSOLE_INITED: AtomicBool = AtomicBool::new(false);
/// DisplayAgent / compositor assume o FB — console de boot (K*) deixa de pintar texto.
static GRAPHICS_OWNED: AtomicBool = AtomicBool::new(false);

/// DisplayAgent chama ao iniciar o desktop gráfico (orb/resolução nativa).
/// Só após o 1º tick do compositor — no register mid-boot o FB ainda é console
/// de diagnóstico (K* / HW sem serial, SESSION_139).
pub fn claim_graphics() {
    if GRAPHICS_OWNED.swap(true, Ordering::SeqCst) {
        return; // já claimed
    }
    CONSOLE_INITED.store(false, Ordering::Relaxed);
    // Apaga K* / TRACE residuais — orb gráfico vem no mesmo tick do compositor.
    let guard = GPU.lock();
    if let Some(ref gpu) = *guard {
        if gpu.present && gpu.fb_addr != 0 {
            clear_fb_pixels(
                gpu.fb_addr as usize,
                gpu.fb_height as usize,
                gpu.stride_bytes(),
                gpu.bytes_per_pixel(),
                gpu.rgb_order,
            );
            // Splash persistente ate o compositor assumir (LLM pode levar 6K+ ticks)
            let msg = "Neural OS Core — Inicializando...";
            let x0 = (gpu.fb_width as usize).saturating_sub(msg.len() * 8) / 2;
            let y0 = gpu.fb_height as usize / 2 - 8;
            splash_draw_text(
                gpu.fb_addr as usize,
                gpu.fb_width as usize,
                gpu.fb_height as usize,
                gpu.stride_bytes(),
                gpu.bytes_per_pixel(),
                gpu.rgb_order,
                x0, y0, msg,
            );
            // ponytail: splash_DrawText simples, reusa font 8x16 existente
            // Cursor IRQ-safe enquanto Hermes THINK bloqueia o DisplayAgent
            k_nano::interrupts::FB_ADDR.store(gpu.fb_addr, Ordering::Release);
            k_nano::interrupts::FB_STRIDE.store(gpu.fb_stride, Ordering::Release);
            k_nano::interrupts::FB_BPP.store(gpu.fb_bpp.max(3), Ordering::Release);
            k_nano::interrupts::FB_W.store(gpu.fb_width, Ordering::Release);
            k_nano::interrupts::FB_H.store(gpu.fb_height, Ordering::Release);
            k_nano::interrupts::FB_RGB_ORDER.store(gpu.rgb_order, Ordering::Release);
        }
    }
}

#[inline]
pub fn graphics_owned() -> bool {
    GRAPHICS_OWNED.load(Ordering::Relaxed)
}

#[derive(Clone, Copy)]
pub struct GpuDevice {
    pub fb_addr: u64,
    pub fb_width: u32,
    pub fb_height: u32,
    /// Stride em BYTES (= pixels_por_linha * bytes_per_pixel).
    pub fb_stride: u32,
    /// Bytes por pixel reportados pelo GOP (`info.bytes_per_pixel`) ou protocolo.
    pub fb_bpp: u32,
    pub notify_addr: u64,
    pub present: bool,
    pub rgb_order: bool,
}

impl GpuDevice {
    pub const fn empty() -> Self {
        GpuDevice { fb_addr: 0, fb_width: 0, fb_height: 0, fb_stride: 0, fb_bpp: 4, notify_addr: 0, present: false, rgb_order: false }
    }

    /// Converte o valor reportado pelo bootloader/GOP em bpp operacional.
    /// Nunca infere bpp a partir de `PixelFormat` (Bgr/Rgb ≠ 24-bit).
    pub fn resolve_bytes_per_pixel(reported: u32) -> u32 {
        match reported {
            3 | 4 => reported,
            0 => {
                k_nano::slog_jarbas!("Display", "info", "bytes_per_pixel=0 do GOP — fallback dinamico bpp=4");
                4
            }
            other if (1..=8).contains(&other) => {
                k_nano::slog_jarbas!("Display", "info", "bytes_per_pixel={} incomum — aceitando valor do GOP", other);
                other
            }
            other => {
                k_nano::slog_jarbas!("Display", "info", "bytes_per_pixel={} invalido — fallback dinamico bpp=4", other);
                4
            }
        }
    }

    #[inline]
    pub fn bytes_per_pixel(&self) -> usize {
        self.fb_bpp as usize
    }

    #[inline]
    pub fn stride_bytes(&self) -> usize {
        self.fb_stride as usize
    }

    /// Monta o device a partir da geometria ja resolvida (fonte = GOP/protocolo).
    pub fn from_probe(
        fb_addr: u64,
        width: u32,
        height: u32,
        stride_px: u32,
        bytes_per_pixel: u32,
        rgb_order: bool,
    ) -> Self {
        let bpp = Self::resolve_bytes_per_pixel(bytes_per_pixel);
        Self {
            fb_addr,
            fb_width: width,
            fb_height: height,
            fb_stride: stride_px.saturating_mul(bpp),
            fb_bpp: bpp,
            notify_addr: 0,
            present: true,
            rgb_order,
        }
    }
}

pub static GPU: k_nano::sync::IrqSafeLock<Option<GpuDevice>> = k_nano::sync::IrqSafeLock::new(None);

/// Força coerência de cache no framebuffer: remapeia páginas do FB como
/// Uncacheable (NO_CACHE|WRITE_THROUGH) e desliga VGA plane Intel (via k-hal R1).
/// Em Intel Skylake+ (6xx), o VGA plane NÃO é completamente desligado pelo
/// sequenciador (0x3C4/0x3C5) — VGACNTRL (0x71400) vive no BE k-hal.
/// DEVE ser chamada APÓS memory init (Phase 2+) — map_page_uc() aloca frames
/// para page tables e precisa do frame allocator pronto.
pub fn fb_remap_uc() {
    let gpu = GPU.lock();
    if let Some(ref gpu_dev) = *gpu {
        if gpu_dev.fb_addr == 0 { return; }

        // HW-6: FB pages mapped as WB by firmware — CPU writes stay in cache,
        // display controller never sees them → garbled/stale output on real HW.
        // Walk every 4K page of FB and set PTE to NO_CACHE | WRITE_THROUGH.
        let pm = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let phys = (gpu_dev.fb_addr as u64).saturating_sub(pm);
        let fb_size = (gpu_dev.fb_height as usize).saturating_mul(gpu_dev.stride_bytes());
        let pages = (fb_size + 4095) / 4096;
        let mut mapped_count = 0usize;
        for i in 0..pages {
            let page_phys = phys.saturating_add((i as u64) * 4096);
            // overflow guard
            if page_phys < phys && i > 0 { break; }
            unsafe {
                k_nano::apic::map_page_uc(page_phys, pm);
            }
            mapped_count += 1;
        }

        // HW-6: MMIO BAR UC → k-hal only (ADR-0041 Fase 2)
        unsafe {
            k_hal::gpu::backend::disable_intel_vga_plane();
        }

        // Sfence + barreira de escrita garantem visibilidade das PTE + writes
        unsafe {
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }
        k_nano::slog_jarbas!("Display", "info",
            "FB remapped as UC: {} pages @phys={:x} stride={} ({}x{})",
            mapped_count, phys, gpu_dev.stride_bytes(),
            gpu_dev.fb_width, gpu_dev.fb_height);
    }
}

/// Framebuffer Limine / raw (ADR-0065) — `pitch_bytes` = bytes por scanline; `bpp_bits` = bits/pixel.
pub fn probe_raw_framebuffer(
    addr: u64,
    width: u32,
    height: u32,
    pitch_bytes: u32,
    bpp_bits: u16,
    rgb_order: bool,
) {
    if addr == 0 || width == 0 || height == 0 {
        k_nano::slog_jarbas!("Display", "info", "Sem framebuffer raw — VGA text mode.");
        return;
    }
    let reported_bpp = ((bpp_bits as u32) + 7) / 8;
    let stride_px = if reported_bpp > 0 {
        pitch_bytes / reported_bpp
    } else {
        width
    };
    let gpu = GpuDevice::from_probe(addr, width, height, stride_px, reported_bpp, rgb_order);
    let bpp = gpu.fb_bpp;
    let fb_stride = gpu.fb_stride;
    k_nano::slog_jarbas!(
        "Display",
        "info",
        "Limine/raw fb: {}x{} bpp={} stride={} pitch={} @{:x} rgb={}",
        gpu.fb_width,
        gpu.fb_height,
        bpp,
        fb_stride,
        pitch_bytes,
        addr,
        rgb_order
    );
    *GPU.lock() = Some(gpu);
    console_clear();
    boot_ckpt(0, "probe FB ok (limine/raw)");
    k_nano::slog_jarbas!(
        "Display",
        "info",
        "Framebuffer raw configurado: {}x{} bpp={} stride={} @{:x}",
        width,
        height,
        bpp,
        fb_stride,
        addr
    );
}

/// Pinta resposta TTS/LLM no FB (antes do scheduler / DisplayAgent).
pub fn paint_tts_response(text: &str) {
    let guard = GPU.lock();
    let Some(gpu) = guard.as_ref() else {
        k_nano::slog_jarbas!("JARBAS", "TTS-FB", "skip — sem FB");
        return;
    };
    if !gpu.present || gpu.fb_addr == 0 {
        k_nano::slog_jarbas!("JARBAS", "TTS-FB", "skip — FB nao present");
        return;
    }
    let bpp = gpu.bytes_per_pixel();
    let stride = gpu.stride_bytes();
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
                    if off + bpp > clear_size {
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
    k_nano::slog_jarbas!("JARBAS", "TTS-FB", "painted len={} {}x{}",
        text.len(),
        w,
        h);
}

/// Splash pós-boot/demo: limpa FB e escreve uma linha legível.
pub fn boot_splash(msg: &str) {
    console_clear();
    console_print(msg);
}

/// Limpa o FB e zera o cursor do console de boot.
pub fn console_clear() {
    let guard = GPU.lock();
    let Some(gpu) = guard.as_ref() else {
        return;
    };
    if !gpu.present || gpu.fb_addr == 0 {
        return;
    }
    let bpp = gpu.bytes_per_pixel();
    let stride = gpu.stride_bytes();
    let h = gpu.fb_height as usize;
    let addr = gpu.fb_addr as usize;
    let rgb = gpu.rgb_order;
    clear_fb_pixels(addr, h, stride, bpp, rgb);
    drop(guard);
    CONSOLE_LINE.store(0, Ordering::Relaxed);
    CONSOLE_INITED.store(true, Ordering::Relaxed);
}

/// Uma linha no FB: limpa a faixa da linha e desenha (sem ghost/TRACE).
/// No-op no pixel buffer depois de `claim_graphics()` (desktop/orb gráficos).
pub fn console_print(text: &str) {
    let text = text.trim_end_matches(['\r', '\n']);
    if text.is_empty() {
        return;
    }
    // Route through vconsole for virtual console support
    crate::vconsole::write_to_active(text);
    crate::vconsole::write_to_active("\n");
    
    if GRAPHICS_OWNED.load(Ordering::Relaxed) {
        return;
    }
    if !CONSOLE_INITED.load(Ordering::Relaxed) {
        console_clear();
    }
    // SAFETY: spin::Mutex is NOT IRQ-safe. GPU.lock() must not be held
    // across an IRQ that might call console_print(). Currently no IRQ path
    // calls this function, but future changes must respect this constraint.
    let guard = GPU.lock();
    let Some(gpu) = guard.as_ref() else {
        return;
    };
    if !gpu.present || gpu.fb_addr == 0 {
        return;
    }
    let bpp = gpu.bytes_per_pixel();
    let stride = gpu.stride_bytes();
    let w = gpu.fb_width as usize;
    let h = gpu.fb_height as usize;
    let addr = gpu.fb_addr as usize;
    let rgb = gpu.rgb_order;
    if bpp == 0 || h < 16 {
        return;
    }
    let ch = 16usize;
    let max_lines = h / ch;
    if max_lines == 0 {
        return;
    }
    // Cursor + clear + draw sob o MESMO GPU.lock (não soltar no wrap → sem ghost).
    let mut line = CONSOLE_LINE.load(Ordering::Relaxed);
    if line >= max_lines {
        clear_fb_pixels(addr, h, stride, bpp, rgb);
        line = 0;
        CONSOLE_LINE.store(0, Ordering::Relaxed);
    }
    CONSOLE_LINE.store(line.saturating_add(1), Ordering::Relaxed);
    draw_console_line(addr, w, h, stride, bpp, rgb, line, text);
}

fn clear_fb_pixels(addr: usize, h: usize, stride: usize, bpp: usize, rgb: bool) {
    let (tr, tg, tb) = (8u8, 8u8, 12u8);
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
            let mut i = 0usize;
            while i + bpp <= clear_size {
                write_volatile(ptr.add(i), c0);
                write_volatile(ptr.add(i + 1), c1);
                write_volatile(ptr.add(i + 2), c2);
                if bpp > 3 {
                    write_volatile(ptr.add(i + 3), 0xFF);
                }
                i += bpp;
            }
        }
    }
}

fn draw_console_line(
    addr: usize,
    w: usize,
    h: usize,
    stride: usize,
    bpp: usize,
    rgb: bool,
    line: usize,
    text: &str,
) {
    let ch = 16usize;
    let cw = 8usize;
    let y0 = line * ch;
    if y0 + ch > h {
        return;
    }
    let (bg0, bg1, bg2) = if rgb {
        (8u8, 8u8, 12u8)
    } else {
        (12u8, 8u8, 8u8)
    };
    let clear_size = h * stride;
    unsafe {
        let ptr = addr as *mut u8;
        for y in y0..(y0 + ch) {
            for x in 0..w {
                let off = y * stride + x * bpp;
                if off + bpp > clear_size {
                    continue;
                }
                write_volatile(ptr.add(off), bg0);
                write_volatile(ptr.add(off + 1), bg1);
                write_volatile(ptr.add(off + 2), bg2);
                if bpp > 3 {
                    write_volatile(ptr.add(off + 3), 0xFF);
                }
            }
        }
    }
    let (fg0, fg1, fg2) = if rgb {
        (230u8, 240u8, 255u8)
    } else {
        (255u8, 240u8, 230u8)
    };
    let mut x = 4usize;
    for c in text.chars() {
        if x + cw > w {
            break;
        }
        if let Some(bitmap) = crate::display::font::get_char_bitmap(c) {
            for dy in 0..ch.min(16) {
                let row = bitmap[dy];
                for dx in 0..cw.min(8) {
                    if (row >> (7 - dx)) & 1 != 1 {
                        continue;
                    }
                    let off = (y0 + dy) * stride + (x + dx) * bpp;
                    unsafe {
                        let ptr = (addr + off) as *mut u8;
                        write_volatile(ptr, fg0);
                        write_volatile(ptr.add(1), fg1);
                        write_volatile(ptr.add(2), fg2);
                        if bpp > 3 {
                            write_volatile(ptr.add(3), 0xFF);
                        }
                    }
                }
            }
        }
        x += cw;
    }
}

/// Checkpoint de boot — uma linha no FB (sem serial duplicado no k_nano).
/// Ponytail: oportunista flush para pendrive (USB-MSC ou ATA fallback) sem hang.
pub fn boot_ckpt(n: u8, msg: &str) {
    let mut buf = [0u8; 100];
    let mut pos = 0usize;
    buf[pos] = b'K';
    pos += 1;
    if n >= 100 {
        buf[pos] = b'0' + (n / 100);
        pos += 1;
    }
    if n >= 10 {
        buf[pos] = b'0' + ((n / 10) % 10);
        pos += 1;
    }
    buf[pos] = b'0' + (n % 10);
    pos += 1;
    buf[pos] = b':';
    pos += 1;
    buf[pos] = b' ';
    pos += 1;
    for &b in msg.as_bytes() {
        if pos >= buf.len() - 1 {
            break;
        }
        buf[pos] = b;
        pos += 1;
    }
    let s = core::str::from_utf8(&buf[..pos]).unwrap_or("K?");
    // Após claim_graphics: só ramlog/serial — não pinta K* por cima do orb.
    if !GRAPHICS_OWNED.load(Ordering::Relaxed) {
        console_print(s);
    }
    k_nano::boot_ramlog::set_last_ckpt(n);
    k_nano::boot_ramlog::append(s);
    // Ponytail: tenta pendrive se já houver backend (USB-MSC ou ATA) sem bloquear.
    // try_lock + backoff → nunca hang em K22 (SMP) nem K137 (TSC).
    let _ = k_nano::boot_logger::try_flush_ramlog();
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
    pub dirty: bool,
}

impl DoubleBuffer {
    /// Constrói o double-buffer a partir do GpuDevice já probeado (fonte dinâmica).
    pub fn from_gpu(gpu: &GpuDevice) -> Self {
        let size = (gpu.fb_height as usize).saturating_mul(gpu.stride_bytes());
        DoubleBuffer {
            info: FramebufferInfo {
                addr: gpu.fb_addr as usize,
                width: gpu.fb_width as usize,
                height: gpu.fb_height as usize,
                stride: gpu.stride_bytes(),
                bpp: gpu.bytes_per_pixel(),
                rgb_order: gpu.rgb_order,
            },
            back: alloc::vec![0u8; size],
            dirty: true,
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height { return; }
        let bpp = self.info.bpp;
        let offset = y * self.info.stride + x * bpp;
        if offset + (bpp - 1) >= self.back.len() { return; }
        self.dirty = true;
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

    /// Bresenham line (set_pixel already bounds-checks).
    pub fn fill_rect_fast(&mut self, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height { return; }
        let x2 = (x + w).min(self.info.width);
        let y2 = (y + h).min(self.info.height);
        let aw = x2 - x;
        let ah = y2 - y;
        if aw == 0 || ah == 0 { return; }
        self.dirty = true;
        let bpp = self.info.bpp;
        let stride = self.info.stride;
        let ptr = self.back.as_mut_ptr();
        unsafe {
            if bpp == 4 {
                let pix = if self.info.rgb_order { u32::from_le_bytes([r,g,b,0xFF]) } else { u32::from_le_bytes([b,g,r,0xFF]) };
                for dy in 0..ah {
                    let off = (y+dy)*stride + x*bpp;
                    let row = ptr.add(off) as *mut u32;
                    for dx in 0..(aw/4) { row.add(dx).write(pix); }
                }
            } else {
                for dy in 0..ah {
                    let base = (y+dy)*stride + x*bpp;
                    for dx in 0..aw {
                        let off = base + dx*bpp;
                        if off+3 < self.back.len() {
                            if self.info.rgb_order { ptr.add(off).write(r); ptr.add(off+1).write(g); ptr.add(off+2).write(b); }
                            else { ptr.add(off).write(b); ptr.add(off+1).write(g); ptr.add(off+2).write(r); }
                            if bpp > 3 { ptr.add(off+3).write(0xFF); }
                        }
                    }
                }
            }
        }
    }

    pub fn draw_line(&mut self, x0: isize, y0: isize, x1: isize, y1: isize, r: u8, g: u8, b: u8) {
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set_pixel(x as usize, y as usize, r, g, b);
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x += sx; }
            if e2 <= dx { err += dx; y += sy; }
        }
    }

    /// Disco com falloff radial (orb JARVIS) — resolução nativa do FB, sem texto.
    pub fn fill_circle_glow(
        &mut self,
        cx: isize,
        cy: isize,
        radius: isize,
        r: u8,
        g: u8,
        b: u8,
        alpha_pct: u8,
    ) {
        if radius <= 0 {
            return;
        }
        let r2 = (radius * radius) as i64;
        let alpha = (alpha_pct as f32 / 100.0).clamp(0.0, 1.0);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let d2 = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
                if d2 > r2 {
                    continue;
                }
                let dist = libm::sqrtf(d2 as f32);
                let falloff = (1.0 - dist / radius as f32) * alpha;
                if falloff <= 0.01 {
                    continue;
                }
                let px = cx + dx;
                let py = cy + dy;
                if px < 0 || py < 0 {
                    continue;
                }
                self.set_pixel(
                    px as usize,
                    py as usize,
                    (r as f32 * falloff) as u8,
                    (g as f32 * falloff) as u8,
                    (b as f32 * falloff) as u8,
                );
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
    /// HW-8: usa chunked copy (u64) em vez de byte-a-byte, reduzindo o numero
    /// de bus writes de ~8M para ~1M em 1920x1080x4. Otimizacao adicional
    /// (rep movsb / dirty tracking) e possivel se o perfil mostrar gargalo.

    /// Write raw byte at back buffer offset (used by client-side rendering)
    pub fn write_back_raw(&mut self, offset: usize, b: u8, g: u8, r: u8) {
        if offset + 3 < self.back.len() {
            self.dirty = true;
            if self.info.rgb_order {
                self.back[offset] = r;
                self.back[offset + 1] = g;
                self.back[offset + 2] = b;
            } else {
                self.back[offset] = b;
                self.back[offset + 1] = g;
                self.back[offset + 2] = r;
            }
            if self.info.bpp > 3 { self.back[offset + 3] = 0xFF; }
        }
    }

    /// Back buffer length in bytes
    pub fn back_len(&self) -> usize {
        self.back.len()
    }


    /// Physical address for pixel at (x, y) - for GPU DMA blit
    pub fn phys_addr_for(&self, x: usize, y: usize) -> Option<u64> {
        let offset = y * self.info.stride + x * self.info.bpp;
        if offset + self.info.bpp > self.back.len() { return None; }
        // Back buffer physical address via HHDM
        let virt = self.back.as_ptr() as u64 + offset as u64;
        let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        if pmoff == 0 { return None; } // HHDM not initialized
        Some(virt - pmoff)
    }

    /// Mark buffer dirty (external compositor wrote to it)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn swap(&mut self) {
        if !self.dirty { return; }
        // Acquire cursor lock to prevent data race with IRQ cursor draw
        while k_nano::interrupts::CURSOR_LOCK.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        let addr = self.info.addr;
        let len = self.back.len();
        unsafe {
            let src = self.back.as_ptr();
            let dst = addr as *mut u8;
            let mut i = 0usize;
            // Align destination pointer to 8 bytes
            while i < len && (dst.add(i) as usize) & 7 != 0 {
                write_volatile(dst.add(i), src.add(i).read());
                i += 1;
            }
            // Copy 8 bytes at a time
            while i + 8 <= len {
                let val = (src.add(i) as *const u64).read_unaligned();
                write_volatile(dst.add(i) as *mut u64, val);
                i += 8;
            }
            // Remaining bytes
            while i < len {
                write_volatile(dst.add(i), src.add(i).read());
                i += 1;
            }
        }
        self.dirty = false;
        k_nano::interrupts::CURSOR_LOCK.store(false, Ordering::Release);
    }
}


