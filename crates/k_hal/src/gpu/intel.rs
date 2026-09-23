//! Intel iGPU/GPU Ring Buffer — Gen9/Gen12/Xe/Xe2.
//! Controla o ring buffer de comandos da GPU Intel via MMIO.
//! Usado para matmul + blit + display.

use crate::gpu::detect::{GpuInfo, GpuVendor, GpuArch};
use core::sync::atomic::{fence, Ordering};

// MMIO offsets — Gen9 RCS0 @ 0x2000 (i915_reg.h). Era 0x120000 (hex a mais,
// mesma classe do BCS 0x220000→0x22000, ADR-0087). TAIL=+0x30 HEAD=+0x34
// START=+0x38 CTL=+0x3C — o layout antigo tinha TAIL no slot de START.
const RENDER_RING_TAIL: u64 = 0x2030;
const RENDER_RING_HEAD: u64 = 0x2034;
const RENDER_RING_START: u64 = 0x2038;
const RENDER_RING_CTL: u64 = 0x203C;
/// RING_CTL 16KB: ((16384/4096)-1)<<12 | VALID = 0x3001 (não o size cru).
const RENDER_RING_CTL_16K: u32 = 0x3001;
/// HEAD/TAIL: offset em **bytes**; bits de wrap descartados no poll.
const RING_PTR_MASK: u32 = 0x001F_FFFC;
const FORCE_WAKEUP: u64 = 0x0A278;

/// Poll HEAD==TAIL. TSC 2s (SESSION_373).
fn wait_ring_idle(mmio: u64, head_off: u64, want: u32, _spin_fallback: u32) -> bool {
    crate::wait::until(2_000_000, || {
        let head = unsafe {
            core::ptr::read_volatile((mmio + head_off) as *const u32)
        } & RING_PTR_MASK;
        head == want
    })
}

// GPU commands (dwords) — H2 (onda3): validados contra i915
// `drivers/gpu/drm/i915/gt/intel_gpu_commands.h`:
//   MI_INSTR(opcode, flags) = (opcode << 23) | flags
//   MI_BATCH_BUFFER_START = MI_INSTR(0x31, 0) = 0x18800000 (era 0x31A00000 — errado)
//   MI_BATCH_BUFFER_END   = MI_INSTR(0x0A, 0) = 0x05000000 — NÃO emitir no *ring*
//     (o engine para e HEAD nunca alcança TAIL, ADR-0087).
//   MI_FLUSH_DW           = MI_INSTR(0x26, 1) = 0x4C000001 (3 dwords; grep-confirmado)
/// MI_INSTR da i915: opcode nos bits 28:23, flags/dword-len nos bits baixos.
const fn mi(opcode: u32, flags: u32) -> u32 {
    (opcode << 23) | flags
}

pub const MI_BATCH_BUFFER_START: u32 = mi(0x31, 0);
/// Gen8+: mesmo opcode com address de 64 bits (3 dwords) — `MI_INSTR(0x31, 1)`.
pub const MI_BATCH_BUFFER_START_GEN8: u32 = mi(0x31, 1);
pub const MI_BATCH_BUFFER_END: u32 = mi(0x0A, 0);
pub const MI_NOOP: u32 = 0;
/// Fase 3 ADR-0087: flush de caches pós-blit (coerência CPU↔GPU no BCS).
pub const MI_FLUSH_DW: u32 = mi(0x26, 1);

// MEDIA_OBJECT — submete compute shader para Execution Units
// i915: (0x3<<29)|(0x2<<27)|(0x1<<24)|(0x0<<16) = 0x71000000 (era 0x2A000000).
pub const MEDIA_OBJECT: u32 = 0x7100_0000;
/// GPGPU_WALKER (Gen9) — i915: (0x3<<29)|(0x2<<27)|(0x1<<24)|(0x5<<16).
pub const GPGPU_WALKER: u32 = 0x7105_0000;
/// COMPUTE_WALKER (Xe-HPG / Arc) — backend separado de Gen9.
pub const COMPUTE_WALKER: u32 = 0x2280_0000;

// PIPELINE_SELECT — i915: (0x3<<29)|(0x1<<27)|(0x1<<24)|(0x4<<16) = 0x69040000
// (era 0x30000000 — errado).
pub const PIPELINE_SELECT: u32 = 0x6904_0000;
const PIPELINE_SELECT_MEDIA: u32 = 0x0000_0001; // REG_BIT(0)

// STATE_BASE_ADDRESS — i915: (0x3<<29)|(0x1<<24)|(0x1<<16) = 0x61010000
pub const STATE_BASE_ADDRESS: u32 = 0x6101_0000;

pub struct IntelRing {
    pub mmio: u64,           // BAR0 virtual
    pub ring_pa: u64,        // ring buffer physical address
    pub ring_va: *mut u32,   // ring buffer virtual address (page 0)
    pub ring_size: u32,      // in dwords (4096 = 16KB)
    pub tail: u32,
    pub has_render: bool,
    pub gen: u32,
}

// IntelRing so contem um raw pointer + integers. Seguro para enviar entre cores.
unsafe impl Send for IntelRing {}

impl IntelRing {
    /// Tenta detectar e inicializar GPU Intel
    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        if gpu.vendor != GpuVendor::Intel { return None; }
        let mmio = gpu.bar0 + pmoff;
        // NOTA: map_bars_uc() já mapeou BAR0 inteiro como UC antes deste probe.
        // Acessar MMIO diretamente via pm_offset é seguro porque o PTE já é UC.

        // M7 (s397): FORCE_WAKEUP ler 0 é valor legítimo do registrador
        // (ECO Gen9) — NÃO reprova presença. Só 0xFFFF_FFFF é assinatura de
        // MMIO não mapeado (pull-up do barramento).
        let test_val = unsafe { core::ptr::read_volatile((mmio + FORCE_WAKEUP) as *const u32) };
        if test_val == 0xFFFFFFFF {
            k_nano::slog_hal!("INTEL", "warn", "GPU nao respondeu. test_val={:#x}", test_val);
            return None;
        }

        let (ring_pa, ring_va) = unsafe { alloc_ring_buffer(4)? };

        unsafe { core::ptr::write_bytes(ring_va, 0, 16384); }

        // GGTT primeiro — RING_START recebe offset GGTT (padrão BCS / i915 xcs_resume).
        let gtt_off = unsafe {
            let mut gtt = crate::gpu::intel_gtt::GgttPin::new(mmio);
            gtt.pin_sys(ring_pa, 4)
        };
        if gtt_off.is_none() {
            unsafe { init_gtt(mmio, ring_pa, 4); }
        }
        let start = gtt_off.unwrap_or(ring_pa);

        unsafe {
            core::ptr::write_volatile((mmio + RENDER_RING_START) as *mut u32, start as u32);
            core::ptr::write_volatile((mmio + RENDER_RING_CTL) as *mut u32, RENDER_RING_CTL_16K);
            core::ptr::write_volatile((mmio + RENDER_RING_HEAD) as *mut u32, 0);
            core::ptr::write_volatile((mmio + RENDER_RING_TAIL) as *mut u32, 0);
        }

        let gen = match gpu.arch {
            GpuArch::IntelGen9 => 9,
            GpuArch::IntelGen12 | GpuArch::IntelXe => 12,
            GpuArch::IntelXe2 => 20,
            _ => 9,
        };

        k_nano::slog_hal!(
            "GPU",
            "intel",
            "Ring OK: {} (Gen{}) mmio={:#x} ring={:#x} start_gtt={:#x}",
            gpu.name,
            gen,
            mmio,
            ring_pa,
            start
        );
        // ring_size = dwords; `tail` = offset em **bytes** (contrato HEAD/TAIL HW).
        Some(IntelRing {
            mmio,
            ring_pa,
            ring_va,
            ring_size: 4096,
            tail: 0,
            has_render: true,
            gen,
        })
    }

    /// Escreve comandos no ring buffer e avanca tail (bytes).
    pub fn write(&mut self, cmd: &[u32]) {
        let ring_bytes = self.ring_size.saturating_mul(4);
        let len = cmd.len().min(self.ring_size as usize);
        let dword_idx = (self.tail / 4) as usize;
        let wrap = (dword_idx + len).saturating_sub(self.ring_size as usize);
        if wrap > 0 {
            let first = len - wrap;
            for i in 0..first {
                unsafe {
                    self.ring_va.add(dword_idx + i).write_volatile(cmd[i]);
                }
            }
            for i in 0..wrap {
                unsafe {
                    self.ring_va.add(i).write_volatile(cmd[first + i]);
                }
            }
        } else {
            for i in 0..len {
                unsafe {
                    self.ring_va.add(dword_idx + i).write_volatile(cmd[i]);
                }
            }
        }
        self.tail = (self.tail + (len as u32).saturating_mul(4)) % ring_bytes;
    }

    /// Notifica GPU para processar o ring buffer
    pub fn submit(&mut self) {
        unsafe {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            core::ptr::write_volatile(
                (self.mmio + RENDER_RING_TAIL) as *mut u32,
                self.tail & RING_PTR_MASK,
            );
        }
    }

    /// Espera GPU completar (HEAD addr == TAIL addr, mask de wrap).
    /// SESSION_373: budget TSC 2s — spin count mente em TCG/WHPX.
    pub fn wait_idle(&self, timeout: u32) -> bool {
        wait_ring_idle(self.mmio, RENDER_RING_HEAD, self.tail & RING_PTR_MASK, timeout)
    }

    /// Executa MI_BATCH_BUFFER_START (submete batch buffer em separado).
    /// Gen8+: 3 dwords com endereço 64-bit (i915 MI_BATCH_BUFFER_START_GEN8).
    pub fn exec_batch(&mut self, batch_pa: u64) -> bool {
        self.write(&[
            MI_BATCH_BUFFER_START_GEN8,
            (batch_pa & 0xFFFFFFFF) as u32,
            (batch_pa >> 32) as u32,
        ]);
        self.submit();
        self.wait_idle(1000000)
    }

    /// Matmul via GPU (SESSION_274/373): sem MEDIA_OBJECT/GPGPU_WALKER
    /// (zebin KernelPack — Layer S). Não aloca NOOP shader. `None` = CPU
    /// fallback explícito em `backend::gpu_matmul` (não conta como GPU).
    pub fn gpu_matmul(&mut self, _a: &cortex::tensor::Tensor, _b: &cortex::tensor::Tensor) -> Option<cortex::tensor::Tensor> {
        k_nano::slog_hal!(
            "INTEL",
            "warn",
            "gpu_matmul None — MEDIA_OBJECT/GPGPU_WALKER Layer S (não fingir GPU)"
        );
        None
    }
}

// GTT (Graphics Translation Table) — GPU MMU que mapeia RAM do sistema.
// GMADR base tipicamente em 0x100000. GTT entries = primeiros 2MB da GMADR.
const GMADR_BASE: u64 = 0x100000;
const GFX_FLSH_CNTL: u64 = 0x101008;
const GTT_ENTRY_COUNT: usize = 512; // 512 entradas × 8 bytes = 4KB

/// Inicializa GTT para que a GPU enxergue paginas de RAM do sistema.
/// Escreve entradas GTT para o ring buffer e batch buffers.
pub unsafe fn init_gtt(mmio: u64, ring_pa: u64, ring_size_pages: u32) -> bool {
    if ring_size_pages == 0 || ring_size_pages as usize > GTT_ENTRY_COUNT {
        k_nano::slog_hal!("GPU", "warn", "init_gtt refuse pages={}", ring_size_pages);
        return false;
    }
    // GTT entries ficam no inicio da GMADR (primeiros 4KB = 512 entradas × 8 bytes)
    let gtt_base = mmio + GMADR_BASE;

    // Cada entrada GTT = 8 bytes: bits 63:12 = PFN (pa >> 12), bit 0 = PRESENT
    // Formato Gen9+: entry = pa | PRESENT. PA ja alinhado, bits 11:0 = 0.
    for i in 0..ring_size_pages {
        let pa = ring_pa + (i as u64) * 4096;
        let entry: u64 = pa | 0x1; // PFN bits 60:12 + PRESENT bit 0
        core::ptr::write_volatile((gtt_base + (i as u64) * 8) as *mut u64, entry);
    }

    // Flush GTT
    core::ptr::write_volatile((mmio + GFX_FLSH_CNTL) as *mut u32, 0);

    k_nano::slog_hal!("GPU", "gtt", "{} entradas escritas @ {:#x} para ring {:#x}", ring_size_pages, gtt_base, ring_pa);
    true
}

// BCS (Blitter Command Streamer) ring — engine dedicado para blit.
// BLT_RING_BASE = 0x22000 (i915_reg.h; Fase 3 ADR-0087: era 0x220000 — hex a mais).
// Offsets do i915: TAIL=+0x30, HEAD=+0x34, START=+0x38, CTL=+0x3C.
// RING_START recebe o GGTT offset do ring pinado (não endereço físico).
const BCS_RING_BASE: u64 = 0x22000;
const BCS_RING_TAIL: u64 = 0x22030;
const BCS_RING_HEAD: u64 = 0x22034;
const BCS_RING_START: u64 = 0x22038;
const BCS_RING_CTL: u64 = 0x2203C;
/// RING_CTL para ring de 16KB: ((16384/4096)-1)<<12 | RING_VALID = 0x3001
const BCS_RING_CTL_16K: u32 = 0x3001;

pub struct BcsRing {
    pub mmio: u64,
    pub ring_pa: u64,
    pub ring_va: *mut u32,
    pub ring_size: u32,
    pub tail: u32,
}

impl BcsRing {
    pub fn probe(mmio_base: u64) -> Option<Self> {
        let mmio = mmio_base;
        let (ring_pa, ring_va) = unsafe { alloc_ring_buffer(4)? };

        unsafe {
            core::ptr::write_bytes(ring_va, 0, 16384);
            // Pin GGTT (ADR-0050 P2) — RING_START recebe o GGTT offset do ring,
            // não endereço físico (Fase 3 ADR-0087; i915 xcs_resume).
            let gtt_off = {
                let mut gtt = crate::gpu::intel_gtt::GgttPin::new(mmio);
                gtt.pin_sys(ring_pa, 4)
            };
            if gtt_off.is_none() {
                // Fallback legado se pin WOPCM falhar (índices esgotados).
                crate::gpu::intel::init_gtt(mmio, ring_pa, 4);
            }
            let start = gtt_off.unwrap_or(ring_pa);
            core::ptr::write_volatile((mmio + BCS_RING_START) as *mut u32, start as u32);
            core::ptr::write_volatile((mmio + BCS_RING_CTL) as *mut u32, BCS_RING_CTL_16K);
            core::ptr::write_volatile((mmio + BCS_RING_HEAD) as *mut u32, 0);
            core::ptr::write_volatile((mmio + BCS_RING_TAIL) as *mut u32, 0);
        }
        k_nano::slog_hal!("BCS", "ok", "Blitter ring at {:#x} size 4096 dw (GTT pinned)", ring_pa);
        Some(BcsRing { mmio, ring_pa, ring_va, ring_size: 4096, tail: 0 })
    }

    pub fn write(&mut self, cmd: &[u32]) {
        let ring_bytes = self.ring_size.saturating_mul(4);
        let len = cmd.len().min(self.ring_size as usize);
        let dword_idx = (self.tail / 4) as usize;
        let wrap = (dword_idx + len).saturating_sub(self.ring_size as usize);
        if wrap > 0 {
            let first = len - wrap;
            for i in 0..first {
                unsafe {
                    self.ring_va.add(dword_idx + i).write_volatile(cmd[i]);
                }
            }
            for i in 0..wrap {
                unsafe {
                    self.ring_va.add(i).write_volatile(cmd[first + i]);
                }
            }
        } else {
            for i in 0..len {
                unsafe {
                    self.ring_va.add(dword_idx + i).write_volatile(cmd[i]);
                }
            }
        }
        // `tail` = bytes (contrato HEAD/TAIL HW), não índice dword.
        self.tail = (self.tail + (len as u32).saturating_mul(4)) % ring_bytes;
    }

    pub fn submit(&mut self) {
        unsafe {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            core::ptr::write_volatile(
                (self.mmio + BCS_RING_TAIL) as *mut u32,
                self.tail & RING_PTR_MASK,
            );
        }
    }

    pub fn wait_idle(&self, timeout: u32) -> bool {
        wait_ring_idle(self.mmio, BCS_RING_HEAD, self.tail & RING_PTR_MASK, timeout)
    }

    /// Executa blit no BCS ring (XY_SRC_COPY_BLT, Gen9 64-bit, 32bpp, untiled).
    /// Fase 3 ADR-0087: encoding correta do i-g-t/i915 (intel_batchbuffer.c) —
    /// header 0x54C00000 (opcode 0x53, não 0x41!), WRITE_A|WRITE_RGB, length 8.
    pub fn blit(&mut self, src: u64, dst: u64, w: u32, h: u32, bpp: u32) -> bool {
        let pitch = w * bpp;
        let cmd = [
            0x54F00008,                                     // DW0: SRC_COPY_BLT 64-bit + len 8
            (3 << 24) | (pitch & 0xFFFF),                   // DW1: depth 32bpp (bits 25:24) + dst_pitch
            (0 << 16) | 0,                                  // DW2: dst_x=0, dst_y=0
            ((h & 0xFFFF) << 16) | (w & 0xFFFF),            // DW3: dst_x2=w, dst_y2=h
            (dst & 0xFFFFFFFF) as u32,                      // DW4: dst_addr lo
            ((dst >> 32) & 0xFFFFFFFF) as u32,              // DW5: dst_addr hi
            (0 << 16) | 0,                                  // DW6: src_x=0, src_y=0
            (pitch & 0xFFFF),                               // DW7: src_pitch
            (src & 0xFFFFFFFF) as u32,                      // DW8: src_addr lo
            ((src >> 32) & 0xFFFFFFFF) as u32,              // DW9: src_addr hi
            MI_FLUSH_DW,                                    // flush caches pós-cópia (coerência CPU)
            0,                                              // addr = 0 (sem store)
            0,                                              // data = 0
        ];
        // ponytail: submissão por ring NÃO usa MI_BATCH_BUFFER_END (engine pararia
        // nele e HEAD nunca alcança TAIL → wait_idle timeout). O ring vazio ⟺
        // HEAD==TAIL; o flush final garante coerência antes do poll.
        self.write(&cmd);
        self.submit();
        self.wait_idle(1000000)
    }
}

// ─── Compute kernel dispatch via ring buffer ──────────────────────────────

/// Submete compute no RCS. Sem MEDIA_OBJECT/GPGPU_WALKER (Layer S) — não
/// emitir MI_BATCH_BUFFER_END no ring (HEAD nunca alcança TAIL) nem fingir
/// dispatch com NOOP. Caller faz CPU fallback.
pub unsafe fn dispatch_compute(
    _ring: &mut IntelRing,
    _a_addr: u64,
    _b_addr: u64,
    _out_addr: u64,
    m: u32,
    k: u32,
    n: u32,
) -> bool {
    k_nano::slog_hal!(
        "INTEL",
        "warn",
        "dispatch_compute None — MEDIA_OBJECT/GPGPU_WALKER Layer S m={} k={} n={}",
        m,
        k,
        n
    );
    false
}

unsafe impl Send for BcsRing {}

unsafe fn alloc_ring_buffer(pages: usize) -> Option<(u64, *mut u32)> {
    
    let mut g = k_nano::memory::GLOBAL_ALLOCATOR.lock();
    let a = g.as_mut()?;
    let f = a.allocate_contiguous(pages)?;
    let pa = f.start_address().as_u64();
    if pa & 0xFFF != 0 {
        k_nano::slog_hal!("INTEL", "warn", "WARNING: ring buffer not page-aligned! {:#x}", pa);
    }
    let off = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let va = (pa + off) as *mut u32;
    Some((pa, va))
}

/// Gen9 / iGPU canário: GPGPU_WALKER via `intel_gen9` (ring vivo preferido).
pub unsafe fn try_vector_add_gen9(
    gpu: &GpuInfo,
    zebin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    crate::gpu::intel_gen9::try_vector_add_gen9_standalone(gpu, zebin, a, b, expect)
}

/// Canário com handle vivo (Degrau Gen9) + GGTT WOPCM.
pub unsafe fn try_vector_add_gen9_ring(
    ring: &mut IntelRing,
    zebin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    let mut gtt = crate::gpu::intel_gtt::GgttPin::new(ring.mmio);
    crate::gpu::intel_gen9::dispatch_vector_add_gen9(ring, &mut gtt, zebin, a, b, expect)
}

/// Arc / Xe-HPG: GuC + CCS + COMPUTE_WALKER — backend separado.
pub unsafe fn try_vector_add_arc(
    gpu: &GpuInfo,
    zebin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    crate::gpu::intel_arc::try_vector_add_arc(gpu, zebin, a, b, expect)
}

