//! GPU SPSC job ring — CPU enfileira jobs, GPU consome.
//! Ring buffer em páginas UC (uncacheable) com doorbell por vendor.
//! Head = onde a GPU leu ate (GPU atualiza), Tail = onde CPU escreveu (CPU atualiza).

use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::serial_println;
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
    pub head: u32,             // GPU leu ate aqui (consumer, polling)
    bar0_virt: u64,            // BAR0 virtual para doorbell
    doorbell: DoorbellFn,      // vendor-specific doorbell
    pub gpu_vendor: GpuVendor,
}

unsafe impl Send for GpuJobRing {}

/// Doorbell: Intel — escreve RENDER_RING_TAIL (offset 0x120038)
unsafe fn intel_doorbell(bar0_virt: u64, tail: u32) {
    core::ptr::write_volatile((bar0_virt + 0x120038) as *mut u32, tail);
}

/// Doorbell: NVIDIA — escreve PFIFO doorbell (offset 0x002000, PUSH_BUFFER)
unsafe fn nvidia_doorbell(bar0_virt: u64, tail: u32) {
    // NVIDIA PFIFO: PUSH_BUFFER tail register (channel 0, offset 0x002000)
    core::ptr::write_volatile((bar0_virt + 0x002000) as *mut u32, tail);
}

/// Doorbell: AMD — escreve PM4 doorbell (offset varia por geração)
unsafe fn amd_doorbell(bar0_virt: u64, tail: u32) {
    // AMD RDNA: doorbell no register 0x1B0 (Compute Queue Doorbell)
    core::ptr::write_volatile((bar0_virt + 0x1B0) as *mut u32, tail);
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
                serial_println!("[GPU-RING] {}: falha ao alocar ring buffer", gpu.name);
                return None;
            }
        };
        let ring_va = (ring_pa + pmoff) as *mut u32;
        core::ptr::write_bytes(ring_va, 0, RING_SIZE_BYTES);

        // Mapear ring buffer como UC para coerência DMA
        crate::apic::map_page_uc(ring_pa, pmoff);

        serial_println!("[GPU-RING] {}: ring={:#x} ({} KB) doorbell={:#x}",
            gpu.name, ring_pa, RING_SIZE_BYTES / 1024, bar0_virt);

        Some(GpuJobRing {
            ring_pa,
            ring_va,
            tail: 0,
            head: 0,
            bar0_virt,
            doorbell,
            gpu_vendor: gpu.vendor,
        })
    }

    /// Adiciona um job ao ring (CPU side, producer)
    pub fn push(&mut self, job: &GpuJob) -> bool {
        let idx = self.tail as usize;
        if idx + 4 > RING_SIZE_DWORDS as usize {
            serial_println!("[GPU-RING] ring full (tail={}, head={})", self.tail, self.head);
            return false;
        }
        unsafe {
            self.ring_va.add(idx).write_volatile(job.cmd);
            self.ring_va.add(idx + 1).write_volatile(job.arg0);
            self.ring_va.add(idx + 2).write_volatile(job.arg1);
            self.ring_va.add(idx + 3).write_volatile(job.arg2);
            fence(Ordering::Release);
        }
        self.tail = ((idx + 4) as u32) % RING_SIZE_DWORDS;
        true
    }

    /// Acorda GPU: escreve doorbell register
    pub unsafe fn ring_doorbell(&mut self) {
        fence(Ordering::SeqCst);
        (self.doorbell)(self.bar0_virt, self.tail);
    }

    /// Polla head avancar (GPU consumiu jobs)
    pub fn poll_head(&self, timeout: u32) -> bool {
        let target = self.tail;
        for _ in 0..timeout {
            let h = unsafe {
                fence(Ordering::Acquire);
                core::ptr::read_volatile((self.bar0_virt + 0x120034) as *const u32)
            };
            if h == target { return true; }
            core::hint::spin_loop();
        }
        false
    }

    /// Enfileira job + doorbell + poll completion
    pub unsafe fn submit_and_wait(&mut self, job: &GpuJob, timeout: u32) -> bool {
        if !self.push(job) { return false; }
        self.ring_doorbell();
        self.poll_head(timeout)
    }

    /// Le head atual (quanto a GPU já consumiu)
    pub fn head(&self) -> u32 { self.head }

    /// Jobs pendentes (tail - head)
    pub fn pending(&self) -> u32 {
        self.tail.wrapping_sub(self.head) % RING_SIZE_DWORDS
    }

    /// Estado do ring para debug
    pub fn status(&self) -> alloc::string::String {
        alloc::format!("[GPU-RING] tail={} head={} pending={} dwords",
            self.tail, self.head, self.pending())
    }
}

fn alloc_ring_pages(n: usize) -> Option<u64> {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    let alloc = guard.as_mut()?;
    let frame = alloc.allocate_contiguous(n)?;
    Some(frame.start_address().as_u64())
}
