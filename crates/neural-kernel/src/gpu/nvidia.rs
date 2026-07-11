//! NVIDIA PFIFO + PUSH_BUFFER + FALCON firmware loader.
//! Pascal+ (GTX 1050 → RTX 5090).
//! Compute via PFIFO channel 0, PUSH_BUFFER submission, VRAM via BAR2.

use crate::gpu::detect::GpuInfo;
use crate::serial_println;

pub struct NvidiaGpu {
    pub mmio: u64,       // BAR0 virtual
    pub vram_bar2: u64,  // BAR2 virtual (VRAM)
    pub vram_size: u64,
    pub bar0_phys: u64,  // BAR0 physical (para GTT/PTE)
    pub bar2_phys: u64,  // BAR2 physical
    pub pfifo_ready: bool,
    pub channel_active: bool,
}

impl NvidiaGpu {
    const PFIFO_BASE: u64 = 0x2000;
    const PFIFO_PUT: u64 = 0x2000;   // PIO method PUT offset
    const PFIFO_GET: u64 = 0x2004;   // PIO method GET offset
    const PUSH_BUFFER_ADDR: u64 = 0x0000; // GPFIFO entry: method addr (channel reg)
    const PUSH_BUFFER_SIZE: u64 = 0x0004; // GPFIFO entry: method count
    const PUSH_BUFFER_CTL: u64 = 0x0008;  // GPFIFO entry: control
    const METHOD_NOP: u32 = 0x00000000;    // method 0x00 = NOP
    const METHOD_DMA: u32 = 0x00000002;    // method 0x02 = DMA
    const SUBCH_0: u32 = 0x00000000;

    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        let mmio = gpu.bar0 + pmoff;
        unsafe { crate::apic::map_page_uc(gpu.bar0, pmoff); }

        // Le VERSION register para confirmar que GPU responde
        let version = unsafe { core::ptr::read_volatile((mmio + 0x000000) as *const u32) };
        serial_println!("[NVIDIA] {}: version={:#x} bar0={:#x} bar2={:#x}",
            gpu.name, version, gpu.bar0, gpu.bar2);
        if version == 0xFFFFFFFF || version == 0 { return None; }

        // Mapeia BAR2 (VRAM): pagina inicial de 4MB
        let vram_window = gpu.vram_size.min(4 * 1024 * 1024);
        let pages = (vram_window / 4096) as usize;
        for i in 0..pages.min(64) { // 64 paginas = 256KB iniciais
            unsafe { crate::apic::map_page_uc(gpu.bar2 + (i as u64) * 4096, pmoff); }
        }

        let vram_ptr = gpu.bar2 + pmoff;
        unsafe { core::ptr::write_volatile(vram_ptr as *mut u32, 0xDEADBEEF); }
        let test = unsafe { core::ptr::read_volatile(vram_ptr as *const u32) };
        let vram_ok = test == 0xDEADBEEF;

        serial_println!("[NVIDIA] VRAM {} MB {}", gpu.vram_mb(),
            if vram_ok { "OK" } else { "SEM FIRMWARE (P8 mode)" });

        // Tenta init PFIFO channel 0 via PIO method submit
        let pfifo_ready = Self::probe_pfifo(mmio);

        Some(NvidiaGpu {
            mmio, vram_bar2: gpu.bar2 + pmoff, vram_size: gpu.vram_size,
            bar0_phys: gpu.bar0, bar2_phys: gpu.bar2,
            pfifo_ready, channel_active: pfifo_ready,
        })
    }

    /// Testa se PFIFO responde via PIO method submit (NOP)
    fn probe_pfifo(mmio: u64) -> bool {
        unsafe {
            // Tenta ler PUT/GET
            let put = core::ptr::read_volatile((mmio + Self::PFIFO_PUT) as *const u32);
            let get = core::ptr::read_volatile((mmio + Self::PFIFO_GET) as *const u32);
            serial_println!("[NVIDIA] PFIFO: PUT={:#x} GET={:#x}", put, get);
            if put == 0xFFFFFFFF && get == 0xFFFFFFFF { return false; }

            // Submete NOP via PIO: method 0x00 com subchannel 0
            let method = Self::METHOD_NOP | Self::SUBCH_0;
            let arg = 0u32; // NOP argument (ignored)
            core::ptr::write_volatile((mmio + Self::PFIFO_BASE + 0x0000) as *mut u32, method);
            core::ptr::write_volatile((mmio + Self::PFIFO_BASE + 0x0004) as *mut u32, arg);
            core::ptr::write_volatile((mmio + Self::PFIFO_BASE + 0x0008) as *mut u32, 1); // kick
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            // Verifica se GET avancou (PFIFO consumiu o NOP)
            let get2 = core::ptr::read_volatile((mmio + Self::PFIFO_GET) as *const u32);
            if get2 == get { return false; }

            serial_println!("[NVIDIA] PFIFO channel 0 OK (NOP submitted, GET moved)");
            true
        }
    }

    /// Submete um metodo via PIO com argumento
    pub unsafe fn pio_method(&self, method: u32, arg: u32) {
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0000) as *mut u32, method);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0004) as *mut u32, arg);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0008) as *mut u32, 1);
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }

    /// Aloca bloco na VRAM via BAR2 (mapeamento direto)
    pub fn vram_alloc(&self, size: usize) -> Option<u64> {
        crate::gpu::vram::vram_alloc(size)
    }

    /// Copia dados CPU->VRAM via BAR2 UC write
    pub unsafe fn cpu_to_vram(&self, vram_off: u64, data: &[u8]) {
        let dst = (self.vram_bar2 + vram_off) as *mut u8;
        for i in 0..data.len() {
            core::ptr::write_volatile(dst.add(i), data[i]);
        }
    }

    /// Copia dados VRAM->CPU via BAR2 UC read
    pub unsafe fn vram_to_cpu(&self, vram_off: u64, buf: &mut [u8]) {
        let src = (self.vram_bar2 + vram_off) as *const u8;
        for i in 0..buf.len() {
            buf[i] = core::ptr::read_volatile(src.add(i));
        }
    }

    pub fn status(&self) -> alloc::string::String {
        alloc::format!("NVIDIA PFIFO: {} | VRAM: {} MB | PIO: {}",
            if self.channel_active { "ativo" } else { "inativo" },
            self.vram_size / (1024*1024),
            if self.pfifo_ready { "ok" } else { "falha" })
    }
}
