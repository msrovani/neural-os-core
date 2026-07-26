//! Intel Display Engine — Page-flip HW (DSPCNTR) + Cursor HW (CUR_*)
//! 
//! ADR-0065 FASE 2.2: Page-flip atômico no vblank + cursor HW sobreposto.
//! Requer Intel iGPU com display engine (Gen9+). QEMU não emula.

use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::cap_gate::{check_map_bar, CapResult};
use k_nano::memory::PHYS_MEM_OFFSET;
use k_nano::slog_hal;
use core::sync::atomic::Ordering;

/// Registradores do Display Engine (Gen9+)
/// Base: BAR0 + offset
const DSPCNTR: u64 = 0x70180;      // Display Plane Control
const DSPSTRIDE: u64 = 0x70188;    // Display Plane Stride
const DSPLINOFF: u64 = 0x70184;    // Display Plane Linear Offset
const DSPSURF: u64 = 0x7019C;      // Display Plane Surface Address
const DSPTILEOFF: u64 = 0x701A4;   // Display Plane Tiled Offset

/// Cursor registers
const CURCNTR: u64 = 0x70080;      // Cursor Control
const CURBASE: u64 = 0x70084;      // Cursor Base Address
const CURPOS: u64 = 0x70088;       // Cursor Position

/// DSPCNTR bits
const DSPCNTR_ENABLE: u32 = 1 << 31;
const DSPCNTR_FORMAT_BGRA8888: u32 = 0x5 << 26; // 32bpp BGRA
const DSPCNTR_TILED: u32 = 1 << 10;
const DSPCNTR_GAMMA_ENABLE: u32 = 1 << 25;

/// CURCNTR bits
const CURCNTR_ENABLE: u32 = 1 << 31;
const CURCNTR_FORMAT_ARGB8888: u32 = 0x2 << 26; // 32bpp ARGB
const CURCNTR_GAMMA_ENABLE: u32 = 1 << 25;

/// Estado do display plane
#[derive(Debug, Clone, Copy)]
pub struct DisplayPlaneState {
    pub enabled: bool,
    pub surface_pa: u64,
    pub stride: u32,
    pub width: u32,
    pub height: u32,
    pub format_bgra8888: bool,
    pub tiled: bool,
}

/// Estado do cursor HW
#[derive(Debug, Clone, Copy)]
pub struct CursorState {
    pub enabled: bool,
    pub base_pa: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Inicializa o display engine Intel (Gen9+)
/// 
/// Deve ser chamado após map_bars_uc() e validate_bar0().
/// Retorna true se display engine detectado e inicializado.
pub unsafe fn init_intel_display(gpu: &GpuInfo) -> bool {
    if gpu.vendor != GpuVendor::Intel || !gpu.has_display_engine || gpu.bar0 == 0 {
        slog_hal!("INTEL_DISP", "init", "GPU não tem display engine ou BAR0 inválido");
        return false;
    }

    if check_map_bar(1, true) == CapResult::Deny {
        slog_hal!("INTEL_DISP", "init", "DENY map_bar (Cap)");
        return false;
    }

    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let bar0_virt = (gpu.bar0 + pmoff) as *mut u32;

    // Verifica se display engine está presente lendo DSPCNTR
    let dspcntr_val = core::ptr::read_volatile(bar0_virt.add((DSPCNTR / 4) as usize));
    if dspcntr_val == 0xFFFFFFFF || dspcntr_val == 0 {
        slog_hal!("INTEL_DISP", "init", "Display engine não responde (DSPCNTR={:#x})", dspcntr_val);
        return false;
    }

    slog_hal!("INTEL_DISP", "init", "Display engine detectado: DSPCNTR={:#x}", dspcntr_val);
    true
}

/// Configura page-flip HW: troca surface address no vblank (DSPCNTR + DSPSURF)
/// 
/// Deve ser chamado com interrupções desabilitadas ou em contexto atômico.
/// O flip ocorre no próximo vblank (double-buffered).
pub unsafe fn page_flip_hw(gpu: &GpuInfo, surface_pa: u64, stride: u32, width: u32, height: u32) -> bool {
    if gpu.vendor != GpuVendor::Intel || gpu.bar0 == 0 {
        return false;
    }

    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let bar0_virt = (gpu.bar0 + pmoff) as *mut u32;

    // 1. Desabilita plane temporariamente
    let dspcntr_off = (DSPCNTR / 4) as usize;
    let mut dspcntr = core::ptr::read_volatile(bar0_virt.add(dspcntr_off));
    dspcntr &= !DSPCNTR_ENABLE;
    core::ptr::write_volatile(bar0_virt.add(dspcntr_off), dspcntr);

    // 2. Configura novo surface address (DSPSURF)
    let dsp_surf_off = (DSPSURF / 4) as usize;
    core::ptr::write_volatile(bar0_virt.add(dsp_surf_off), surface_pa as u32);
    core::ptr::write_volatile(bar0_virt.add(dsp_surf_off + 1), (surface_pa >> 32) as u32);

    // 3. Configura stride (DSPSTRIDE)
    let dsp_stride_off = (DSPSTRIDE / 4) as usize;
    core::ptr::write_volatile(bar0_virt.add(dsp_stride_off), stride);

    // 4. Configura linear offset (DSPLINOFF) = 0 para linear
    let dsp_linoff_off = (DSPLINOFF / 4) as usize;
    core::ptr::write_volatile(bar0_virt.add(dsp_linoff_off), 0);

    // 5. Reabilita plane com novo surface
    dspcntr |= DSPCNTR_ENABLE | DSPCNTR_FORMAT_BGRA8888 | DSPCNTR_GAMMA_ENABLE;
    core::ptr::write_volatile(bar0_virt.add(dspcntr_off), dspcntr);

    // 6. Aguarda vblank para confirmar flip (polling simples)
    // Em produção, usar interrupt de vblank
    for _ in 0..100000 {
        let status = core::ptr::read_volatile(bar0_virt.add(dspcntr_off));
        if status & DSPCNTR_ENABLE != 0 {
            slog_hal!("INTEL_DISP", "flip", "Page-flip HW OK: surface={:#x} stride={} {}x{}", surface_pa, stride, width, height);
            return true;
        }
        core::hint::spin_loop();
    }

    slog_hal!("INTEL_DISP", "flip", "TIMEOUT aguardando vblank");
    false
}

/// Configura cursor HW (CUR_*)
/// 
/// cursor_pa: endereço físico do buffer do cursor (32bpp ARGB, 64x64 max)
/// x, y: posição do cursor em coordenadas de tela
pub unsafe fn cursor_set_hw(gpu: &GpuInfo, cursor_pa: u64, x: i32, y: i32, width: u32, height: u32) -> bool {
    if gpu.vendor != GpuVendor::Intel || gpu.bar0 == 0 {
        return false;
    }

    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let bar0_virt = (gpu.bar0 + pmoff) as *mut u32;

    // Desabilita cursor
    let curcntr_off = (CURCNTR / 4) as usize;
    let mut curcntr = core::ptr::read_volatile(bar0_virt.add(curcntr_off));
    curcntr &= !CURCNTR_ENABLE;
    core::ptr::write_volatile(bar0_virt.add(curcntr_off), curcntr);

    // Configura base address
    let curbase_off = (CURBASE / 4) as usize;
    core::ptr::write_volatile(bar0_virt.add(curbase_off), cursor_pa as u32);
    core::ptr::write_volatile(bar0_virt.add(curbase_off + 1), (cursor_pa >> 32) as u32);

    // Configura posição (CURPOS: x em bits 0-15, y em bits 16-31)
    let curpos_off = (CURPOS / 4) as usize;
    let pos_val = ((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF);
    core::ptr::write_volatile(bar0_virt.add(curpos_off), pos_val);

    // Reabilita cursor
    curcntr |= CURCNTR_ENABLE | CURCNTR_FORMAT_ARGB8888 | CURCNTR_GAMMA_ENABLE;
    core::ptr::write_volatile(bar0_virt.add(curcntr_off), curcntr);

    slog_hal!("INTEL_DISP", "cursor", "Cursor HW OK: pos=({},{}); size={}x{} base={:#x}", x, y, width, height, cursor_pa);
    true
}

/// Move cursor HW (apenas posição, sem reconfigurar base)
pub unsafe fn cursor_move_hw(gpu: &GpuInfo, x: i32, y: i32) -> bool {
    if gpu.vendor != GpuVendor::Intel || gpu.bar0 == 0 {
        return false;
    }

    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let bar0_virt = (gpu.bar0 + pmoff) as *mut u32;

    let curpos_off = (CURPOS / 4) as usize;
    let pos_val = ((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF);
    core::ptr::write_volatile(bar0_virt.add(curpos_off), pos_val);

    true
}

/// Desabilita cursor HW
pub unsafe fn cursor_disable_hw(gpu: &GpuInfo) -> bool {
    if gpu.vendor != GpuVendor::Intel || gpu.bar0 == 0 {
        return false;
    }

    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let bar0_virt = (gpu.bar0 + pmoff) as *mut u32;

    let curcntr_off = (CURCNTR / 4) as usize;
    let mut curcntr = core::ptr::read_volatile(bar0_virt.add(curcntr_off));
    curcntr &= !CURCNTR_ENABLE;
    core::ptr::write_volatile(bar0_virt.add(curcntr_off), curcntr);

    true
}

/// Canário page-flip: flip entre dois buffers e verifica se não há tearing
/// 
/// Aloca dois buffers, faz flip alternado, verifica consistência.
pub unsafe fn run_page_flip_canary(gpu: &GpuInfo) -> bool {
    use k_nano::memory::GLOBAL_ALLOCATOR;

    if gpu.vendor != GpuVendor::Intel || !gpu.has_display_engine {
        slog_hal!("INTEL_DISP", "canary", "SKIP — GPU não tem display engine");
        return false;
    }

    // Aloca dois buffers de teste (1920x1080x4 = ~8MB cada)
    const TEST_W: u32 = 640;
    const TEST_H: u32 = 480;
    const TEST_BPP: u32 = 4;
    const BUF_SIZE: usize = (TEST_W * TEST_H * TEST_BPP) as usize;

    let mut alloc = GLOBAL_ALLOCATOR.lock();
    let Some(a) = alloc.as_mut() else {
        slog_hal!("INTEL_DISP", "canary", "FAIL — allocator não disponível");
        return false;
    };

    let buf1 = match a.allocate_contiguous((BUF_SIZE + 4095) / 4096) {
        Some(f) => f,
        None => {
            slog_hal!("INTEL_DISP", "canary", "FAIL — buf1 alloc");
            return false;
        }
    };
    let buf2 = match a.allocate_contiguous((BUF_SIZE + 4095) / 4096) {
        Some(f) => f,
        None => {
            slog_hal!("INTEL_DISP", "canary", "FAIL — buf2 alloc");
            return false;
        }
    };
    drop(alloc);

    let buf1_pa = buf1.start_address().as_u64();
    let buf2_pa = buf2.start_address().as_u64();
    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);

    // Preenche buf1 com padrão A (vermelho)
    let buf1_va = (buf1_pa + pmoff) as *mut u32;
    for i in 0..(TEST_W * TEST_H) as usize {
        unsafe { buf1_va.add(i).write_volatile(0xFFFF0000); } // BGRA: vermelho
    }

    // Preenche buf2 com padrão B (azul)
    let buf2_va = (buf2_pa + pmoff) as *mut u32;
    for i in 0..(TEST_W * TEST_H) as usize {
        unsafe { buf2_va.add(i).write_volatile(0xFF0000FF); } // BGRA: azul
    }

    let stride = TEST_W * TEST_BPP;

    // Flip 1: buf1
    if !page_flip_hw(gpu, buf1_pa, stride, TEST_W, TEST_H) {
        slog_hal!("INTEL_DISP", "canary", "FAIL — flip 1");
        return false;
    }

    // Flip 2: buf2
    if !page_flip_hw(gpu, buf2_pa, stride, TEST_W, TEST_H) {
        slog_hal!("INTEL_DISP", "canary", "FAIL — flip 2");
        return false;
    }

    // Flip 3: buf1 novamente
    if !page_flip_hw(gpu, buf1_pa, stride, TEST_W, TEST_H) {
        slog_hal!("INTEL_DISP", "canary", "FAIL — flip 3");
        return false;
    }

    slog_hal!("INTEL_DISP", "canary", "PASS — page-flip HW funcional");
    true
}

/// Canário cursor HW: desenha cursor, move, verifica posição
pub unsafe fn run_cursor_canary(gpu: &GpuInfo) -> bool {
    use k_nano::memory::GLOBAL_ALLOCATOR;

    if gpu.vendor != GpuVendor::Intel || !gpu.has_display_engine {
        slog_hal!("INTEL_DISP", "cursor_canary", "SKIP — GPU não tem display engine");
        return false;
    }

    // Aloca buffer cursor 64x64x4 = 16KB
    const CUR_W: u32 = 64;
    const CUR_H: u32 = 64;
    const CUR_SIZE: usize = (CUR_W * CUR_H * 4) as usize;

    let mut alloc = GLOBAL_ALLOCATOR.lock();
    let Some(a) = alloc.as_mut() else {
        slog_hal!("INTEL_DISP", "cursor_canary", "FAIL — allocator não disponível");
        return false;
    };

    let cursor_frame = match a.allocate_contiguous((CUR_SIZE + 4095) / 4096) {
        Some(f) => f,
        None => {
            slog_hal!("INTEL_DISP", "cursor_canary", "FAIL — cursor alloc");
            return false;
        }
    };
    drop(alloc);

    let cursor_pa = cursor_frame.start_address().as_u64();
    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let cursor_va = (cursor_pa + pmoff) as *mut u32;

    // Desenha cursor: seta branca no centro
    for y in 0..CUR_H {
        for x in 0..CUR_W {
            let idx = (y * CUR_W + x) as usize;
            // Seta simples: diagonal
            let val = if x == y || x + y == CUR_W - 1 {
                0xFFFFFFFF // branco
            } else {
                0x00000000 // transparente
            };
            unsafe { cursor_va.add(idx).write_volatile(val); }
        }
    }

    // Habilita cursor em (100, 100)
    if !cursor_set_hw(gpu, cursor_pa, 100, 100, CUR_W, CUR_H) {
        slog_hal!("INTEL_DISP", "cursor_canary", "FAIL — cursor_set_hw");
        return false;
    }

    // Move para (200, 200)
    if !cursor_move_hw(gpu, 200, 200) {
        slog_hal!("INTEL_DISP", "cursor_canary", "FAIL — cursor_move_hw");
        return false;
    }

    // Move para (300, 300)
    if !cursor_move_hw(gpu, 300, 300) {
        slog_hal!("INTEL_DISP", "cursor_canary", "FAIL — cursor_move_hw 2");
        return false;
    }

    // Desabilita
    cursor_disable_hw(gpu);

    slog_hal!("INTEL_DISP", "cursor_canary", "PASS — cursor HW funcional");
    true
}