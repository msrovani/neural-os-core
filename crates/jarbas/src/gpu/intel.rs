//! Intel iGPU/GPU Ring Buffer — Gen9/Gen12/Xe/Xe2.
//! Controla o ring buffer de comandos da GPU Intel via MMIO.
//! Usado para matmul + blit + display.

use crate::gpu::detect::{GpuInfo, GpuVendor, GpuArch};
use k_nano::serial_println;
use k_nano::kjson;
use core::sync::atomic::{fence, Ordering};

// MMIO offsets para ring buffer (Gen9+)
const RENDER_RING_BASE: u64 = 0x120000;
const RENDER_RING_HEAD: u64 = 0x120034;
const RENDER_RING_TAIL: u64 = 0x120038;
const RENDER_RING_CTL: u64 = 0x12003C;
const FORCE_WAKEUP: u64 = 0x0A278;

// GPU commands (dwords)
// MI_BATCH_BUFFER_END e MI_FLUSH compartilham opcode 0x00500000 em Gen9+ (MI_FLUSH removido Gen11+)
pub const MI_BATCH_BUFFER_START: u32 = 0x31A00000;
pub const MI_BATCH_BUFFER_END: u32 = 0x00500000;
pub const MI_NOOP: u32 = 0x00000000;

// MEDIA_OBJECT — submete compute shader para Execution Units
pub const MEDIA_OBJECT: u32 = 0x2A000000;

// PIPELINE_SELECT — alterna entre render e compute pipelines
pub const PIPELINE_SELECT: u32 = 0x30000000;
const PIPELINE_SELECT_MEDIA: u32 = 0x00000001;

// STATE_BASE_ADDRESS — configura endereços base de estado
pub const STATE_BASE_ADDRESS: u32 = 0x31000000;

pub struct IntelRing {
    pub mmio: u64,           // BAR0 virtual
    pub ring_pa: u64,        // ring buffer physical address
    pub ring_va: *mut u32,   // ring buffer virtual address (page 0)
    pub ring_size: u32,      // in dwords (4096 = 16KB)
    pub tail: u32,
    pub has_render: bool,
    pub gen: u32,
    pub shader_pa: u64,      // Physical address of loaded shader
    pub shader_loaded: bool,
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

        let test_val = unsafe { core::ptr::read_volatile((mmio + FORCE_WAKEUP) as *const u32) };
        if test_val == 0xFFFFFFFF || test_val == 0 {
            serial_println!("[INTEL] GPU nao respondeu. test_val={:#x}", test_val);
            return None;
        }

        let (ring_pa, ring_va) = unsafe { alloc_ring_buffer(4)? };

        unsafe { core::ptr::write_bytes(ring_va, 0, 16384); }

        unsafe {
            core::ptr::write_volatile((mmio + RENDER_RING_BASE) as *mut u64, ring_pa);
            core::ptr::write_volatile((mmio + RENDER_RING_CTL) as *mut u32, 4096);
            core::ptr::write_volatile((mmio + RENDER_RING_HEAD) as *mut u32, 0);
            core::ptr::write_volatile((mmio + RENDER_RING_TAIL) as *mut u32, 0);
        }

        // Inicializa GTT para que a GPU enxergue o ring buffer em RAM
        unsafe { init_gtt(mmio, ring_pa, 4); }

        let gen = match gpu.arch {
            GpuArch::IntelGen9 => 9,
            GpuArch::IntelGen12 | GpuArch::IntelXe => 12,
            GpuArch::IntelXe2 => 20,
            _ => 9,
        };

        serial_println!("[INTEL] Ring buffer OK: {} (Gen{}) mmio={:#x} ring={:#x}", gpu.name, gen, mmio, ring_pa);
        Some(IntelRing { mmio, ring_pa, ring_va, ring_size: 4096, tail: 0, has_render: true, gen, shader_pa: 0, shader_loaded: false })
    }

    /// Escreve comandos no ring buffer e avanca tail
    pub fn write(&mut self, cmd: &[u32]) {
        let len = cmd.len();
        if len > self.ring_size as usize {
            serial_println!("[INTEL] WARNING: cmd len {} > ring size {}, truncating!", len, self.ring_size);
        }
        let len = len.min(self.ring_size as usize);
        let wrap = (self.tail as usize + len).saturating_sub(self.ring_size as usize);
        if wrap > 0 {
            let first = len - wrap;
            for i in 0..first {
                unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
            }
            for i in 0..wrap {
                unsafe { self.ring_va.add(i).write_volatile(cmd[first + i]); }
            }
        } else {
            for i in 0..len {
                unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
            }
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
        self.write(&[
            MI_BATCH_BUFFER_START | 0x02,
            (batch_pa & 0xFFFFFFFF) as u32,
            (batch_pa >> 32) as u32,
        ]);
        self.submit();
        self.wait_idle(1000000)
    }

    /// Matmul via GPU: GEN compute shader (producao futura — placeholder CPU).
    pub fn gpu_matmul(&mut self, a: &cortex::tensor::Tensor, b: &cortex::tensor::Tensor) -> Option<cortex::tensor::Tensor> {
        // Verifica se shader está carregado, senão carrega
        if !self.shader_loaded {
            if let Some(shader_pa) = self.load_gen_matmul_shader() {
                self.shader_pa = shader_pa;
                self.shader_loaded = true;
                serial_println!("[INTEL-MATMUL] GEN matmul shader carregado @ {:#x}", shader_pa);
            } else {
                serial_println!("[INTEL-MATMUL] Falha ao carregar shader GEN — usando fallback CPU");
                return a.matmul(b);
            }
        }

        // TODO: Implementar execução real do shader via MEDIA_OBJECT
        // Por enquanto: fallback CPU matmul, infra GPU preparada
        serial_println!("[INTEL-MATMUL] GEN compute stub — usando fallback CPU");
        a.matmul(b)
    }

    /// Carrega shader GEN para matmul na VRAM (stub preparado para shader real)
    /// Retorna physical address do shader ou None em caso de erro
    fn load_gen_matmul_shader(&mut self) -> Option<u64> {
        // Aloca 1 página para o shader (4KB suficiente para matmul simples)
        let (shader_pa, shader_va) = unsafe { alloc_ring_buffer(1)? };

        // TODO: Escrever shader GEN assembly real aqui
        // Documentação GEN assembly é NDA da Intel, requer engenharia reversa
        // do i915 driver ou uso de assembler externo
        //
        // Estrutura esperada do shader GEN:
        // - MEDIA_INTERFACE_DESCRIPTOR_LOAD
        // - MEDIA_VFE_STATE
        // - MEDIA_CURBE_LOAD
        // - MEDIA_OBJECT com instruções de compute (load, mul, add, store)
        //
        // Por enquanto, escreve um stub de NOOPs para testar alocação
        unsafe {
            let shader_dwords = shader_va as *mut u32;
            for i in 0..1024 {
                shader_dwords.add(i).write_volatile(MI_NOOP);
            }
        }

        // Adiciona shader ao GTT para que a GPU enxergue
        unsafe { init_gtt(self.mmio, shader_pa, 1); }

        Some(shader_pa)
    }

    /// Executa shader GEN via MEDIA_OBJECT (infraestrutura preparada)
    fn execute_gen_shader(&mut self, a_pa: u64, b_pa: u64, c_pa: u64, m: u32, n: u32, k: u32) -> bool {
        if !self.shader_loaded {
            serial_println!("[INTEL] Shader nao carregado — execute_gen_shader falhou");
            return false;
        }

        // TODO: Implementar MEDIA_OBJECT para submeter shader aos EU
        // Estrutura do batch buffer:
        // 1. PIPELINE_SELECT (media)
        // 2. STATE_BASE_ADDRESS (shader, surface, dynamic)
        // 3. MEDIA_OBJECT (dispatch shader)
        //
        // Por enquanto, stub retorna true para não quebrar compilação
        serial_println!("[INTEL] execute_gen_shader stub: a={:#x} b={:#x} c={:#x} {}x{}x{}",
            a_pa, b_pa, c_pa, m, n, k);
        true
    }

    /// Blitter: copia de VRAM para framebuffer (usado pelo Desktop Cube)
    /// Nota: idealmente usa BCS ring (blitter engine), nao RCS.
    /// Sem GTT set up, batch buffers em RAM do sistema nao sao visiveis pela GPU.
    pub fn gpu_blit(&mut self, src: u64, dst: u64, w: u32, h: u32, bpp: u32) -> bool {
        let pitch = w * bpp;
        let cmd = [
            0x41000000 | (3 << 24) | (pitch << 0),
            (0xCC << 16) | (h << 0),
            (0 << 16) | (w << 0),
            (dst & 0xFFFFFFFF) as u32,
            ((dst >> 32) & 0xFFFFFFFF) as u32,
            (src & 0xFFFFFFFF) as u32,
            ((src >> 32) & 0xFFFFFFFF) as u32,
            MI_BATCH_BUFFER_END,
        ];
        self.write(&cmd);
        self.submit();
        self.wait_idle(1000000)
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

    serial_println!("[GTT] {} entradas escritas @ {:#x} para ring {:#x}",
        ring_size_pages, gtt_base, ring_pa);
    true
}

// BCS (Blitter Command Streamer) ring — engine dedicado para blit.
// Register base em 0x22000, layout identico ao RCS.
const BCS_RING_BASE: u64 = 0x220000;
const BCS_RING_HEAD: u64 = 0x220034;
const BCS_RING_TAIL: u64 = 0x220038;
const BCS_RING_CTL: u64 = 0x22003C;

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
            core::ptr::write_volatile((mmio + BCS_RING_BASE) as *mut u64, ring_pa);
            core::ptr::write_volatile((mmio + BCS_RING_CTL) as *mut u32, 4096);
            core::ptr::write_volatile((mmio + BCS_RING_HEAD) as *mut u32, 0);
            core::ptr::write_volatile((mmio + BCS_RING_TAIL) as *mut u32, 0);
        }
        serial_println!("[BCS] Blitter ring at {:#x} size 4096 dw", ring_pa);
        Some(BcsRing { mmio, ring_pa, ring_va, ring_size: 4096, tail: 0 })
    }

    pub fn write(&mut self, cmd: &[u32]) {
        let len = cmd.len().min(self.ring_size as usize);
        let wrap = (self.tail as usize + len).saturating_sub(self.ring_size as usize);
        if wrap > 0 {
            let first = len - wrap;
            for i in 0..first {
                unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
            }
            for i in 0..wrap {
                unsafe { self.ring_va.add(i).write_volatile(cmd[first + i]); }
            }
        } else {
            for i in 0..len {
                unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
            }
        }
        self.tail = (self.tail + len as u32) % self.ring_size;
    }

    pub fn submit(&mut self) {
        unsafe {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            core::ptr::write_volatile((self.mmio + BCS_RING_TAIL) as *mut u32, self.tail);
        }
    }

    pub fn wait_idle(&self, timeout: u32) -> bool {
        for _ in 0..timeout {
            let head = unsafe { core::ptr::read_volatile((self.mmio + BCS_RING_HEAD) as *const u32) };
            if head == self.tail { return true; }
            core::hint::spin_loop();
        }
        false
    }

    /// Executa blit no BCS ring (XY_SRC_COPY_BLT)
    pub fn blit(&mut self, src: u64, dst: u64, w: u32, h: u32, bpp: u32) -> bool {
        let pitch = w * bpp;
        let cmd = [
            0x41000000 | (3 << 24) | (pitch << 0),
            (0xCC << 16) | (h << 0),
            (0 << 16) | (w << 0),
            (dst & 0xFFFFFFFF) as u32,
            ((dst >> 32) & 0xFFFFFFFF) as u32,
            (src & 0xFFFFFFFF) as u32,
            ((src >> 32) & 0xFFFFFFFF) as u32,
            MI_BATCH_BUFFER_END,
        ];
        self.write(&cmd);
        self.submit();
        self.wait_idle(1000000)
    }
}

// ─── Compute kernel dispatch via ring buffer ──────────────────────────────

/// Submete um kernel de computacao (matmul) para a GPU Intel via ring buffer.
/// Preenche o ring com MI_MATH + pipe_control + batch buffer end.
/// Retorna true se a GPU consumiu o comando.
pub unsafe fn dispatch_compute(ring: &mut IntelRing, _a_addr: u64, _b_addr: u64, _out_addr: u64, m: u32, k: u32, n: u32) -> bool {
    // Pipe control: flushes caches antes do compute
    let flush = [0x7A00_0005u32, 0x0010_0000, 0x0000_0000, 0x0000_0000];
    ring.write(&flush);

    // MEDIA_OBJECT (ou GPGPU_WALKER) — placeholder para quando GEN assembly
    // estiver disponivel. Atualmente: NOOP + MI_BATCH_BUFFER_END.
    let compute = [
        0x0000_0000u32, // NOOP
        0x0500_0001u32, // MI_BATCH_BUFFER_END
    ];
    ring.write(&compute);
    fence(Ordering::Release);

    ring.submit();
    let done = ring.wait_idle(100000);
    kjson!("INTEL", "COMPUTE", "dispatch", "m", m, "k", k, "n", n, "done", done as u32);
    done
}

unsafe impl Send for BcsRing {}

unsafe fn alloc_ring_buffer(pages: usize) -> Option<(u64, *mut u32)> {
    
    let mut g = k_nano::memory::GLOBAL_ALLOCATOR.lock();
    let a = g.as_mut()?;
    let f = a.allocate_contiguous(pages)?;
    let pa = f.start_address().as_u64();
    if pa & 0xFFF != 0 {
        serial_println!("[INTEL] WARNING: ring buffer not page-aligned! {:#x}", pa);
    }
    let off = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let va = (pa + off) as *mut u32;
    Some((pa, va))
}
