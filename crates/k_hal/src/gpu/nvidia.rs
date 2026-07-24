//! NVIDIA PFIFO + PUSH_BUFFER + bring-up LegacyAcr (Maxwell/Pascal/Volta) / Gsp (Turing+).
//! Kernel Pack = CUBIN offline; ISA via pack — sem hardcode de SKU (GTX 1050 = teste).
//! Degrau ACR: `nvidia_pascal_acr` (antes do probe). Degrau 2–4: `nvidia_pascal`.

use crate::gpu::compute_abi::ComputeBackendKind;
use crate::gpu::detect::{self, GpuInfo};
use crate::gpu::nvidia_pascal::{self, PascalD2};

pub struct NvidiaGpu {
    pub mmio: u64,       // BAR0 virtual
    pub vram_bar2: u64,  // BAR2 virtual (VRAM)
    pub vram_size: u64,
    pub bar0_phys: u64,  // BAR0 physical (para GTT/PTE)
    pub bar2_phys: u64,  // BAR2 physical
    pub pfifo_ready: bool,
    pub channel_active: bool,
    pub backend: ComputeBackendKind,
    pub compute_ready: bool,
    /// Degrau 2 — só Pascal LegacyAcr; None em GSP/falha.
    pub d2: Option<PascalD2>,
}

impl NvidiaGpu {
    const PFIFO_BASE: u64 = 0x2000;
    const PFIFO_PUT: u64 = 0x2000;
    const PFIFO_GET: u64 = 0x2004;
    const METHOD_NOP: u32 = 0x00000000;
    const SUBCH_0: u32 = 0x00000000;

    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        let mmio = gpu.bar0 + pmoff;
        unsafe { k_nano::apic::map_page_uc(gpu.bar0, pmoff); }

        let version = unsafe { core::ptr::read_volatile((mmio + 0x000000) as *const u32) };
        k_nano::slog_hal!("GPU", "nvidia", "{}: version={:#x} bar0={:#x} bar2={:#x}", gpu.name, version, gpu.bar0, gpu.bar2);
        if version == 0xFFFFFFFF || version == 0 { return None; }

        if gpu.vram_size > 0 {
            let vram_aligned = gpu.vram_size.next_power_of_two().min(256 * 1024 * 1024);
            let mapped = unsafe { k_nano::apic::map_region_uc_2mb(gpu.bar2, vram_aligned, pmoff) };
            k_nano::slog_hal!("NVIDIA", "info", "{} VRAM {} MB mapeado ({} x 2MB pages)", gpu.name, gpu.vram_mb(), mapped);
        }

        let vram_ptr = gpu.bar2 + pmoff;
        unsafe { core::ptr::write_volatile(vram_ptr as *mut u32, 0xDEADBEEF); }
        let test = unsafe { core::ptr::read_volatile(vram_ptr as *const u32) };
        let vram_ok = test == 0xDEADBEEF;

        k_nano::slog_hal!("GPU", "nvidia", "VRAM {} MB {}",
            gpu.vram_mb(),
            if vram_ok { "OK" } else { "SEM FIRMWARE (P8 mode)" });

        let pfifo_ready = Self::probe_pfifo(mmio);
        // Família genérica: respeitar backend_kind do detect (DID|PMC); não hardcode SKU.
        let backend = match gpu.backend_kind {
            ComputeBackendKind::LegacyAcr | ComputeBackendKind::Gsp => gpu.backend_kind,
            _ if detect::is_nvidia_legacy_acr(gpu.arch) => ComputeBackendKind::LegacyAcr,
            _ if detect::is_nvidia_gsp_family(gpu.arch) => ComputeBackendKind::Gsp,
            _ => ComputeBackendKind::CpuFallback,
        };
        k_nano::slog_hal!(
            "GPU",
            "nvidia",
            "family={} isa={} backend={:?} (LegacyAcr=Maxwell/Pascal/Volta; Gsp=Turing+ scaffold; SKU-agnostic)",
            detect::nvidia_family_str(gpu.arch),
            gpu.isa_tag.as_str(),
            backend
        );

        let mut d2 = if backend == ComputeBackendKind::LegacyAcr {
            unsafe { nvidia_pascal::bring_up_d2(gpu, mmio) }
        } else if backend == ComputeBackendKind::Gsp {
            k_nano::slog_hal!(
                "GPU",
                "nvidia",
                "step=gsp status=PARTIAL reason=gsp_rm_scaffold family={}",
                detect::nvidia_family_str(gpu.arch)
            );
            None
        } else {
            None
        };
        // Degrau 3 — sistema nervoso (pushbuffer + runlist + kick) sobre as
        // estruturas do D2. Não-fatal; status honesto (sem HW não há golden).
        if let Some(d) = d2.as_mut() {
            unsafe { nvidia_pascal::bring_up_d3(d, mmio); }
        }
        let channel_active = match &d2 {
            Some(d) => d.channel_structures_ok() || pfifo_ready,
            None => pfifo_ready,
        };

        Some(NvidiaGpu {
            mmio, vram_bar2: gpu.bar2 + pmoff, vram_size: gpu.vram_size,
            bar0_phys: gpu.bar0, bar2_phys: gpu.bar2,
            pfifo_ready, channel_active,
            backend,
            compute_ready: false,
            d2,
        })
    }

    pub fn vram_rel(&self, pa: u64) -> Option<u64> {
        if pa < self.bar2_phys {
            return None;
        }
        let off = pa - self.bar2_phys;
        if off >= self.vram_size {
            return None;
        }
        Some(off)
    }

    fn probe_pfifo(mmio: u64) -> bool {
        unsafe {
            let put = core::ptr::read_volatile((mmio + Self::PFIFO_PUT) as *const u32);
            let get = core::ptr::read_volatile((mmio + Self::PFIFO_GET) as *const u32);
            k_nano::slog_hal!("NVIDIA", "info", "PFIFO: PUT={:#x} GET={:#x}", put, get);
            if put == 0xFFFFFFFF && get == 0xFFFFFFFF { return false; }

            core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
            let method = Self::METHOD_NOP | Self::SUBCH_0;
            let arg = 0u32;
            core::ptr::write_volatile((mmio + Self::PFIFO_BASE + 0x0000) as *mut u32, method);
            core::ptr::write_volatile((mmio + Self::PFIFO_BASE + 0x0004) as *mut u32, arg);
            core::ptr::write_volatile((mmio + Self::PFIFO_BASE + 0x0008) as *mut u32, 1);

            let get2 = core::ptr::read_volatile((mmio + Self::PFIFO_GET) as *const u32);
            if get2 == get { return false; }

            k_nano::slog_hal!("NVIDIA", "info", "PFIFO channel 0 OK (NOP submitted, GET moved)");
            true
        }
    }

    pub unsafe fn pio_method(&self, method: u32, arg: u32) {
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0000) as *mut u32, method);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0004) as *mut u32, arg);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0008) as *mut u32, 1);
    }

    pub fn vram_alloc(&self, size: usize) -> Option<u64> {
        crate::gpu::vram::vram_alloc(size)
    }

    pub unsafe fn cpu_to_vram(&self, vram_off: u64, data: &[u8]) {
        let dst = (self.vram_bar2 + vram_off) as *mut u32;
        let words = data.len() / 4;
        for i in 0..words {
            let val = u32::from_le_bytes([data[i*4], data[i*4+1], data[i*4+2], data[i*4+3]]);
            core::ptr::write_volatile(dst.add(i), val);
        }
        for i in (words * 4)..data.len() {
            core::ptr::write_volatile((self.vram_bar2 + vram_off + i as u64) as *mut u8, data[i]);
        }
    }

    pub unsafe fn vram_to_cpu(&self, vram_off: u64, buf: &mut [u8]) {
        let src = (self.vram_bar2 + vram_off) as *const u32;
        let words = buf.len() / 4;
        for i in 0..words {
            let val = core::ptr::read_volatile(src.add(i));
            buf[i*4..i*4+4].copy_from_slice(&val.to_le_bytes());
        }
        for i in (words * 4)..buf.len() {
            buf[i] = core::ptr::read_volatile((self.vram_bar2 + vram_off + i as u64) as *const u8);
        }
    }

    pub unsafe fn pushbuffer_submit(&self, buf_phys: u64, num_methods: u32) -> bool {
        if !self.pfifo_ready { return false; }
        let entries = (num_methods + 1) / 2;
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0000) as *mut u32, buf_phys as u32);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0004) as *mut u32, (buf_phys >> 32) as u32);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_BASE + 0x0008) as *mut u32, entries);
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        let put = core::ptr::read_volatile((self.mmio + Self::PFIFO_PUT) as *const u32);
        core::ptr::write_volatile((self.mmio + Self::PFIFO_PUT) as *mut u32, put + 1);
        for _ in 0..1000000 {
            core::hint::spin_loop();
            let get = core::ptr::read_volatile((self.mmio + Self::PFIFO_GET) as *const u32);
            if get >= put + 1 { return true; }
        }
        k_nano::slog_hal!("GPU", "nvidia", "PUSH_BUFFER timeout: PUT={} GET={}",
            put + 1,
            core::ptr::read_volatile((self.mmio + Self::PFIFO_GET) as *const u32));
        false
    }

    pub unsafe fn dma_matmul_test(&self, vram_src: u64, sys_dst: u64, words: u32) -> bool {
        let mut cmdbuf = [0u32; 16];
        cmdbuf[0] = 0x00000002;
        cmdbuf[1] = vram_src as u32;
        cmdbuf[2] = 0x00000002;
        cmdbuf[3] = (vram_src >> 32) as u32;
        cmdbuf[4] = 0x00000002;
        cmdbuf[5] = sys_dst as u32;
        cmdbuf[6] = 0x00000002;
        cmdbuf[7] = (sys_dst >> 32) as u32;
        cmdbuf[8] = 0x00000000;
        cmdbuf[9] = words;
        let cmdbuf_phys = 0x200000u64;
        let ptr = (cmdbuf_phys + k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)) as *mut u32;
        for i in 0..10 { core::ptr::write_volatile(ptr.add(i), cmdbuf[i]); }
        self.pushbuffer_submit(cmdbuf_phys, 10)
    }

    pub fn status(&self) -> alloc::string::String {
        let (d2s, d3s, d4s) = match &self.d2 {
            Some(d) => (
                alloc::format!("{:?}", d.status),
                alloc::format!("{:?}", d.d3),
                alloc::format!("{:?}", d.d4),
            ),
            None => (
                alloc::string::String::from("n/a"),
                alloc::string::String::from("n/a"),
                alloc::string::String::from("n/a"),
            ),
        };
        alloc::format!(
            "NVIDIA {:?} PFIFO={} VRAM={}MB D2={} D3={} D4={} compute_ready={}",
            self.backend,
            if self.pfifo_ready { "ok" } else { "no" },
            self.vram_size / (1024 * 1024),
            d2s,
            d3s,
            d4s,
            self.compute_ready
        )
    }

    pub fn d2_ready(&self) -> bool {
        self.d2.as_ref().map(|d| d.channel_structures_ok()).unwrap_or(false)
    }

    /// Degrau 4 — QMD + fence + golden sobre o contexto D2/D3 deste handle.
    pub unsafe fn try_vector_add_d4(
        &mut self,
        cubin: &[u8],
        a: &[f32],
        b: &[f32],
        expect: &[f32],
    ) -> bool {
        match self.d2.as_mut() {
            Some(d2) => nvidia_pascal::dispatch_vector_add(d2, self.mmio, cubin, a, b, expect),
            None => {
                k_nano::slog_hal!("NVIDIA", "D4", "sem contexto D2 no probe");
                false
            }
        }
    }
}

/// Fallback LegacyAcr sem `&mut NvidiaGpu` — só log; o caminho real é `try_vector_add_d4`.
pub unsafe fn try_vector_add_legacy(
    gpu: &GpuInfo,
    cubin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    let _ = (a, b, expect);
    if cubin.is_empty() {
        k_nano::slog_hal!("NVIDIA", "C1", "LegacyAcr: CUBIN vazio");
        return false;
    }
    if !detect::is_nvidia_legacy_acr(gpu.arch) {
        k_nano::slog_hal!(
            "NVIDIA",
            "C1",
            "LegacyAcr recusado family={}",
            detect::nvidia_family_str(gpu.arch)
        );
        return false;
    }
    k_nano::slog_hal!("NVIDIA", "C1", "LegacyAcr: use try_vector_add_d4 via backend (pack {}B, QMD={:#x})",
        cubin.len(),
        nvidia_pascal::PASCAL_COMPUTE_B);
    false
}

/// GspBackend (Turing+): zero offsets Pascal; RPC GSP-RM.
pub unsafe fn try_vector_add_gsp(
    gpu: &GpuInfo,
    cubin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    let _ = (a, b, expect);
    if detect::is_nvidia_legacy_acr(gpu.arch) {
        k_nano::slog_hal!(
            "NVIDIA",
            "C1b",
            "GSP recusado family={} (use LegacyAcr)",
            detect::nvidia_family_str(gpu.arch)
        );
        return false;
    }
    if cubin.is_empty() {
        return false;
    }
    k_nano::slog_hal!(
        "NVIDIA",
        "C1b",
        "GspBackend scaffold: family={} pack {}B; GSP-RM incompleto — quarantine",
        detect::nvidia_family_str(gpu.arch),
        cubin.len()
    );
    false
}
