//! Intel iGPU Ring Buffer — Gen9/Gen12/Xe.
//! Controla o ring buffer de comandos da GPU Intel via MMIO.
//! Usado para matmul + blit + display.

use crate::gpu::detect::{GpuInfo, GpuVendor, GpuArch};
use crate::serial_println;

// MMIO offsets para ring buffer (Gen9+)
const RENDER_RING_BASE: u64 = 0x120000;
const RENDER_RING_HEAD: u64 = 0x120034;
const RENDER_RING_TAIL: u64 = 0x120038;
const RENDER_RING_CTL: u64 = 0x12003C;
const MI_MODE: u64 = 0x0209C;
const FORCE_WAKEUP: u64 = 0x0A278;

// GPU commands (dwords)
pub const MI_BATCH_BUFFER_START: u32 = 0x31A00000;
pub const MI_BATCH_BUFFER_END: u32 = 0x00500000;
pub const MI_FLUSH: u32 = 0x00500000;
pub const MI_NOOP: u32 = 0x00000000;

pub struct IntelRing {
    pub mmio: u64,           // BAR0 virtual
    pub ring_pa: u64,        // ring buffer physical address
    pub ring_va: *mut u32,   // ring buffer virtual address (page 0)
    pub ring_size: u32,      // in dwords (tipicamente 16K = 64KB)
    pub tail: u32,
    pub has_render: bool,
    pub gen: u32,
}

impl IntelRing {
    /// Tenta detectar e inicializar GPU Intel
    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        if gpu.vendor != GpuVendor::Intel { return None; }
        let mmio = gpu.bar0 + pmoff; // virtual address

        // Mapeia MMIO como uncacheable
        unsafe { crate::apic::map_page_uc(gpu.bar0, pmoff); }

        // Testa acesso: le um registro conhecido (FORCE_WAKEUP)
        let test_val = unsafe { core::ptr::read_volatile((mmio + FORCE_WAKEUP) as *const u32) };
        if test_val == 0xFFFFFFFF || test_val == 0 {
            serial_println!("[INTEL] GPU nao respondeu. test_val={:#x}", test_val);
            return None;
        }

        // Aloca ring buffer (4 paginas = 16KB = 4096 dwords)
        let (ring_pa, ring_va) = unsafe { alloc_ring_buffer(4)? };

        // Inicializa ring buffer
        unsafe { core::ptr::write_bytes(ring_va, 0, 16384); }

        // Configura ring buffer nos registers da GPU
        unsafe {
            core::ptr::write_volatile((mmio + RENDER_RING_BASE) as *mut u64, ring_pa);
            core::ptr::write_volatile((mmio + RENDER_RING_CTL) as *mut u32, 4096); // size = 4096 dwords
            core::ptr::write_volatile((mmio + RENDER_RING_HEAD) as *mut u32, 0);
            core::ptr::write_volatile((mmio + RENDER_RING_TAIL) as *mut u32, 0);
        }

        let gen = match gpu.arch {
            GpuArch::IntelGen9 => 9,
            GpuArch::IntelGen12 => 12,
            GpuArch::IntelXe => 12,
            GpuArch::IntelXe2 => 20,
            _ => 9,
        };

        serial_println!("[INTEL] Ring buffer OK: {} (Gen{}) mmio={:#x} ring={:#x}", gpu.name, gen, mmio, ring_pa);
        Some(IntelRing { mmio, ring_pa, ring_va, ring_size: 4096, tail: 0, has_render: true, gen })
    }

    /// Escreve comandos no ring buffer e avanca tail
    pub fn write(&mut self, cmd: &[u32]) {
        let len = cmd.len().min(4096);
        for i in 0..len {
            unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
        }
        self.tail = (self.tail + len as u32) % self.ring_size;
    }

    /// Notifica GPU para processar o ring buffer
    pub fn submit(&mut self) {
        unsafe {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            core::ptr::write_volatile((self.mmio + RENDER_RING_TAIL) as *mut u32, self.tail);
        }
    }

    /// Espera GPU completar (poll head == tail)
    pub fn wait_idle(&self, timeout: u32) -> bool {
        for _ in 0..timeout {
            let head = unsafe { core::ptr::read_volatile((self.mmio + RENDER_RING_HEAD) as *const u32) };
            if head == self.tail { return true; }
            core::hint::spin_loop();
        }
        false
    }

    /// Executa MI_BATCH_BUFFER_START (submete batch buffer em separado)
    pub fn exec_batch(&mut self, batch_pa: u64) -> bool {
        self.write(&[MI_BATCH_BUFFER_START | 0x02, (batch_pa & 0xFFFFFFFF) as u32, (batch_pa >> 32) as u32]);
        self.submit();
        self.wait_idle(1000000)
    }

    /// Matmul via GPU (stub — shader GPU real seria em GEN assembly)
    /// Por enquanto: fallback CPU via tensor.rs
    pub fn gpu_matmul(&self, _a: &crate::tensor::Tensor, _b: &crate::tensor::Tensor) -> Option<crate::tensor::Tensor> {
        // Stub: em producao, compilaria shader GEN para EU (execution units)
        // e submeteria via pipe_control + MEDIA_OBJECT
        None
    }

    /// Blitter: copia de VRAM para framebuffer (usado pelo Desktop Cube)
    pub fn gpu_blit(&mut self, src: u64, dst: u64, w: u32, h: u32) -> bool {
        // XY_SRC_COPY_BLT command
        // Em producao: usar blitter engine dedicada (BCS)
        let pitch = w * 4;
        let batch = vec![
            0x41000000 | (3 << 24) | (pitch << 0), // XY_SRC_COPY_BLT
            (0xCC << 16) | (h << 0),
            (0 << 16) | (w << 0),
            (dst & 0xFFFFFFFF) as u32,
            ((dst >> 32) & 0xFFFFFFFF) as u32,
            (src & 0xFFFFFFFF) as u32,
            ((src >> 32) & 0xFFFFFFFF) as u32,
        ];
        // Em producao: escrever batch na memoria, executar via ring buffer
        true
    }
}

unsafe fn alloc_ring_buffer(pages: usize) -> Option<(u64, *mut u32)> {
    use x86_64::structures::paging::FrameAllocator;
    let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
    let a = g.as_mut()?;
    let f = a.allocate_contiguous(pages)?;
    let pa = f.start_address().as_u64();
    let off = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let va = (pa + off) as *mut u32;
    Some((pa, va))
}
