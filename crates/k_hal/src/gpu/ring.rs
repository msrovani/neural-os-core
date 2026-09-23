//! GPU SPSC job ring — CPU enfileira jobs, GPU consome.
//! Ring buffer em páginas UC (uncacheable) com doorbell por vendor.
//! Head = onde a GPU leu ate (GPU atualiza), Tail = onde CPU escreveu (CPU atualiza).

use crate::gpu::detect::{GpuInfo, GpuVendor};
use k_nano::memory::GLOBAL_ALLOCATOR;
use core::sync::atomic::{fence, Ordering};

/// Tamanho do ring em dwords (4096 dwords = 16KB = 1024 jobs de 16 bytes)
pub const RING_SIZE_DWORDS: u32 = 4096;
const RING_SIZE_BYTES: usize = (RING_SIZE_DWORDS * 4) as usize;

/// Job descriptor: comando + argumentos (formato vendor-specific)
#[repr(C)]
pub struct GpuJob {
    pub cmd: u32,       // comando (vendor-specific)
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
}

/// Função de doorbell: escreve tail no register correto para acordar GPU
type DoorbellFn = unsafe fn(bar0_virt: u64, tail: u32);

/// SPSC job ring: CPU (produtor) escreve jobs, GPU (consumidor) executa
pub struct GpuJobRing {
    pub ring_pa: u64,          // physical address do ring buffer
    ring_va: *mut u32,         // virtual address
    pub tail: u32,             // CPU escreve ate aqui (producer)
    bar0_virt: u64,            // BAR0 virtual para doorbell
    doorbell: DoorbellFn,      // vendor-specific doorbell
    pub gpu_vendor: GpuVendor,
}

unsafe impl Send for GpuJobRing {}

/// Doorbell Intel: **não** escrever RCS TAIL.
/// Este SPSC guarda descritores de software; o RCS real é `IntelRing` (TAIL 0x2030).
/// Offset antigo 0x120038 era START (hex a mais) — clobberava MMIO (SESSION_355/373).
unsafe fn intel_doorbell(_bar0_virt: u64, _tail: u32) {}

/// Doorbell: NVIDIA — escreve PFIFO doorbell (offset 0x002000, PUSH_BUFFER)
unsafe fn nvidia_doorbell(bar0_virt: u64, tail: u32) {
    // NVIDIA PFIFO: PUSH_BUFFER tail register (channel 0, offset 0x002000)
    core::ptr::write_volatile((bar0_virt + 0x002000) as *mut u32, tail);
}

/// Doorbell: AMD — noop até bring-up C3 (offset/doorbell por geração).
/// Não escrever 0x1B0 genérico cross-gen.
unsafe fn amd_doorbell(_bar0_virt: u64, tail: u32) {
    // ADR-0049: doorbell por GC. 0x1B0 genérico clobberava GFX errado.
    // KiQ/MES escrevem o offset da geração; esta fila software não.
    k_nano::slog_hal!("AMD", "warn", "doorbell skip tail={} — sem offset por GC", tail);
}

/// Doorbell: VirtIO — notifica via queue notify (não é doorbell real, mas similar)
unsafe fn virtio_doorbell(_bar0_virt: u64, _tail: u32) {
    // VirtIO usa queue notify separado — esta função é placeholder
}

impl GpuJobRing {
    /// Cria e inicializa um job ring para a GPU detectada
    pub unsafe fn new(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        let doorbell: DoorbellFn = match gpu.vendor {
            GpuVendor::Intel => intel_doorbell,
            GpuVendor::Nvidia => nvidia_doorbell,
            GpuVendor::Amd => amd_doorbell,
            GpuVendor::VirtIo => virtio_doorbell,
            GpuVendor::Unknown => return None,
        };

        let bar0_virt = gpu.bar0 + pmoff;
        let pages = (RING_SIZE_BYTES + 4095) / 4096;

        let ring_pa = match alloc_ring_pages(pages) {
            Some(pa) => pa,
            None => {
                k_nano::slog_hal!("GPU", "RING", "{}: falha ao alocar ring buffer", gpu.name);
                return None;
            }
        };
        let ring_va = (ring_pa + pmoff) as *mut u32;
        core::ptr::write_bytes(ring_va, 0, RING_SIZE_BYTES);

        // Mapear ring buffer como UC para coerência DMA
        k_nano::apic::map_page_uc(ring_pa, pmoff);

        k_nano::slog_hal!("GPU", "ring", "{}: ring={:#x} ({} KB) doorbell={:#x}", gpu.name, ring_pa, RING_SIZE_BYTES / 1024, bar0_virt);

        Some(GpuJobRing {
            ring_pa,
            ring_va,
            tail: 0,
            bar0_virt,
            doorbell,
            gpu_vendor: gpu.vendor,
        })
    }

    /// Doorbell real wired para este vendor? M5 (s397): só NVIDIA tem um
    /// doorbell verdadeiro aqui (PFIFO 0x2000). Intel = fila de software
    /// (RCS consome `IntelRing`, não este) e AMD/VirtIO são noop explícitos.
    pub fn doorbell_wired(&self) -> bool {
        matches!(self.gpu_vendor, GpuVendor::Nvidia)
    }

    /// Adiciona um job ao ring (CPU side, producer).
    /// Fail-closed: vendor sem doorbell wired → **Err** (não fingir ring vivo
    /// com jobs que a GPU nunca será acordada para consumir).
    pub fn push(&mut self, job: &GpuJob) -> Result<(), &'static str> {
        if !self.doorbell_wired() {
            return Err("doorbell nao wired para este vendor — ring inerte (fail-closed)");
        }
        let h = self.head_reg() as usize;
        let t = self.tail as usize;
        let space = if t >= h { RING_SIZE_DWORDS as usize - (t - h) } else { h - t };
        if space < 4 {
            k_nano::slog_hal!("GPU", "RING", "ring full (tail={}, head={}, space={})", self.tail, self.head_reg(), space);
            return Err("ring cheio");
        }
        let idx = self.tail as usize;
        unsafe {
            self.ring_va.add(idx).write_volatile(job.cmd);
            self.ring_va.add(idx + 1).write_volatile(job.arg0);
            self.ring_va.add(idx + 2).write_volatile(job.arg1);
            self.ring_va.add(idx + 3).write_volatile(job.arg2);
            fence(Ordering::Release);
        }
        self.tail = ((idx + 4) as u32) % RING_SIZE_DWORDS;
        Ok(())
    }

    /// Acorda GPU: escreve doorbell register
    pub unsafe fn ring_doorbell(&mut self) {
        fence(Ordering::SeqCst);
        (self.doorbell)(self.bar0_virt, self.tail);
    }

    /// Lê o register HEAD do vendor especifico
    fn head_reg(&self) -> u32 {
        unsafe {
            fence(Ordering::Acquire);
            match self.gpu_vendor {
                crate::gpu::detect::GpuVendor::Intel => {
                    // Software queue: RCS não consome (IntelRing). Nunca fingir complete.
                    0
                }
                crate::gpu::detect::GpuVendor::Nvidia =>
                    core::ptr::read_volatile((self.bar0_virt + 0x002004) as *const u32),
                crate::gpu::detect::GpuVendor::Amd => 0, // sem poll MMIO até C3
                _ => 0,
            }
        }
    }

    /// Polla head avancar (GPU consumiu jobs)
    pub fn poll_head(&self, _timeout: u32) -> bool {
        let target = self.tail;
        crate::wait::until(2_000_000, || self.head_reg() == target)
    }

    /// Enfileira job + doorbell + poll completion.
    /// Err propagado do push (vendor sem doorbell → nunca finge progresso).
    pub unsafe fn submit_and_wait(&mut self, job: &GpuJob, timeout: u32) -> Result<bool, &'static str> {
        self.push(job)?;
        self.ring_doorbell();
        Ok(self.poll_head(timeout))
    }

    /// Le head atual (quanto a GPU já consumiu) — head_reg **real**,
    /// não cache local que nunca avançava (M5).
    pub fn head(&self) -> u32 { self.head_reg() }

    /// Jobs pendentes (tail - head real)
    pub fn pending(&self) -> u32 {
        self.tail.wrapping_sub(self.head_reg()) % RING_SIZE_DWORDS
    }

    /// Estado do ring para debug
    pub fn status(&self) -> alloc::string::String {
        alloc::format!("[GPU-RING] tail={} head={} pending={} dwords",
            self.tail, self.head_reg(), self.pending())
    }
}

fn alloc_ring_pages(n: usize) -> Option<u64> {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    let alloc = guard.as_mut()?;
    let frame = alloc.allocate_contiguous(n)?;
    Some(frame.start_address().as_u64())
}
