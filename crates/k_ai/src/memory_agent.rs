//! MemoryAgent — orçamento adaptativo de memória baseado no modelo AI.
//! Calcula heap, cache, modelo, KV cache baseado em:
//!   - RAM total do sistema (via boot_info.memory_regions)
//!   - VRAM disponivel (via GPU detection)
//!   - Tamanho do modelo (via header .bitnet)
//!
//! Policy: modelo X params -> heap = X/10 MB, cache = heap/2, KV = X/40

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
        k_nano::serial_println!("[MEM] Calculando budget: params={} ram={} vram={}", model_params, total_ram_mb, total_vram_mb);
        let model_gb: f64 = model_params as f64 / 1_000_000_000.0;
        let model_mb = (model_params * 2 / 8 / 1024 / 1024) as usize;

        let heap_mb = ((model_gb * 100.0) as usize).clamp(128, 2048);
        let kv_mb = (model_params as usize / 40 / 1024 / 1024).clamp(8, 4096);
        let arc_mb = (heap_mb / 2).clamp(64, 2048);
        let vram_mb = if total_vram_mb > 0 && model_mb <= total_vram_mb as usize {
            model_mb } else { 0 };

        let used = (heap_mb + model_mb + kv_mb + arc_mb) as u64;
        let free = total_ram_mb.saturating_sub(used);

        k_nano::serial_println!("[MEM]  RAM: {} MB | VRAM: {} MB | Modelo: {} params",
            total_ram_mb, total_vram_mb, model_params);
        k_nano::serial_println!("[MEM]  Heap:{}MB Model:{}MB KV:{}MB ARC:{}MB Vram:{}MB",
            heap_mb, model_mb, kv_mb, arc_mb, vram_mb);
        k_nano::serial_println!("[MEM]  Livre apos: {} MB", free);

        MemoryBudget {
            total_ram_mb, total_vram_mb,
            heap_target_mb: heap_mb, model_ram_mb: model_mb,
            kv_cache_mb: kv_mb, arc_cache_mb: arc_mb,
            vram_model_mb: vram_mb, free_after_mb: free,
            is_gpu: total_vram_mb > 0,
        }
    }

    pub fn budget(&self) -> Option<&MemoryBudget> { self.budget.as_ref() }

    fn measure_cpu_freq() -> u64 {
        let start_lo: u32; let start_hi: u32;
        unsafe {
            core::arch::asm!("rdtsc", out("eax") start_lo, out("edx") start_hi);
        }
        let timer_ticks = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        let target = timer_ticks + 10;
        while k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) < target {
            core::hint::spin_loop();
        }
        let end_lo: u32; let end_hi: u32;
        unsafe {
            core::arch::asm!("rdtsc", out("eax") end_lo, out("edx") end_hi);
        }
        let start = start_lo as u64 | ((start_hi as u64) << 32);
        let end = end_lo as u64 | ((end_hi as u64) << 32);
        (end.wrapping_sub(start) * 12 / 10) / 1_000_000
    }

    fn count_active_agents() -> usize {
        // Heuristic: count Continuous + PollEvery agents that are always active
        // In practice, this is tracked by the scheduler
        15 // Default estimate
    }

    fn calibrate_tick_init(active: usize, _cpu_mhz: u64) -> u64 {
        // Current: 8388608 = 0x800000 (12 ticks/s @ 100 MHz BCLK)
        // Dynamic: fewer agents → smaller init → faster ticks (lower latency)
        //          more agents → larger init → slower ticks (less overhead)
        let current = 0x800000u64;
        match active {
            0..=5   => current / 16,  // 192 ticks/s
            6..=10  => current / 8,   // 96 ticks/s
            11..=20 => current / 4,   // 48 ticks/s
            21..=50 => current / 2,   // 24 ticks/s
            _       => current,       // 12 ticks/s (default)
        }
    }
}

impl Agent for MemoryAgent {
    fn manifest(&self) -> &AgentManifest { &self.manifest }
    fn tick(&mut self, _tick: u64, _tick_count: u64) -> AgentTickResult {
        if self.ran { return AgentTickResult::Done; }
        self.ran = true;

        // RAM total: soma todas as regiões de memória do boot
        let total_ram = {
            let guard = k_nano::memory::GLOBAL_ALLOCATOR.lock();
            guard.as_ref().map(|a| (a.total_frames as u64 * 4096) / (1024*1024)).unwrap_or(0)
        };

        let total_vram = 0u64; // VRAM buddy é Ring 2 (jarbas) — Ring 1 não acopla

        let model_params = cortex::cortex::GLOBAL_MODEL_PARAMS.load(core::sync::atomic::Ordering::Relaxed);

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

        k_nano::serial_println!("[MEM] Orcamento adaptativo de memoria");
        k_nano::serial_println!("[MEM]  RAM: {} MB | VRAM: {} MB | Modelo: {} params",
            budget.total_ram_mb, budget.total_vram_mb, model_params);
        k_nano::serial_println!("[MEM]  Heap:{}MB Model:{}MB KV:{}MB ARC:{}MB Vram:{}MB",
            budget.heap_target_mb, budget.model_ram_mb,
            budget.kv_cache_mb, budget.arc_cache_mb, budget.vram_model_mb);
        k_nano::serial_println!("[MEM]  Livre apos: {} MB", budget.free_after_mb);

        // ── Clock measurement + dynamic tick calibration ──
        let cpu_mhz = Self::measure_cpu_freq();
        let active = Self::count_active_agents();
        let optimal_init = Self::calibrate_tick_init(active, cpu_mhz);
        k_nano::serial_println!("[MEM]  CPU: {} MHz | {} agentes ativos | tick init: {}",
            cpu_mhz, active, optimal_init);

        // Resize heap
        k_nano::allocator::resize_heap_to_mb(budget.heap_target_mb);

        // VRAM buddy é Ring 2 — registro MHI de modelo GPU fica no jarbas/optimizer
        let _ = budget.vram_model_mb;

        if budget.free_after_mb < 256 {
            k_nano::serial_println!("[MEM]  ⚠ RAM insuficiente!");
        }

        self.budget = Some(budget);
        AgentTickResult::Done
    }
}
