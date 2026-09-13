//! Raw framebuffer — pixel writer + double buffer.
//! Suporta UEFI GOP (hardware real) e VirtIO-GPU (QEMU).
//! Contrato: bpp/stride/rgb_order vêm da leitura do bootloader/GOP (ou do
//! protocolo VirtIO). Consumidores NÃO devem hardcodar 3 nem 4 — leem GpuDevice.

use core::ptr::{copy_nonoverlapping, write_volatile};
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

/// Seno quantizado i8 (-127..127) por fase 0..255 = turno completo.
/// Substitui sinf/cosf no hot path do orb (SIN_LUT existente).
#[inline(always)]
pub fn sin_q8(phase: u8) -> i8 { SIN_LUT[phase as usize] }
/// Cosseno quantizado i8 via SIN_LUT (fase + 64).
#[inline(always)]
pub fn cos_q8(phase: u8) -> i8 { SIN_LUT[((phase as usize) + 64) & 0xFF] }

/// LUT de meias-larguras de círculo: `out[radius+dy] = isqrt(radius²-dy²)`.
/// Construída uma vez por raio inteiro (cache do chamador) e reusada por
/// `fill_circle_*_spans`/`ring_spans` — elimina sqrt/trig por ponto.
/// Retorna o número de entradas escritas (2·radius+1) ou 0 se `out` for curto.
pub fn build_half_width_lut(radius: usize, out: &mut [u16]) -> usize {
    let need = radius * 2 + 1;
    if out.len() < need || radius == 0 {
        return 0;
    }
    let r2 = radius as u64 * radius as u64;
    for dy in 0..=radius {
        let hw = isqrt_u64(r2 - (dy * dy) as u64) as u16;
        out[radius + dy] = hw;
        out[radius - dy] = hw;
    }
    need
}

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
            let msg = "Neural OS Core - Inicializando...";
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
            // Libera lock legado (IRQ cursor podia sair cedo com lock=true → swap spin forever).
            k_nano::interrupts::CURSOR_LOCK.store(false, Ordering::Release);
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
/// **Write-Combining (WC)** — antes era UC (NO_CACHE|WRITE_THROUGH). WC usa a
/// entrada 4 do `IA32_PAT` (PTE bit 7 = PAT, PCD=PWT=0), o que deixa o
/// back→front sequential present muito mais rápido que UC, e desliga VGA plane
/// Intel (via k-hal R1).
/// Em Intel Skylake+ (6xx), o VGA plane NÃO é completamente desligado pelo
/// sequenciador (0x3C4/0x3C5) — VGACNTRL (0x71400) vive no BE k-hal.
/// DEVE ser chamada APÓS memory init (Phase 2+) — map_page_wc() aloca frames
/// para page tables e precisa do frame allocator pronto.
/// Se PAT/WC não estiver disponível, cai por página para `map_page_uc` (UC) —
/// nunca crasha. O ganho de WC é **metal-only** (QEMU é RAM WB comum e ignora
/// o hint — não é mensurável em QEMU).
pub fn fb_remap_uc() {
    let gpu = GPU.lock();
    if let Some(ref gpu_dev) = *gpu {
        if gpu_dev.fb_addr == 0 { return; }

        // PAT é pré-requisito do WC. Se falhar, cada página cai para UC.
        let wc_ok = k_nano::apic::init_pat();

        // HW-6: FB pages mapped as WB by firmware — CPU writes stay in cache,
        // display controller never sees them → garbled/stale output on real HW.
        // Aqui viram WC (PTE PAT) quando PAT disponível; senão UC.
        let pm = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let phys = (gpu_dev.fb_addr as u64).saturating_sub(pm);
        let fb_size = (gpu_dev.fb_height as usize).saturating_mul(gpu_dev.stride_bytes());
        let pages = (fb_size + 4095) / 4096;
        let mut mapped_count = 0usize;
        let mut wc_pages = 0usize;
        for i in 0..pages {
            let page_phys = phys.saturating_add((i as u64) * 4096);
            // overflow guard
            if page_phys < phys && i > 0 { break; }
            unsafe {
                if wc_ok && k_nano::apic::map_page_wc(page_phys, pm) {
                    wc_pages += 1;
                } else {
                    // Fallback UC: funciona igual, só mais lento no present.
                    k_nano::apic::map_page_uc(page_phys, pm);
                }
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
        if wc_ok && wc_pages == mapped_count {
            k_nano::slog_jarbas!("Display", "ok",
                "FB remapped as WC: {} pages @phys={:x} stride={} ({}x{}) [metal-only gain]",
                mapped_count, phys, gpu_dev.stride_bytes(),
                gpu_dev.fb_width, gpu_dev.fb_height);
        } else {
            k_nano::slog_jarbas!("Display", "warn",
                "FB remapped UC (wc_ok={} wc_pages={}/{}): PAT/WC indisponível @phys={:x}",
                wc_ok, wc_pages, mapped_count, phys);
        }
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
        k_nano::slog_jarbas!("Display", "ok", "home=jarbas::display::fb | Sem framebuffer raw — VGA text mode.");
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
        "ok",
        "home=jarbas::display::fb | Limine/raw fb: {}x{} bpp={} stride={} pitch={} @{:x} rgb={}",
        gpu.fb_width,
        gpu.fb_height,
        bpp,
        fb_stride,
        pitch_bytes,
        addr,
        rgb_order
    );
    let w = gpu.fb_width as usize;
    let h = gpu.fb_height as usize;
    let stride = gpu.fb_stride as usize;
    let bpp = gpu.fb_bpp as usize;
    let addr = gpu.fb_addr as usize;
    let rgb = gpu.rgb_order;
    *GPU.lock() = Some(gpu);
    // Ideia Redox graphical_debug: pintar no T+0 SEM limpar o FB inteiro
    // (write_volatile em 1080p UC parece freeze no splash Limine) e SEM slog/ramlog.
    paint_t0_banner(addr, w, h, stride, bpp, rgb);
    // Banner ~72px → começar linhas de progresso abaixo (5 × 16).
    CONSOLE_LINE.store(5, Ordering::Relaxed);
    CONSOLE_INITED.store(true, Ordering::Relaxed);
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
    // vconsole faz String::push — so apos heap (kernel_boot chama phase_line antes).
    if k_nano::boot_logger::heap_ready() {
        crate::vconsole::write_to_active(text);
        crate::vconsole::write_to_active("\n");
    }
    
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

// ─── C5: print routing — canônico jarbas (lesson 261 avoid double-paint) ───
// `fb_print` formata 160B sem alloc e delega a console_print; `_print` é o
// entry usado pelo macro `print!`/`println!` — se FB/compositor ativo, dropa VGA.
pub fn fb_print(args: core::fmt::Arguments) -> bool {
    let has_fb = {
        let gpu = GPU.lock();
        matches!(gpu.as_ref(), Some(g) if g.present && g.fb_addr != 0 && g.fb_width > 0 && g.fb_height > 0)
    };
    if !has_fb {
        return false;
    }
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
    console_print(text);
    true
}

pub fn _print(args: core::fmt::Arguments) {
    if GRAPHICS_OWNED.load(Ordering::Relaxed) {
        return;
    }
    let fb_active = GPU.lock().as_ref().map(|g| g.present).unwrap_or(false);
    if fb_active {
        return;
    }
    // COMPOSITOR check — sem dependência circular via GPU/GRAPHICS_OWNED já cobre
    // o caso compositor ativo (claim_graphics seta GRAPHICS_OWNED). Fallback VGA.
    use core::fmt::Write;
    let mut w = k_nano::vga_buffer::WRITER.lock();
    if let Some(ref mut w) = *w {
        let _ = w.write_fmt(args);
    }
}

fn clear_fb_pixels(addr: usize, h: usize, stride: usize, bpp: usize, rgb: bool) {
    let (tr, tg, tb) = (8u8, 12u8, 24u8); // JARVIS navy
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
    // Navy product row + cyan accent 3px à esquerda (canal B ADR-0092).
    let (bg0, bg1, bg2) = if rgb {
        (8u8, 12u8, 24u8)
    } else {
        (24u8, 12u8, 8u8)
    };
    let (acc0, acc1, acc2) = if rgb {
        (0u8, 212u8, 255u8)
    } else {
        (255u8, 212u8, 0u8)
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
                let (c0, c1, c2) = if x < 3 {
                    (acc0, acc1, acc2)
                } else {
                    (bg0, bg1, bg2)
                };
                write_volatile(ptr.add(off), c0);
                write_volatile(ptr.add(off + 1), c1);
                write_volatile(ptr.add(off + 2), c2);
                if bpp > 3 {
                    write_volatile(ptr.add(off + 3), 0xFF);
                }
            }
        }
    }
    let (fg0, fg1, fg2) = if rgb {
        (220u8, 235u8, 250u8)
    } else {
        (250u8, 235u8, 220u8)
    };
    let mut x = 12usize;
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

/// Checkpoint de boot — ramlog + slog TRACE (ADR-0092: nunca pintar K* no FB).
/// **Sem** `try_flush_ramlog` a cada K* (ATA/USB-MSC PIO no path quente = minutes no
/// metal e no QEMU). Flush só em marcos: fases, `init_after_usb`, fim K33, SCORE.
pub fn boot_ckpt(n: u8, msg: &str) {
    boot_ckpt_noflush(n, msg);
}

/// Igual a `boot_ckpt` + flush oportunista (marcos raros — não usar em loops).
pub fn boot_ckpt_and_flush(n: u8, msg: &str) {
    boot_ckpt_noflush(n, msg);
    let _ = k_nano::boot_logger::try_flush_ramlog();
}

fn boot_ckpt_noflush(n: u8, msg: &str) {
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
    k_nano::slog_jarbas!("BOOT", "trace", "{}", s);
    k_nano::boot_ramlog::set_last_ckpt(n);
    k_nano::boot_ramlog::append(s);
}

/// Canal B (ADR-0092): uma linha de fase no FB só antes do compositor.
/// Formato produto: sem K*, sem dump — texto curto e legível (ASCII font).
pub fn phase_line(s: &str) {
    if !GRAPHICS_OWNED.load(Ordering::Relaxed) {
        let t = s.trim();
        if t.starts_with("PHASE")
            || t.starts_with("BOOT")
            || t.starts_with("NEURAL")
            || t.starts_with("> ")
        {
            console_print(t);
            return;
        }
        let mut buf = [0u8; 96];
        buf[0] = b'>';
        buf[1] = b' ';
        let src = t.as_bytes();
        let n = src.len().min(buf.len() - 2);
        buf[2..2 + n].copy_from_slice(&src[..n]);
        if let Ok(out) = core::str::from_utf8(&buf[..2 + n]) {
            console_print(out);
        } else {
            console_print(t);
        }
    }
}

/// Progresso de boot visível no FB (live USB / sem compositor ainda).
/// Prefixo estável para IA correlacionar com slog `src=BOOT`.
pub fn boot_progress_line(msg: &str) {
    let m = msg.trim();
    if m.starts_with("BOOT:") || m.starts_with("PHASE") {
        phase_line(m);
        return;
    }
    let mut buf = [0u8; 100];
    let prefix = b"BOOT: ";
    buf[..prefix.len()].copy_from_slice(prefix);
    let src = m.as_bytes();
    let n = src.len().min(buf.len() - prefix.len());
    buf[prefix.len()..prefix.len() + n].copy_from_slice(&src[..n]);
    if let Ok(out) = core::str::from_utf8(&buf[..prefix.len() + n]) {
        phase_line(out);
    } else {
        phase_line(m);
    }
}

/// Faixa T+0 profissional (ADR-0092 canal B): marca + estado, sem K*/dump.
/// Navy JARVIS (#080C18) + accent cyan (#00D4FF). Sem limpar o FB inteiro
/// (write UC em 1080p parece freeze no Limine).
fn paint_t0_banner(
    addr: usize,
    width: usize,
    height: usize,
    stride: usize,
    bpp: usize,
    rgb: bool,
) {
    if bpp == 0 || width == 0 || height == 0 {
        return;
    }
    let bar_h = 72usize.min(height);
    // Accent strip (cyan) 4px no topo + corpo navy.
    let (acc0, acc1, acc2) = if rgb {
        (0u8, 212u8, 255u8)
    } else {
        (255u8, 212u8, 0u8)
    };
    let (n0, n1, n2) = if rgb {
        (8u8, 12u8, 24u8)
    } else {
        (24u8, 12u8, 8u8)
    };
    let accent_h = 4usize.min(bar_h);
    unsafe {
        let ptr = addr as *mut u8;
        for y in 0..bar_h {
            let row = ptr.add(y * stride);
            let (c0, c1, c2) = if y < accent_h {
                (acc0, acc1, acc2)
            } else {
                (n0, n1, n2)
            };
            if bpp == 4 {
                let pix = u32::from_le_bytes([c0, c1, c2, 0xFF]);
                let mut x = 0usize;
                let row_bytes = (width * 4).min(stride);
                while x + 4 <= row_bytes {
                    write_volatile(row.add(x) as *mut u32, pix);
                    x += 4;
                }
            } else {
                let mut x = 0usize;
                while x + bpp <= stride && x / bpp < width {
                    write_volatile(row.add(x), c0);
                    write_volatile(row.add(x + 1), c1);
                    write_volatile(row.add(x + 2), c2);
                    x += bpp;
                }
            }
        }
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }
    // Texto: brand + linha de estado (produto, não dmesg).
    let (fg0, fg1, fg2) = if rgb {
        (230u8, 245u8, 255u8)
    } else {
        (255u8, 245u8, 230u8)
    };
    let (dim0, dim1, dim2) = if rgb {
        (140u8, 170u8, 200u8)
    } else {
        (200u8, 170u8, 140u8)
    };
    splash_draw_text_fg(
        addr, width, height, stride, bpp, 20, 16, "NEURAL OS", fg0, fg1, fg2,
    );
    splash_draw_text_fg(
        addr,
        width,
        height,
        stride,
        bpp,
        20,
        36,
        "AIOS bring-up  |  K3CHJ",
        dim0,
        dim1,
        dim2,
    );
    splash_draw_text_fg(
        addr,
        width,
        height,
        stride,
        bpp,
        20,
        52,
        "serial=dmesg  fb=product  score=BOOT SCORE",
        dim0,
        dim1,
        dim2,
    );
    unsafe {
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }
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
    let (c0, c1, c2) = if rgb_order {
        (0u8, 200u8, 255u8)
    } else {
        (255u8, 200u8, 0u8)
    };
    splash_draw_text_fg(fb_addr, width, height, stride, bpp, x0, y0, text, c0, c1, c2);
}

fn splash_draw_text_fg(
    fb_addr: usize,
    width: usize,
    height: usize,
    stride: usize,
    bpp: usize,
    x0: usize,
    y0: usize,
    text: &str,
    c0: u8,
    c1: u8,
    c2: u8,
) {
    if bpp == 0 {
        return;
    }
    let ch = 16usize;
    let cw = 8usize;
    let mut x = x0;
    let y = y0;
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

fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x0 = n / 2;
    loop {
        let x1 = (x0 + n / x0) / 2;
        if x1 >= x0 {
            return x0;
        }
        x0 = x1;
    }
}

/// Framebuffer com double buffering interno.
/// Todas as operacoes de pixel vao para o back buffer (Vec<u8> em heap).
/// swap() copia back → front (memcpy), eliminando cintilacao.
pub struct DoubleBuffer {
    pub info: FramebufferInfo,
    back: Vec<u8>,
    pub dirty: bool,
}

/// Diagnóstico freeze (s318): geom do FB real p/ marcas de estágio. Escrita
/// volatile DIRETA (bypass back/swap) — o frame congelado mostra o último
/// estágio completo do tick do DisplayAgent. Lock-free.
pub static DIAG_FB_ADDR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static DIAG_FB_STRIDE: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static DIAG_FB_BPP: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static DIAG_FB_WIDTH: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Marca de estágio (1..=8): n barras no canto sup. direito do FB REAL.
/// Apaga a faixa antes (última marca vence). No-op pré-graphics.
pub fn diag_mark(n: u8) {
    diag_mark_at(0, n);
}

/// Marca de estágio em linha arbitrária (freeze s321): row y0=0 = barras do
/// display; y0=32 = sub-estágios do tick em curso (ex: Cortex). Volatile.
pub fn diag_mark_at(y0: usize, n: u8) {
    let addr = DIAG_FB_ADDR.load(core::sync::atomic::Ordering::Relaxed);
    if addr == 0 || n == 0 || n > 9 {
        return;
    }
    let stride = DIAG_FB_STRIDE.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let bpp = DIAG_FB_BPP.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let width = DIAG_FB_WIDTH.load(core::sync::atomic::Ordering::Relaxed) as usize;
    if stride == 0 || bpp == 0 || width < 64 {
        return;
    }
    let (bw, bh, gap) = (2usize, 12usize, 2usize);
    let strip_w = 9 * (bw + gap) + 4;
    let x0 = width - strip_w - 2;
    let base = addr as *mut u8;
    unsafe {
        // apaga a faixa inteira (preto) — última marca vence
        for dy in 0..bh {
            let off = (y0 + dy) * stride + x0 * bpp;
            for b in 0..strip_w * bpp {
                base.add(off + b).write_volatile(0);
            }
        }
        // desenha n barras
        for i in 0..n as usize {
            let bx = x0 + 2 + i * (bw + gap);
            for dy in 0..bh {
                let off = (y0 + dy) * stride + bx * bpp;
                for b in 0..bpp {
                    let c = match b {
                        0 => 0xFFu8,
                        1 => 0xD4u8,
                        2 => 0x00u8,
                        _ => 0xFFu8,
                    };
                    base.add(off + b).write_volatile(c);
                }
            }
        }
    }
}

/// Wrapper p/ o bridge agent-core: sub-estágios na linha 2 (y=32) + dígito
/// "S<n>" em y=64 (s326 — barras são ilegíveis em foto).
pub fn diag_stage_row1(n: u8) {
    diag_mark_at(32, n);
    diag_stamp_stage(n);
}

/// Stamp do agente em curso (freeze s319): escreve o NOME direto no FB real
/// (topo-direita, abaixo das barras de estágio). Chamada pelo scheduler a cada
/// TROCA de agente — o frame congelado mostra o AGENTE TRAVADO, não o último
/// paint do display (que é o que a linha IN: do HUD mostra). Volatile, lock-free.
pub fn diag_stamp_agent(name: &[u8]) {
    let addr = DIAG_FB_ADDR.load(core::sync::atomic::Ordering::Relaxed);
    if addr == 0 {
        return;
    }
    let stride = DIAG_FB_STRIDE.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let bpp = DIAG_FB_BPP.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let width = DIAG_FB_WIDTH.load(core::sync::atomic::Ordering::Relaxed) as usize;
    if stride == 0 || bpp == 0 || width < 200 {
        return;
    }
    const MAX_CHARS: usize = 16;
    let (cw, ch) = (8usize, 16usize);
    let n = name.len().min(MAX_CHARS);
    let strip_w = MAX_CHARS * (cw + 1) + 4;
    let x0 = width - strip_w - 2;
    let y0 = 14usize; // abaixo das barras de estágio (0..12)
    let base = addr as *mut u8;
    unsafe {
        // apaga a faixa (preto) — último stamp vence
        for dy in 0..ch {
            let off = (y0 + dy) * stride + x0 * bpp;
            for b in 0..strip_w * bpp {
                base.add(off + b).write_volatile(0);
            }
        }
        // desenha o nome (ciano, MSB-first — mesma lógica de draw_text_scaled)
        for (i, &c) in name.iter().take(n).enumerate() {
            let Some(glyph) = crate::display::font::get_char_bitmap(c as char) else {
                continue;
            };
            let gx = x0 + 2 + i * (cw + 1);
            for (dy, &row) in glyph.iter().enumerate() {
                for dx in 0..cw {
                    if (row >> (7 - dx)) & 1 == 1 {
                        let off = (y0 + dy) * stride + (gx + dx) * bpp;
                        for b in 0..bpp {
                            let v = match b {
                                0 => 0xFFu8,
                                1 => 0xD4u8,
                                2 => 0x00u8,
                                _ => 0xFFu8,
                            };
                            base.add(off + b).write_volatile(v);
                        }
                    }
                }
            }
        }
    }
}

/// Stamp de exceção (freeze s320): escreve o NOME+ip da exceção DIRETO no FB
/// real (vermelho, y=48, esquerda) — o dump serial é invisível no metal.
/// Chamada de IRQ context (dump_exception) — volatile, lock-free, sem alloc.
pub fn diag_stamp_exception(text: &[u8]) {
    let addr = DIAG_FB_ADDR.load(core::sync::atomic::Ordering::Relaxed);
    if addr == 0 {
        return;
    }
    let stride = DIAG_FB_STRIDE.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let bpp = DIAG_FB_BPP.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let width = DIAG_FB_WIDTH.load(core::sync::atomic::Ordering::Relaxed) as usize;
    if stride == 0 || bpp == 0 || width < 500 {
        return;
    }
    const MAX_CHARS: usize = 52;
    let (cw, ch) = (8usize, 16usize);
    let n = text.len().min(MAX_CHARS);
    let strip_w = MAX_CHARS * (cw + 1) + 8;
    let (x0, y0) = (8usize, 48usize);
    let base = addr as *mut u8;
    unsafe {
        // apaga a faixa (preto)
        for dy in 0..ch {
            let off = (y0 + dy) * stride + x0 * bpp;
            for b in 0..strip_w * bpp {
                base.add(off + b).write_volatile(0);
            }
        }
        // desenha o texto (vermelho — BGR: B=0x3C G=0x3C R=0xFF)
        for (i, &c) in text.iter().take(n).enumerate() {
            let Some(glyph) = crate::display::font::get_char_bitmap(c as char) else {
                continue;
            };
            let gx = x0 + 4 + i * (cw + 1);
            for (dy, &row) in glyph.iter().enumerate() {
                for dx in 0..cw {
                    if (row >> (7 - dx)) & 1 == 1 {
                        let off = (y0 + dy) * stride + (gx + dx) * bpp;
                        for b in 0..bpp {
                            let v = match b {
                                0 => 0x3Cu8,
                                1 => 0x3Cu8,
                                2 => 0xFFu8,
                                _ => 0xFFu8,
                            };
                            base.add(off + b).write_volatile(v);
                        }
                    }
                }
            }
        }
    }
}

/// Dígito do estágio (freeze s326): escreve "S<n>" (ciano, y=64, direita) —
/// barras são ilegíveis em foto; o dígito é inequívoco. Volatile, lock-free.
pub fn diag_stamp_stage(n: u8) {
    let addr = DIAG_FB_ADDR.load(core::sync::atomic::Ordering::Relaxed);
    if addr == 0 {
        return;
    }
    let stride = DIAG_FB_STRIDE.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let bpp = DIAG_FB_BPP.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let width = DIAG_FB_WIDTH.load(core::sync::atomic::Ordering::Relaxed) as usize;
    if stride == 0 || bpp == 0 || width < 64 {
        return;
    }
    let text = [b'S', if n >= 10 { b'?' } else { b'0' + n }];
    const CHARS: usize = 2;
    let (cw, ch) = (8usize, 16usize);
    let strip_w = CHARS * (cw + 1) + 4;
    let (x0, y0) = (width - strip_w - 2, 64usize);
    let base = addr as *mut u8;
    unsafe {
        for dy in 0..ch {
            let off = (y0 + dy) * stride + x0 * bpp;
            for b in 0..strip_w * bpp {
                base.add(off + b).write_volatile(0);
            }
        }
        for (i, &c) in text.iter().enumerate() {
            let Some(glyph) = crate::display::font::get_char_bitmap(c as char) else {
                continue;
            };
            let gx = x0 + 2 + i * (cw + 1);
            for (dy, &row) in glyph.iter().enumerate() {
                for dx in 0..cw {
                    if (row >> (7 - dx)) & 1 == 1 {
                        let off = (y0 + dy) * stride + (gx + dx) * bpp;
                        for b in 0..bpp {
                            let v = match b {
                                0 => 0xFFu8,
                                1 => 0xD4u8,
                                2 => 0x00u8,
                                _ => 0xFFu8,
                            };
                            base.add(off + b).write_volatile(v);
                        }
                    }
                }
            }
        }
    }
}

/// Heartbeat do timer (freeze s326): escreve "T=<hex8>" (ciano, y=80, direita)
/// a CADA tick do timer IRQ. Discriminador vivo-vs-morto: tela congelada + T
/// avançando = thread girando com IRQ viva; T congelado = freeze DENTRO de IRQ
/// (ou IF=0). Volatile, lock-free, sem alloc — seguro em IRQ context.
pub fn heartbeat_stamp(ticks: u64) {
    let addr = DIAG_FB_ADDR.load(core::sync::atomic::Ordering::Relaxed);
    if addr == 0 {
        return;
    }
    let stride = DIAG_FB_STRIDE.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let bpp = DIAG_FB_BPP.load(core::sync::atomic::Ordering::Relaxed) as usize;
    let width = DIAG_FB_WIDTH.load(core::sync::atomic::Ordering::Relaxed) as usize;
    if stride == 0 || bpp == 0 || width < 128 {
        return;
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut text = [b'T'; 10];
    text[1] = b'=';
    for i in 0..8 {
        text[2 + i] = HEX[((ticks >> (60 - 4 * i)) & 0xF) as usize];
    }
    const CHARS: usize = 10;
    let (cw, ch) = (8usize, 16usize);
    let strip_w = CHARS * (cw + 1) + 4;
    let (x0, y0) = (width - strip_w - 2, 80usize);
    let base = addr as *mut u8;
    unsafe {
        for dy in 0..ch {
            let off = (y0 + dy) * stride + x0 * bpp;
            for b in 0..strip_w * bpp {
                base.add(off + b).write_volatile(0);
            }
        }
        for (i, &c) in text.iter().enumerate() {
            let Some(glyph) = crate::display::font::get_char_bitmap(c as char) else {
                continue;
            };
            let gx = x0 + 2 + i * (cw + 1);
            for (dy, &row) in glyph.iter().enumerate() {
                for dx in 0..cw {
                    if (row >> (7 - dx)) & 1 == 1 {
                        let off = (y0 + dy) * stride + (gx + dx) * bpp;
                        for b in 0..bpp {
                            let v = match b {
                                0 => 0xFFu8,
                                1 => 0xD4u8,
                                2 => 0x00u8,
                                _ => 0xFFu8,
                            };
                            base.add(off + b).write_volatile(v);
                        }
                    }
                }
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// #5 — Row blitters SSE2 (padrão do repo: #[target_feature] + gate runtime).
// O target soft-float desliga sse2 no nível do target; `#[target_feature]`
// reabilita por função (cf. `cortex::bitnet_sse`). Fallback escalar sempre
// existe — nada é hard-`cfg`ado para fora.
// ══════════════════════════════════════════════════════════════════════════

/// Gate runtime SSE2. SSE2 é baseline do x86_64 (o `-C target-feature=-sse2`
/// do soft-float é política de build, não ausência de hardware). Espelha
/// `cortex::bitnet_sse::sse2_available()`.
#[inline]
fn sse2_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        true
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Copia `len` bytes com SSE2 (16 B/ciclo) + cauda escalar.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn sse2_copy_bytes(dst: *mut u8, src: *const u8, len: usize) {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    while i + 16 <= len {
        let v = _mm_loadu_si128(src.add(i) as *const __m128i);
        _mm_storeu_si128(dst.add(i) as *mut __m128i, v);
        i += 16;
    }
    while i < len {
        *dst.add(i) = *src.add(i);
        i += 1;
    }
}

/// Dispatch: `copy_nonoverlapping` (compiler memcpy).
///
/// ⚠️ s335: o kernel builda com `-C target-feature=-sse2` (soft-float,
/// `.cargo/config.toml`). Sob essa flag o LLVM rebaixa os intrinsics de
/// `sse2_copy_bytes` (`_mm_loadu_si128`/`_mm_storeu_si128`) para código
/// escalar **incorreto**: cada bloco de 16 B copiado zera o byte 1 de cada
/// u32 (o canal G no layout BGRX do FB) — o tail escalar de 8 B copia certo.
/// Sintoma no QEMU: orb/HUD sem o canal G (azul em vez de ciano), dock (via
/// movnti) correto. Mesma classe do bug documentado de `find_child_byte16_sse`
/// (AGENTS.md). O `copy_nonoverlapping` compila para memcpy escalar correto
/// (provado pelo fill_rect_fast/dock). `sse2_copy_bytes` fica só para os
/// testes de paridade no host (SSE2 nativo lá é correto).
#[inline]
unsafe fn copy_bytes(dst: *mut u8, src: *const u8, len: usize) {
    core::ptr::copy_nonoverlapping(src, dst, len);
}

/// Copia `len` bytes com **stores non-temporal** (streaming) SSE2 — evita o
/// read-for-ownership (RFO) da linha de cache no destino. A cauda (`< 8 B`) é
/// escalar.
///
/// Usa `movnti` (non-temporal store de 64-bit em registrador GP). O `movntdq`
/// de 128-bit (`_mm_stream_si128`) NÃO compila neste target soft-float: o asm
/// com constraint `xmm_reg` dispara `rustc-LLVM ERROR: Do not know how to split
/// this operator's operand!`. `movnti` dá o mesmo hint sem operando XMM.
///
/// Só faz sentido quando o destino é WC (framebuffer); em memória WB o hint é
/// apenas uma dica e o dado continua correto. Como os stores são fracamente
/// ordenados, o caller DEVE emitir `sfence` antes de considerar o frame
/// apresentado. Ganho **metal-only** — QEMU ignora o hint.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn sse2_stream_copy_bytes(dst: *mut u8, src: *const u8, len: usize) {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    while i + 8 <= len {
        let v = (src.add(i) as *const i64).read_unaligned();
        _mm_stream_si64(dst.add(i) as *mut i64, v);
        i += 8;
    }
    while i < len {
        *dst.add(i) = *src.add(i);
        i += 1;
    }
}

/// Copia back→front usando streaming stores quando o destino está alinhado a
/// 16 B (bulk full-width); senão delega ao `copy_bytes`. Retorna `true` se o
/// caminho non-temporal foi usado — o caller deve emitir `sfence` depois.
#[inline]
unsafe fn stream_copy_bytes(dst: *mut u8, src: *const u8, len: usize) -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        if sse2_available() && len >= 16 && (dst as usize & 15) == 0 {
            sse2_stream_copy_bytes(dst, src, len);
            return true;
        }
    }
    copy_bytes(dst, src, len);
    false
}

/// SWAR escalar (referência exata) — `dst += ((tint-dst)*k)>>8` por canal,
/// 2 canais por word via máscara 0x00FF00FF. Mesma matemática do bpp==4.
pub(crate) unsafe fn tint_swar_scalar(
    ptr: *mut u8,
    stride: usize,
    x: usize,
    y: usize,
    aw: usize,
    ah: usize,
    tint_word: u32,
    k: u32,
) {
    let inv = 256 - k;
    let te = tint_word & 0x00FF_00FF;
    let to = (tint_word >> 8) & 0x00FF_00FF;
    let tek = te * k;
    let tok = to * k;
    for dy in 0..ah {
        let mut p = ptr.add((y + dy) * stride + x * 4) as *mut u32;
        for _ in 0..aw {
            let d = p.read();
            let de = d & 0x00FF_00FF;
            let od = (d >> 8) & 0x00FF_00FF;
            let re = (tek + de * inv) >> 8;
            let ro = (tok + od * inv) >> 8;
            p.write((re & 0x00FF_00FF) | ((ro & 0x00FF_00FF) << 8));
            p = p.add(1);
        }
    }
}

/// Versão SSE2 do tint: 4 pixels (16 B) por iteração, paridade EXATA com o
/// SWAR escalar (produtos ≤65025 caibam em u16; carry recalculado nos bytes
/// baixos). Cauda escalar. `k == 0` é filtrado pelo caller.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn tint_sse2(
    ptr: *mut u8,
    stride: usize,
    x: usize,
    y: usize,
    aw: usize,
    ah: usize,
    tint_word: u32,
    k: u32,
) {
    use core::arch::x86_64::*;
    let inv = 256 - k;
    let mask = _mm_set1_epi32(0x00FF_00FFu32 as i32);
    let lowbyte = _mm_set1_epi16(0x00FFu16 as i16);
    let kkv = _mm_set1_epi16(k as i16);
    let invv = _mm_set1_epi16(inv as i16);
    let te = tint_word & 0x00FF_00FF;
    let to = (tint_word >> 8) & 0x00FF_00FF;
    let tev = _mm_set1_epi32(te as i32);
    let tov = _mm_set1_epi32(to as i32);
    // floor(const/256) + carry pré-computados para as constantes.
    let thi_e = _mm_mulhi_epu16(_mm_slli_epi16(tev, 8), kkv);
    let thi_o = _mm_mulhi_epu16(_mm_slli_epi16(tov, 8), kkv);
    let tlo_e = _mm_and_si128(_mm_mullo_epi16(tev, kkv), lowbyte);
    let tlo_o = _mm_and_si128(_mm_mullo_epi16(tov, kkv), lowbyte);
    for dy in 0..ah {
        let mut p = ptr.add((y + dy) * stride + x * 4);
        let mut remaining = aw;
        while remaining >= 4 {
            let d = _mm_loadu_si128(p as *const __m128i);
            let de = _mm_and_si128(d, mask);
            let od = _mm_and_si128(_mm_srli_epi32(d, 8), mask);
            let dhi_e = _mm_mulhi_epu16(_mm_slli_epi16(de, 8), invv);
            let dhi_o = _mm_mulhi_epu16(_mm_slli_epi16(od, 8), invv);
            let dlo_e = _mm_and_si128(_mm_mullo_epi16(de, invv), lowbyte);
            let dlo_o = _mm_and_si128(_mm_mullo_epi16(od, invv), lowbyte);
            let carry_e = _mm_srli_epi16(_mm_add_epi16(dlo_e, tlo_e), 8);
            let carry_o = _mm_srli_epi16(_mm_add_epi16(dlo_o, tlo_o), 8);
            let re = _mm_add_epi16(_mm_add_epi16(dhi_e, thi_e), carry_e);
            let ro = _mm_add_epi16(_mm_add_epi16(dhi_o, thi_o), carry_o);
            let res = _mm_or_si128(
                _mm_and_si128(re, mask),
                _mm_slli_epi32(_mm_and_si128(ro, mask), 8),
            );
            _mm_storeu_si128(p as *mut __m128i, res);
            p = p.add(16);
            remaining -= 4;
        }
        let mut q = p as *mut u32;
        for _ in 0..remaining {
            let d = q.read();
            let de = d & 0x00FF_00FF;
            let od = (d >> 8) & 0x00FF_00FF;
            let re = (te * k + de * inv) >> 8;
            let ro = (to * k + od * inv) >> 8;
            q.write((re & 0x00FF_00FF) | ((ro & 0x00FF_00FF) << 8));
            q = q.add(1);
        }
    }
}

impl DoubleBuffer {
    /// Constrói o double-buffer a partir do GpuDevice já probeado (fonte dinâmica).
    pub fn from_gpu(gpu: &GpuDevice) -> Self {
        let size = (gpu.fb_height as usize).saturating_mul(gpu.stride_bytes());
        let db = DoubleBuffer {
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
        };
        DIAG_FB_ADDR.store(gpu.fb_addr as u64, core::sync::atomic::Ordering::Relaxed);
        DIAG_FB_STRIDE.store(gpu.stride_bytes() as u32, core::sync::atomic::Ordering::Relaxed);
        DIAG_FB_BPP.store(gpu.bytes_per_pixel() as u32, core::sync::atomic::Ordering::Relaxed);
        DIAG_FB_WIDTH.store(gpu.fb_width as u32, core::sync::atomic::Ordering::Relaxed);
        db
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

    /// §3.1 (ADR-0090 Tier 1): unchecked pixel write — caller guarantees
    /// `x < width && y < height`. Skips bounds checks; sets `dirty`.
    /// Grid blit (§3.2) pre-clips positions at build, so per-frame cost
    /// is one raw write per dot.
    pub fn set_pixel_unchecked(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        self.dirty = true;
        let bpp = self.info.bpp;
        let offset = y * self.info.stride + x * bpp;
        unsafe {
            let back = self.back.as_mut_ptr().add(offset);
            if self.info.rgb_order {
                *back = r;
                *back.add(1) = g;
                *back.add(2) = b;
            } else {
                *back = b;
                *back.add(1) = g;
                *back.add(2) = r;
            }
            if bpp > 3 { *back.add(3) = 0xFF; }
        }
    }

    /// §3.1 (ADR-0090 Tier 1): glyph row blit — 1 row of 8 px, MSB-first,
    /// transparent background. One clip check per row; set bits drained via
    /// `leading_zeros` (no per-pixel fn call, no branch on empty pixels).
    /// Pixel-identical to the `set_pixel` loop in `draw_text_scaled(scale=1)`.
    pub fn blit_glyph_row(&mut self, x: usize, y: usize, bits: u32, r: u8, g: u8, b: u8) {
        if bits == 0 || y >= self.info.height || x >= self.info.width {
            return;
        }
        self.dirty = true;
        let bpp = self.info.bpp;
        let base = y * self.info.stride + x * bpp;
        let back_len = self.back.len();
        let rgb = self.info.rgb_order;
        let ptr = self.back.as_mut_ptr();
        let mut w = bits;
        while w != 0 {
            let dx = w.leading_zeros() as usize;
            if dx >= 8 {
                break;
            }
            w &= !(0x8000_0000u32 >> dx);
            if x + dx >= self.info.width {
                continue;
            }
            let off = base + dx * bpp;
            if off + bpp > back_len {
                continue;
            }
            unsafe {
                if rgb {
                    *ptr.add(off) = r;
                    *ptr.add(off + 1) = g;
                    *ptr.add(off + 2) = b;
                } else {
                    *ptr.add(off) = b;
                    *ptr.add(off + 1) = g;
                    *ptr.add(off + 2) = r;
                }
                if bpp > 3 {
                    *ptr.add(off + 3) = 0xFF;
                }
            }
        }
    }

    /// Copia retângulo do back buffer para `dst` (layout packed bpp).
    pub fn copy_rect_out(&self, x: usize, y: usize, w: usize, h: usize, dst: &mut [u8]) {
        let bpp = self.info.bpp;
        let need = w.saturating_mul(h).saturating_mul(bpp);
        if dst.len() < need || w == 0 || h == 0 {
            return;
        }
        let row_bytes = w * bpp;
        let back_len = self.back.len();
        let mut di = 0usize;
        for row in 0..h {
            let py = y + row;
            if py >= self.info.height {
                break;
            }
            // #5 fast path: linha inteira dentro do back → cópia contígua SSE2.
            let off = py * self.info.stride + x * bpp;
            if x + w <= self.info.width && off + row_bytes <= back_len {
                unsafe {
                    copy_bytes(dst.as_mut_ptr().add(di), self.back.as_ptr().add(off), row_bytes);
                }
                di += row_bytes;
                continue;
            }
            for col in 0..w {
                let px = x + col;
                let off = if px < self.info.width {
                    py * self.info.stride + px * bpp
                } else {
                    usize::MAX
                };
                if off != usize::MAX && off + bpp <= back_len {
                    dst[di..di + bpp].copy_from_slice(&self.back[off..off + bpp]);
                } else {
                    for b in 0..bpp {
                        dst[di + b] = if b == 3 { 0xFF } else { 0 };
                    }
                }
                di += bpp;
            }
        }
    }

    /// Escreve retângulo packed bpp de `src` no back buffer.
    pub fn copy_rect_in(&mut self, x: usize, y: usize, w: usize, h: usize, src: &[u8]) {
        let bpp = self.info.bpp;
        let need = w.saturating_mul(h).saturating_mul(bpp);
        if src.len() < need || w == 0 || h == 0 {
            return;
        }
        self.dirty = true;
        let row_bytes = w * bpp;
        let back_len = self.back.len();
        let mut si = 0usize;
        for row in 0..h {
            let py = y + row;
            if py >= self.info.height {
                break;
            }
            // #5 fast path: linha inteira dentro do back → cópia contígua SSE2.
            let off = py * self.info.stride + x * bpp;
            if x + w <= self.info.width && off + row_bytes <= back_len {
                unsafe {
                    copy_bytes(self.back.as_mut_ptr().add(off), src.as_ptr().add(si), row_bytes);
                }
                si += row_bytes;
                continue;
            }
            for col in 0..w {
                let px = x + col;
                if px < self.info.width {
                    let off = py * self.info.stride + px * bpp;
                    if off + bpp <= back_len {
                        self.back[off..off + bpp].copy_from_slice(&src[si..si + bpp]);
                    }
                }
                si += bpp;
            }
        }
    }

    /// Lê pixel do back buffer (software cursor underlay).
    pub fn get_pixel(&self, x: usize, y: usize) -> Option<(u8, u8, u8)> {
        if x >= self.info.width || y >= self.info.height {
            return None;
        }
        let bpp = self.info.bpp;
        let offset = y * self.info.stride + x * bpp;
        if offset + (bpp - 1) >= self.back.len() {
            return None;
        }
        if self.info.rgb_order {
            Some((self.back[offset], self.back[offset + 1], self.back[offset + 2]))
        } else {
            Some((self.back[offset + 2], self.back[offset + 1], self.back[offset]))
        }
    }

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        self.fill_rect_fast(0, 0, self.info.width, self.info.height, r, g, b);
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
        self.fill_rect_fast(x, y, w, h, r, g, b);
    }

    /// Fill sólido por linha (u32). O loop `aw/4` antigo pintava só 25% do rect.
    /// Retorna os pixels efetivamente escritos (clipping aplicado).
    pub fn fill_rect_fast(&mut self, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8) -> usize {
        if x >= self.info.width || y >= self.info.height { return 0; }
        let x2 = (x + w).min(self.info.width);
        let y2 = (y + h).min(self.info.height);
        let aw = x2 - x;
        let ah = y2 - y;
        if aw == 0 || ah == 0 { return 0; }
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
                    if aw < 16 {
                        for dx in 0..aw {
                            row.add(dx).write(pix);
                        }
                    } else {
                        // Doubling memcpy: 1 → 2 → 4 … cobre a linha em log2(aw) copies.
                        row.write(pix);
                        let mut filled = 1usize;
                        while filled < aw {
                            let n = filled.min(aw - filled);
                            copy_nonoverlapping(row, row.add(filled), n);
                            filled += n;
                        }
                    }
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
        aw * ah
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

    /// Disco sólido scanline (camadas empilhadas no Soul Mirror fazem o glow).
    /// Evita O(r²) `sqrtf`+`set_pixel` — o orb r≈260 era ~280k sqrts/frame.
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
        let scale = (alpha_pct as u16).min(100);
        if scale == 0 {
            return;
        }
        let rr = ((r as u16 * scale) / 100) as u8;
        let gg = ((g as u16 * scale) / 100) as u8;
        let bb = ((b as u16 * scale) / 100) as u8;
        let r2 = radius as i64 * radius as i64;
        let fw = self.info.width as isize;
        let fh = self.info.height as isize;
        for dy in -radius..=radius {
            let yy = dy as i64 * dy as i64;
            if yy > r2 {
                continue;
            }
            let dx = isqrt_u64((r2 - yy) as u64) as isize;
            let py = cy + dy;
            if py < 0 || py >= fh {
                continue;
            }
            let mut x0 = cx - dx;
            let mut x1 = cx + dx;
            if x1 < 0 || x0 >= fw {
                continue;
            }
            if x0 < 0 {
                x0 = 0;
            }
            if x1 >= fw {
                x1 = fw - 1;
            }
            if x1 < x0 {
                continue;
            }
            self.fill_rect_fast(
                x0 as usize,
                py as usize,
                (x1 - x0 + 1) as usize,
                1,
                rr,
                gg,
                bb,
            );
        }
    }


    /// Circle with radial gradient — quadratic falloff, no sqrt, no exp.
    /// distance-squared comparison + alpha = (1 - d²/r²)² for smooth glow.
    /// Each row is one fill_rect_fast call (scanline).
    pub fn fill_circle_gradient_scanline(
        &mut self,
        cx: isize,
        cy: isize,
        radius: isize,
        r: u8,
        g: u8,
        b: u8,
        r_sq: i64,  // pre-computed radius²
    ) {
        if radius <= 0 { return; }
        let fw = self.info.width as isize;
        let fh = self.info.height as isize;
        for dy in -radius..=radius {
            let py = cy + dy;
            if py < 0 || py >= fh { continue; }
            let yy = (dy as i64) * (dy as i64);
            if yy > r_sq { continue; }
            let dx_max = isqrt_u64((r_sq - yy) as u64) as isize;
            let mut x0 = cx - dx_max;
            let mut x1 = cx + dx_max;
            if x1 < 0 || x0 >= fw { continue; }
            if x0 < 0 { x0 = 0; }
            if x1 >= fw { x1 = fw - 1; }
            if x1 < x0 { continue; }

            // For each pixel in this row, compute alpha from distance²
            // alpha = (1 - d²/r²)² — quadratic falloff for smooth glow
            // Use 8-bit fixed point: alpha_u8 = ((r_sq - d_sq) * 255 / r_sq)² / 255
            let bpp = self.info.bpp;
            let stride = self.info.stride;
            let back_ptr = self.back.as_mut_ptr();
            let back_len = self.back.len();
            let rgb = self.info.rgb_order;
            self.dirty = true;
            let row_off = (py as usize) * stride;

            for px in x0..=x1 {
                let ddx = (px - cx) as i64;
                let d_sq = ddx * ddx + yy;
                // Linear alpha: 0 at edge, 255 at center
                let linear = ((r_sq - d_sq) * 255 / r_sq) as i32;
                // Quadratic: alpha = linear² / 255 for smoother falloff
                let alpha = ((linear * linear) / 255).clamp(0, 255) as u16;
                if alpha < 2 { continue; }
                let off = row_off + (px as usize) * bpp;
                if off + 3 >= back_len { continue; }
                unsafe {
                    let er = *back_ptr.add(off + if rgb { 0 } else { 2 }) as u16;
                    let eg = *back_ptr.add(off + 1) as u16;
                    let eb = *back_ptr.add(off + if rgb { 2 } else { 0 }) as u16;
                    let inv = 256 - alpha;
                    let nr = ((r as u16 * alpha + er * inv) >> 8) as u8;
                    let ng = ((g as u16 * alpha + eg * inv) >> 8) as u8;
                    let nb = ((b as u16 * alpha + eb * inv) >> 8) as u8;
                    if rgb {
                        *back_ptr.add(off) = nr;
                        *back_ptr.add(off + 1) = ng;
                        *back_ptr.add(off + 2) = nb;
                    } else {
                        *back_ptr.add(off) = nb;
                        *back_ptr.add(off + 1) = ng;
                        *back_ptr.add(off + 2) = nr;
                    }
                    *back_ptr.add(off + 3) = 0xFF;
                }
            }
        }
    }

    /// SWAR tint blend: `dst += ((tint - dst) * k) >> 8` por canal, sem
    /// divisão por pixel. Escrita in-place — preserva grid/scanlines por baixo
    /// (halo do orb). `k`: 0 = identidade, 255 = tint. Retorna pixels tocados.
    pub fn fill_rect_darken_tint(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        r: u8,
        g: u8,
        b: u8,
        k: u8,
    ) -> usize {
        if k == 0 || w == 0 || h == 0 || x >= self.info.width || y >= self.info.height {
            return 0;
        }
        let x2 = (x + w).min(self.info.width);
        let y2 = (y + h).min(self.info.height);
        let aw = x2 - x;
        let ah = y2 - y;
        if aw == 0 || ah == 0 { return 0; }
        self.dirty = true;
        let bpp = self.info.bpp;
        let stride = self.info.stride;
        let ptr = self.back.as_mut_ptr();
        let kk = k as u32;
        unsafe {
            if bpp == 4 {
                let tint_word = if self.info.rgb_order {
                    u32::from_le_bytes([r, g, b, 0xFF])
                } else {
                    u32::from_le_bytes([b, g, r, 0xFF])
                };
                // ⚠️ s335: `tint_sse2` usa `_mm_loadu_si128`/`_mm_storeu_si128`
                // e sofre da MESMA miscompilação soft-float que `sse2_copy_bytes`
                // (zera o canal G no kernel). O `tint_swar_scalar` é a referência
                // de paridade e compila correto. `tint_sse2` fica p/ testes host.
                tint_swar_scalar(ptr, stride, x, y, aw, ah, tint_word, kk);
            } else {
                let (c0, c1, c2) = if self.info.rgb_order { (r, g, b) } else { (b, g, r) };
                let ch = [c0, c1, c2];
                for dy in 0..ah {
                    let base = (y + dy) * stride + x * bpp;
                    for dx in 0..aw {
                        let off = base + dx * bpp;
                        for ci in 0..3 {
                            let d = ptr.add(off + ci).read() as i32;
                            let nv = d + (((ch[ci] as i32 - d) * kk as i32) >> 8);
                            ptr.add(off + ci).write(nv.clamp(0, 255) as u8);
                        }
                        if bpp > 3 { ptr.add(off + 3).write(0xFF); }
                    }
                }
            }
        }
        aw * ah
    }

    /// Disco plano por linhas (cor sólida) usando LUT de meias-larguras.
    /// Zero trabalho por pixel além do fill de linha. Retorna pixels pintados.
    pub fn fill_circle_flat_spans(
        &mut self,
        cx: isize,
        cy: isize,
        radius: isize,
        half: &[u16],
        r: u8,
        g: u8,
        b: u8,
    ) -> usize {
        if radius <= 0 { return 0; }
        let ru = radius as usize;
        if half.len() < ru * 2 + 1 { return 0; }
        let fh = self.info.height as isize;
        let fw = self.info.width as isize;
        let mut filled = 0usize;
        for dy in -radius..=radius {
            let py = cy + dy;
            if py < 0 || py >= fh { continue; }
            let xw = half[(radius + dy) as usize] as isize;
            let x0 = (cx - xw).max(0);
            let x1 = (cx + xw).min(fw - 1);
            if x1 < x0 { continue; }
            filled += self.fill_rect_fast(x0 as usize, py as usize, (x1 - x0 + 1) as usize, 1, r, g, b);
        }
        filled
    }

    /// Disco com falloff radial em BANDAS DE LINHA: alpha quadrático por linha
    /// (1 recíproco por disco, zero divisão por pixel) e blend SWAR in-place.
    /// `strength` = alpha máximo do centro (0..=255). Retorna pixels tocados.
    pub fn fill_circle_alpha_bands(
        &mut self,
        cx: isize,
        cy: isize,
        radius: isize,
        half: &[u16],
        r: u8,
        g: u8,
        b: u8,
        strength: u8,
    ) -> usize {
        if radius <= 0 || strength == 0 { return 0; }
        let ru = radius as usize;
        if half.len() < ru * 2 + 1 { return 0; }
        let r2 = (radius as u64) * (radius as u64);
        if r2 == 0 { return 0; }
        // Recíproco Q32 — 1 divisão por disco; ratio_q32 >> 24 = alpha 0..255.
        let inv_q32 = (1u64 << 32) / r2;
        let fh = self.info.height as isize;
        let fw = self.info.width as isize;
        let mut filled = 0usize;
        for dy in -radius..=radius {
            let py = cy + dy;
            if py < 0 || py >= fh { continue; }
            let yy = (dy * dy) as u64;
            if yy > r2 { continue; }
            let lin = (((r2 - yy) * inv_q32) >> 24).min(255) as u32; // 0..255
            let a = (lin * lin) >> 8; // falloff quadrático
            let k = (a * strength as u32) >> 8;
            if k < 2 { continue; }
            let xw = half[(radius + dy) as usize] as isize;
            let x0 = (cx - xw).max(0);
            let x1 = (cx + xw).min(fw - 1);
            if x1 < x0 { continue; }
            filled += self.fill_rect_darken_tint(
                x0 as usize, py as usize, (x1 - x0 + 1) as usize, 1, r, g, b, k as u8,
            );
        }
        filled
    }

    /// Arco/anel elíptico por spans (2 spans por linha) com brilho viajante da
    /// SIN_LUT. `half` = LUT de meias-larguras do raio `ry` (círculo unitário
    /// escalado para rx/ry) — zero trig por ponto. `dy0..=dy1` permite desenhar
    /// só a metade traseira/dianteira do anel (perspectiva). Retorna pixels.
    pub fn ring_spans(
        &mut self,
        cx: isize,
        cy: isize,
        rx: isize,
        ry: isize,
        thickness: isize,
        r: u8,
        g: u8,
        b: u8,
        phase: u8,
        dy0: isize,
        dy1: isize,
        half: &[u16],
    ) -> usize {
        if rx <= 0 || ry <= 0 || thickness <= 0 { return 0; }
        let ruy = ry as usize;
        if half.len() < ruy * 2 + 1 { return 0; }
        let fh = self.info.height as isize;
        let fw = self.info.width as isize;
        let scale_q16 = ((rx as u64) << 16) / (ry as u64);
        let t_step = (256u64 << 8) / ((2 * ry) as u64).max(1);
        let halfth = thickness / 2;
        let mut t_q = 0u64;
        let mut filled = 0usize;
        for dy in -ry..=ry {
            if dy >= dy0 && dy <= dy1 {
                let py = cy + dy;
                if py >= 0 && py < fh {
                    let xw = ((half[(ry + dy) as usize] as u64 * scale_q16) >> 16) as isize;
                    if xw > 0 {
                        let idx = ((t_q >> 8) as i32 + phase as i32) & 0xFF;
                        let bright = (128 + SIN_LUT[idx as usize] as i32).clamp(0, 255) as u32;
                        let rr = ((r as u32 * bright) >> 8) as u8;
                        let gg = ((g as u32 * bright) >> 8) as u8;
                        let bb = ((b as u32 * bright) >> 8) as u8;
                        let lx = cx - xw - halfth;
                        if lx + thickness > 0 && lx < fw {
                            let x0 = lx.max(0) as usize;
                            let x1 = (lx + thickness).min(fw) as usize;
                            filled += self.fill_rect_fast(x0, py as usize, x1 - x0, 1, rr, gg, bb);
                        }
                        let rx_ = cx + xw - halfth;
                        if rx_ + thickness > 0 && rx_ < fw {
                            let x0 = rx_.max(0) as usize;
                            let x1 = (rx_ + thickness).min(fw) as usize;
                            filled += self.fill_rect_fast(x0, py as usize, x1 - x0, 1, rr, gg, bb);
                        }
                    }
                }
            }
            t_q += t_step;
        }
        filled
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

    /// Copia retângulo back→front (sem spinlock). Preferível ao swap full no TCG.
    pub fn swap_rect(&mut self, x: usize, y: usize, w: usize, h: usize) {
        if w == 0 || h == 0 {
            return;
        }
        let bpp = self.info.bpp;
        let stride = self.info.stride;
        let fw = self.info.width;
        let fh = self.info.height;
        let x0 = x.min(fw);
        let y0 = y.min(fh);
        let x1 = (x + w).min(fw);
        let y1 = (y + h).min(fh);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let row_bytes = (x1 - x0) * bpp;
        let front = self.info.addr as *mut u8;
        if front.is_null() {
            return;
        }
        unsafe {
            let mut used_nt = false;
            for py in y0..y1 {
                let off = py * stride + x0 * bpp;
                if off + row_bytes > self.back.len() {
                    break;
                }
                // #5: streaming (non-temporal) quando a linha alinha; senão SSE2.
                used_nt |= stream_copy_bytes(front.add(off), self.back.as_ptr().add(off), row_bytes);
            }
            // Stores NT/WC são fracamente ordenados: o display só vê após sfence.
            if used_nt {
                core::arch::asm!("sfence", options(nostack, preserves_flags));
            }
        }
    }

    /// Full back→front. Sem spin em CURSOR_LOCK (causava freeze permanente).
    pub fn swap(&mut self) {
        if !self.dirty {
            return;
        }
        // Cinto: limpa lock legado caso IRQ antigo tenha vazado.
        k_nano::interrupts::CURSOR_LOCK.store(false, Ordering::Release);
        let addr = self.info.addr;
        if addr == 0 {
            return;
        }
        let len = self.back.len();
        unsafe {
            // #5: streaming NT no full-frame alinhado (FB WC); SSE2/escalar no resto.
            let used_nt = stream_copy_bytes(addr as *mut u8, self.back.as_ptr(), len);
            if used_nt {
                // Visibilidade ao display engine antes de considerar apresentado.
                core::arch::asm!("sfence", options(nostack, preserves_flags));
            }
        }
        self.dirty = false;
    }
}

#[cfg(test)]
mod simd_parity_tests {
    #![allow(clippy::needless_range_loop)]
    #[cfg(target_arch = "x86_64")]
    use super::*;

    /// xorshift determinístico (sem rand no no_std/host).
    fn fill_pseudo(buf: &mut [u8], mut seed: u32) {
        for b in buf.iter_mut() {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *b = (seed >> 8) as u8;
        }
    }

    /// #5: SSE2 row copy byte-idêntico ao `copy_nonoverlapping` (varias larguras,
    /// inclusive não múltiplas de 16).
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn sse2_copy_matches_scalar_for_any_len() {
        for &len in &[0usize, 1, 3, 15, 16, 17, 31, 32, 33, 64, 100, 255, 1000] {
            let mut src = alloc::vec![0u8; len.max(1)];
            fill_pseudo(&mut src, 0x1234_5678 ^ len as u32);
            let mut got = alloc::vec![0xEEu8; len];
            let mut want = alloc::vec![0xEEu8; len];
            if len > 0 {
                unsafe { sse2_copy_bytes(got.as_mut_ptr(), src.as_ptr(), len) };
                want.copy_from_slice(&src[..len]);
            }
            assert_eq!(got, want, "len={len}");
        }
    }

    /// #5: kernel SSE2 do tint tem paridade EXATA com o SWAR escalar, com
    /// cauda (aw%4 != 0) e várias combinações de k/tint.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn tint_sse2_matches_swar_scalar() {
        // Largura 11 → 2 blocos de 4 + cauda de 3. Altura 3.
        let (w, h) = (11usize, 3usize);
        let stride = w * 4;
        let tints: [(u8, u8, u8); 3] = [(8, 12, 24), (0xD4, 0xFF, 0x00), (200, 40, 90)];
        for (r, g, b) in tints {
            for &k in &[1u8, 37, 128, 216, 255] {
                for rgb_order in [false, true] {
                    let tint_word = if rgb_order {
                        u32::from_le_bytes([r, g, b, 0xFF])
                    } else {
                        u32::from_le_bytes([b, g, r, 0xFF])
                    };
                    let mut a = alloc::vec![0u8; stride * h];
                    fill_pseudo(&mut a, 0xABCD_0001);
                    let mut c = a.clone();
                    unsafe {
                        tint_swar_scalar(a.as_mut_ptr(), stride, 0, 0, w, h, tint_word, k as u32);
                        tint_sse2(c.as_mut_ptr(), stride, 0, 0, w, h, tint_word, k as u32);
                    }
                    assert_eq!(a, c, "rgb_order={rgb_order} k={k} tint={r},{g},{b}");
                }
            }
        }
    }

    /// #5: o dispatch público usa SSE2 no host sem alterar o resultado e
    /// escreve o retorno esperado.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn fill_rect_darken_tint_dispatch_writes_and_counts() {
        let gpu = super::GpuDevice::from_probe(0, 16, 8, 16, 4, false);
        let mut db = super::DoubleBuffer::from_gpu(&gpu);
        let n = db.fill_rect_darken_tint(0, 0, 16, 8, 8, 12, 24, 216);
        assert_eq!(n, 16 * 8);
        assert!(db.dirty);
    }

    /// #2 WC/NT: o streaming copy (dst 16 B-alinhado) é byte-idêntico ao
    /// `copy_nonoverlapping`, inclusive caudas não-múltiplas de 16.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn sse2_stream_copy_matches_scalar_for_aligned_len() {
        for &len in &[16usize, 17, 31, 32, 33, 100, 1000, 4095, 4096] {
            let mut src = alloc::vec![0u8; len];
            fill_pseudo(&mut src, 0x9E37_79B9 ^ len as u32);
            // +16 B extras e alinhamento manual: bulk WC alinhado (16 B).
            let mut raw = alloc::vec![0xEEu8; len + 16];
            let base = raw.as_mut_ptr();
            let aligned = ((base as usize + 15) & !15) as *mut u8;
            unsafe { sse2_stream_copy_bytes(aligned, src.as_ptr(), len) };
            let got = unsafe { core::slice::from_raw_parts(aligned, len) };
            assert_eq!(got, &src[..], "len={len}");
        }
    }
}


// revert-probe-12345
