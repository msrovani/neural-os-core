//! VRAM Tier — buddy allocator para VRAM da GPU.
//! Alocação contígua power-of-2 com splitting/merging de blocos.
//! Base para MSched evicção ótima (Belady) futura.

use alloc::vec::Vec;
use crate::gpu::detect::GpuInfo;
use core::sync::atomic::{AtomicBool, Ordering};

pub(crate) static VRAM_READY: AtomicBool = AtomicBool::new(false);

/// Níveis do buddy: 2^MIN_ORDER = 4KB (página mínima) até 2^MAX_ORDER = 4GB
const MIN_ORDER: u32 = 12;  // 4 KB
const MAX_ORDER: u32 = 32;  // 4 GB

const NUM_ORDERS: usize = (MAX_ORDER - MIN_ORDER + 1) as usize;

pub struct VramBuddy {
    pub base: u64,
    pub size: u64,
    free: [Vec<u64>; NUM_ORDERS],
    total_allocated: u64,
    pub gpu_name: &'static str,
}

fn order(size: u64) -> u32 {
    let mut o = MIN_ORDER;
    while (1u64 << o) < size && o < MAX_ORDER { o += 1; }
    o
}

fn buddy(addr: u64, size: u64) -> u64 {
    addr ^ size
}

impl VramBuddy {
    pub fn new(base: u64, size: u64, gpu_name: &'static str) -> Self {
        let aligned_size = 1u64 << order(size);
        // Inicializa free lists manualmente (sem const { Vec::new() })
        let mut vram = VramBuddy {
            base,
            size: aligned_size,
            free: [
                Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(),
                Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(),
                Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(),
                Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(),
                Vec::new(),
            ],
            total_allocated: 0,
            gpu_name,
        };
        let o = order(aligned_size);
        vram.free[(o - MIN_ORDER) as usize].push(base);
        k_nano::slog_hal!("VRAM", "BUDDY", "{}: base={:#x} size={}MB ordem={}", gpu_name, base, aligned_size/(1024*1024), o);
        vram
    }

    /// Aloca bloco de tamanho mínimo de `size` bytes. Retorna endereço físico.
    pub fn alloc(&mut self, size: u64) -> Option<u64> {
        let req = size.max(4096);
        let o = order(req);
        // Busca do order requisitado até MAX_ORDER
        let mut found_idx: Option<usize> = None;
        for idx in (o - MIN_ORDER) as usize..NUM_ORDERS {
            if !self.free[idx].is_empty() {
                found_idx = Some(idx);
                break;
            }
        }
        let idx = found_idx?;
        let found_order = idx + MIN_ORDER as usize;
        // Remove bloco da free list
        let addr = self.free[idx].pop()?;

        // Split ate o order requisitado
        let current = addr;
        let mut current_order = found_order as u32;
        while current_order > o {
            current_order -= 1;
            let half = 1u64 << current_order;
            let b = current ^ half;
            self.free[(current_order - MIN_ORDER) as usize].push(b);
        }

        self.total_allocated += req;
        Some(current)
    }

    /// Libera bloco, merge com buddy se livre
    pub fn free(&mut self, addr: u64, size: u64) {
        let req = size.max(4096);
        let mut o = order(req);
        let mut block_size = 1u64 << o;
        let mut current = addr;

        self.total_allocated = self.total_allocated.saturating_sub(req);

        // Tenta merge: sobe níveis enquanto buddy está livre
        loop {
            let b = buddy(current, block_size);
            let idx = (o - MIN_ORDER) as usize;
            if let Some(pos) = self.free[idx].iter().position(|&x| x == b) {
                self.free[idx].remove(pos);
                current = current.min(b);
                block_size <<= 1;
                o += 1;
                if o > MAX_ORDER { break; }
            } else {
                break;
            }
        }

        let idx = (o - MIN_ORDER) as usize;
        self.free[idx].push(current);
    }

    pub fn allocated(&self) -> u64 { self.total_allocated }
    pub fn available(&self) -> u64 {
        self.free.iter().enumerate().map(|(i, v)| v.len() as u64 * (1u64 << (i as u32 + MIN_ORDER))).sum()
    }
}

pub static VRAM_BUDDY: spin::Mutex<Option<VramBuddy>> = spin::Mutex::new(None);

/// Inicializa VRAM buddy allocator para a GPU detectada
pub unsafe fn init_vram_tier(gpu: &GpuInfo) -> bool {
    if gpu.bar2 == 0 || gpu.vram_size == 0 {
        k_nano::slog_hal!("VRAM", "info", "{}: sem BAR2 mapeavel (usando DRAM compartilhada)", gpu.name);
        return false;
    }

    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let vram_phys = gpu.bar2;
    let vram_size = gpu.vram_size;

    let pages = k_nano::apic::map_region_uc_2mb(vram_phys, vram_size, pmoff);
    if pages == 0 {
        k_nano::slog_hal!("VRAM", "info", "{}: falha ao mapear VRAM @ {:#x}!", gpu.name, vram_phys);
        return false;
    }
    k_nano::slog_hal!("VRAM", "info", "Mapeados {} x 2MB pages para VRAM @ {:#x}", pages, vram_phys);

    let test_addr = vram_phys + pmoff;
    let test_val: u32 = 0xDEADBEEF;
    core::ptr::write_volatile(test_addr as *mut u32, test_val);
    let read_back = core::ptr::read_volatile(test_addr as *const u32);
    if read_back != test_val {
        k_nano::slog_hal!("VRAM", "info", "{}: teste VRAM falhou em {:#x}", gpu.name, vram_phys);
        return false;
    }
    k_nano::slog_hal!("VRAM", "info", "{}: teste VRAM OK @ {:#x}", gpu.name, vram_phys);

    let buddy = VramBuddy::new(vram_phys, vram_size, gpu.name);
    *VRAM_BUDDY.lock() = Some(buddy);
    VRAM_READY.store(true, Ordering::Release);

    // SASOS (ADR-0087 Fase 4a): mapeia a aperture no espaço do heap
    // (0x4020_0000_0000+) — o ponteiro unificado para Tensor::location =
    // MemTier::Vram. Não falha o init se SASOS não conseguir (VRAM segue
    // acessível por identidade phys+pmoff).
    let _ = crate::gpu::sasos::init_sasos_vram(vram_phys, vram_size, pmoff);

    k_nano::slog_hal!("VRAM", "info", "{} buddy allocator ativo: {} MB", gpu.name, vram_size / (1024*1024));
    true
}

/// Aloca na VRAM via buddy allocator
pub fn vram_alloc(size: usize) -> Option<u64> {
    let addr = VRAM_BUDDY.lock().as_mut()?.alloc(size as u64)?;
    // ADR-0087 Fase 2: registra no MHI para a policy ver acessos (hot → VRAM).
    k_nano::mhi::MHI_REGISTRY.lock().register(
        x86_64::PhysAddr::new(addr), size, k_nano::mhi::AllocTier::Vram, "vram");
    Some(addr)
}

/// Libera bloco VRAM com merge automático de buddies
pub fn vram_free(addr: u64, size: usize) {
    if let Some(ref mut buddy) = *VRAM_BUDDY.lock() {
        buddy.free(addr, size as u64);
    }
    // ADR-0087 Fase 2: remove do registry (evita crescimento infinito).
    k_nano::mhi::unregister(addr);
}

/// #334 MSched — Belady OPT eviction predictor (conectado ao MschedPredictor)
use crate::gpu::msched::MschedPredictor;

static MSCHED: spin::Mutex<Option<MschedPredictor>> = spin::Mutex::new(None);

pub fn msched_init() {
    *MSCHED.lock() = Some(MschedPredictor::new(1024));
    k_nano::kjson!("VRAM", "MSCHED", "init", "window", 1024);
}

pub fn msched_record(addr: u64) {
    if let Some(ref mut m) = *MSCHED.lock() {
        m.record_access(addr);
    }
    // ADR-0087 Fase 2: wiring de acesso real no MHI (latency não medida aqui).
    k_nano::mhi::record_access(addr, 0);
}

pub fn msched_predict(working_set: &[u64]) -> u64 {
    MSCHED.lock().as_ref().and_then(|m| m.predict_evict(working_set)).unwrap_or(0)
}

pub fn msched_status() -> alloc::string::String {
    MSCHED.lock().as_ref().map(|m| m.status()).unwrap_or_default()
}

pub fn vram_status() -> alloc::string::String {
    let guard = VRAM_BUDDY.lock();
    if let Some(ref buddy) = *guard {
        let total_mb = buddy.size / (1024*1024);
        let used_mb = buddy.allocated() / (1024*1024);
        let free_mb = buddy.available() / (1024*1024);
        alloc::format!("VRAM buddy: {} MB usado / {} MB livre / {} MB total ({} fragmentos)",
            used_mb, free_mb, total_mb,
            buddy.free.iter().filter(|v| !v.is_empty()).count())
    } else {
        alloc::string::String::from("VRAM: not initialized")
    }
}

/// `(used_bytes, total_bytes)` do buddy VRAM — None se não init.
pub fn vram_usage() -> Option<(u64, u64)> {
    let guard = VRAM_BUDDY.lock();
    guard.as_ref().map(|b| (b.allocated(), b.size))
}
