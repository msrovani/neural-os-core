//! MemoryAgent — orçamento adaptativo de memória baseado no modelo AI.
//! Calcula heap, cache, modelo, KV cache baseado em:
//!   - RAM total do sistema (via boot_info.memory_regions)
//!   - VRAM disponivel (via GPU detection)
//!   - Tamanho do modelo (via header .bitnet)
//!
//! Policy: modelo X params -> heap = X/10 MB, cache = heap/2, KV = X/40

use alloc::string::String;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

pub struct MemoryBudget {
    pub total_ram_mb: u64,
    pub total_vram_mb: u64,
    pub heap_target_mb: usize,
    pub model_ram_mb: usize,
    pub kv_cache_mb: usize,
    pub arc_cache_mb: usize,
    pub vram_model_mb: usize,
    pub free_after_mb: u64,
    pub is_gpu: bool,
}

pub struct MemoryAgent {
    manifest: AgentManifest,
    budget: Option<MemoryBudget>,
    ran: bool,
}

impl MemoryAgent {
    pub fn new() -> Self {
        MemoryAgent {
            manifest: AgentManifest {
                name: "MemoryAgent", kind: AgentKind::System,
                schedule: ScheduleKind::Oneshot, auto_start: true, persist: false,
            },
            budget: None, ran: false,
        }
    }

    pub fn calculate_budget(model_params: u64, total_ram_mb: u64, total_vram_mb: u64) -> MemoryBudget {
        let model_gb: f64 = model_params as f64 / 1_000_000_000.0;
        let model_mb = (model_params * 2 / 8 / 1024 / 1024) as usize;

        let heap_mb = ((model_gb * 100.0) as usize).clamp(128, 2048);
        let kv_mb = (model_params as usize / 40 / 1024 / 1024).clamp(8, 4096);
        let arc_mb = (heap_mb / 2).clamp(64, 2048);
        let vram_mb = if total_vram_mb > 0 && model_mb <= total_vram_mb as usize {
            model_mb } else { 0 };

        let used = (heap_mb + model_mb + kv_mb + arc_mb) as u64;
        let free = total_ram_mb.saturating_sub(used);

        MemoryBudget {
            total_ram_mb, total_vram_mb,
            heap_target_mb: heap_mb, model_ram_mb: model_mb,
            kv_cache_mb: kv_mb, arc_cache_mb: arc_mb,
            vram_model_mb: vram_mb, free_after_mb: free,
            is_gpu: total_vram_mb > 0,
        }
    }

    pub fn budget(&self) -> Option<&MemoryBudget> { self.budget.as_ref() }
}

impl Agent for MemoryAgent {
    fn manifest(&self) -> &AgentManifest { &self.manifest }
    fn tick(&mut self, _tick: u64, _tick_count: u64) -> AgentTickResult {
        if self.ran { return AgentTickResult::Done; }
        self.ran = true;

        // RAM total: soma todas as regiões de memória do boot
        let total_ram = {
            let guard = crate::memory::GLOBAL_ALLOCATOR.lock();
            guard.as_ref().map(|a| (a.total_frames as u64 * 4096) / (1024*1024)).unwrap_or(0)
        };

        // VRAM: via GPU detection
        let total_vram = {
            let vram = crate::gpu::vram::VRAM_STATE.lock();
            vram.as_ref().map(|v| v.size / (1024*1024)).unwrap_or(0)
        };

        let model_params = crate::cortex::GLOBAL_MODEL_PARAMS.load(core::sync::atomic::Ordering::Relaxed);

        let budget = if model_params > 0 {
            Self::calculate_budget(model_params, total_ram, total_vram)
        } else {
            MemoryBudget {
                total_ram_mb: total_ram, total_vram_mb: total_vram,
                heap_target_mb: 128, model_ram_mb: 0, kv_cache_mb: 0,
                arc_cache_mb: 64, vram_model_mb: 0,
                free_after_mb: total_ram - 192, is_gpu: total_vram > 0,
            }
        };

        crate::serial_println!("[MEM] Orcamento adaptativo de memoria");
        crate::serial_println!("[MEM]  RAM: {} MB | VRAM: {} MB | Modelo: {} params",
            budget.total_ram_mb, budget.total_vram_mb, model_params);
        crate::serial_println!("[MEM]  Heap:{}MB Model:{}MB KV:{}MB ARC:{}MB Vram:{}MB",
            budget.heap_target_mb, budget.model_ram_mb,
            budget.kv_cache_mb, budget.arc_cache_mb, budget.vram_model_mb);
        crate::serial_println!("[MEM]  Livre apos: {} MB", budget.free_after_mb);

        // Resize heap
        crate::allocator::resize_heap_to_mb(budget.heap_target_mb);

        // Register VRAM in MHI
        if budget.vram_model_mb > 0 {
            if let Some(ref vram) = *crate::gpu::vram::VRAM_STATE.lock() {
                crate::mhi::MHI_REGISTRY.lock().register(
                    x86_64::PhysAddr::new(vram.base),
                    budget.vram_model_mb * 1024 * 1024,
                    crate::mhi::AllocTier::Vram, "model_weights");
            }
        }

        if budget.free_after_mb < 256 {
            crate::serial_println!("[MEM]  ⚠ RAM insuficiente!");
        }

        self.budget = Some(budget);
        AgentTickResult::Done
    }
}
