#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use bootloader_api::BootInfo;
use event_bus::{CapabilityToken, Event, Receiver};
use skill_registry::{McpManifest, Skill, SkillRegistry, OutputSchema};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

// ═══════════════════════════════════════════════════════════════════
// DEAD MODULES — mantidos como referência para sprints futuros.
// Cada módulo tem `#![allow(dead_code)]` e comentário `// @dead — motivo`.
// =================================================================
// shell          — @dead: substituído por HermesApp (Hermes Agent).
//                  Futuro: CLI/TUI (Sprint ~90+).
// voice_skill    — @dead: sem hardware de áudio.
//                  Futuro: Voice MCP TTS (Sprint ~95+).
// bench          — @dead: benchmark framework nunca integrado.
//                  Futuro: Performance Profiling (Sprint ~85+).
// verify         — @dead: OpCode VM substituído por WASM.
//                  Futuro: Skill Sandbox eBPF-style (Sprint ~90+).
// orchestrator   — @dead: ToT substituído por FanOutPool.
//                  Futuro: Multi-Agent Orchestration (Sprint ~85+).
// tracer         — @dead: span tracing substituído por SelfCritique.
//                  Futuro: Distributed Tracing (Sprint ~90+).
// skill_market   — @dead: substituído por SkillRegistry + IntentCache.
//                  Futuro: Skill Reputation System (Sprint ~90+).
// hal            — @dead: só usado por shell.rs (também dead).
//                  Futuro: portabilidade aarch64/riscv64 (Sprint ~95+).
// ═══════════════════════════════════════════════════════════════════

mod acpi;
mod agents;
mod allocator;
mod apic;
mod ata;
mod cortex;
mod fat32;
mod hw_agents;
mod agency;
mod agency_importer;
mod cron;
mod display;
mod hermes;
mod identity;
mod mcp;
mod interrupts;
mod inventory;
mod memory;
mod mhi;
mod pci;
mod slab;
mod smp;
mod sync;
mod nn;
mod trust;
mod self_heal;
mod serial;
mod skill_loader;
mod xhci;
mod simd;
mod task;
mod tensor;
mod time_utils;
mod usage;
mod chunker;
mod conversation;
mod delta;
mod dma;
mod vga_buffer;
mod net;
mod netstack;
mod network_agent;
mod optimizer;
mod proto;
mod rtl8139;
mod e1000;
mod safety;
mod security;
mod skill_observer;
mod usb_msc;
mod virtio_net;
mod virtio_gpu;
mod mmio;
mod tracer;
mod orchestrator;
mod skill_market;
mod profile;
mod wasm;
mod tv_dsl;
mod gguf;
mod vfs;
mod fs;
mod shell;
mod bpe;
mod apps;
mod skill_gen;
mod voice_skill;
mod browser_agent;
mod verify;
mod hal;
mod bench;
mod gpu;
mod wifi_agent;
mod generic_wifi;
mod wifi_protocol;
mod hw_rng;
mod wifi_msix;
mod link_watcher;
mod boot_logger;
mod boot_log_agent;
mod shutdown;
mod tpm;
mod disk_agent;
mod memory_agent;
mod bitnet_avx2;
mod trinity;
mod jarvis;
mod alloc_adapter;
mod audit;
mod ahci;
mod dhcp;
mod memory_systems;
mod cfs;
mod app_store;
mod multi_user;
mod workflow;
mod hub;
mod elf_loader;
mod wasm_rt;
mod cognitive;
mod audio;

use lazy_static::lazy_static;
use cognitive::{IntentPlanner, SuccessEngine, NeuralCache, FeedbackLoop, WorkflowPredictor, CodebookVQ, ReActLoop, McpServer, AutoSkillGen, DynamicScaler, SelfOptScheduler, ReplayBuffer, BitNetTrainer, EpisodicMemory, TaskSpawner, WorkspaceIsolation, DeltaBranch, MatMulFreeLM};
use trinity::TrinityRouter;

/// Log buffer sector no SDHC (LBA 2048 = 1MB, depois da bootimage de 606KB)
pub const LOG_SECTOR: u32 = 2048;

/// ATA driver global para escrita de log no SDHC
pub static ATA_DRIVER: spin::Mutex<Option<ata::AtaDriver>> = spin::Mutex::new(None);



struct EchoSkill;

impl Skill for EchoSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("echo"),
            description: String::from("Reverses the input payload bytes as a demonstration skill"),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }
    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }
    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let reversed: Vec<u8> = payload.iter().rev().copied().collect();
        Ok(reversed)
    }
}

struct SystemStatusSkill;

impl Skill for SystemStatusSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("system_status"),
            description: String::from("Reports RAM free/total per MHI tier and CPU status"),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }
    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        let mhi_guard = crate::MEMORY_HIERARCHY.lock();
        if mhi_guard.as_ref().is_none() {
            return Err("MHI nao inicializado");
        }
        Ok(())
    }
    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let mhi_guard = MEMORY_HIERARCHY.lock();
        let msg = if let Some(mhi) = mhi_guard.as_ref() {
            if let Some(tier) = mhi.tiers.first() {
                let guard = crate::memory::GLOBAL_ALLOCATOR.lock();
                let occupancy = guard.as_ref().map_or(0.0, |a| a.hardware_context_tensor()[0]);
                drop(guard);
                let free_mb = (tier.capacity_bytes as f64 * (1.0 - occupancy as f64)) / 1_048_576.0;
                let total_mb = tier.capacity_bytes as f64 / 1_048_576.0;
                alloc::format!("[{:?}] {:.1} MB free / {:.1} MB total. CPU: modo cooperativo.",
                    tier.kind, free_mb, total_mb)
            } else {
                String::from("MHI: no tiers available")
            }
        } else {
            String::from("MHI not initialized")
        };
        drop(mhi_guard);
        serial_println!("[SKILL] SystemStatus: {}", msg);
        println!("[SKILL] SystemStatus: {}", msg);
        Ok(msg.into_bytes())
    }
}

struct HardwareInfoSkill;

impl Skill for HardwareInfoSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("hardware_info"),
            description: String::from("Reports hardware inventory and system architecture"),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }
    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        let arch = SYSTEM_ARCH.lock();
        if arch.as_ref().is_none() {
            return Err("SystemArchitecture nao inicializada");
        }
        Ok(())
    }
    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let arch = SYSTEM_ARCH.lock();
        let info = arch.as_ref().map(|a| {
            alloc::format!(
                "Arch: ring0={} ring1={} heap={}MB trust={} power={} tensor={}",
                a.ring0_mode, a.ring1_mode, a.heap_size_mb,
                a.trust_level, a.power_mode, a.tensor_tier,
            )
        }).unwrap_or_else(|| String::from("Arch: unknown"));
        drop(arch);

        let mhi_guard = MEMORY_HIERARCHY.lock();
        let mem_info = mhi_guard.as_ref().map(|m| {
            let tier = &m.tiers[0];
            alloc::format!("RAM: {} MB avail ({:?})", tier.capacity_bytes / 1_048_576, tier.kind)
        }).unwrap_or_else(|| String::from("MHI: unknown"));
        drop(mhi_guard);

        let response = alloc::format!("{}\n{}", info, mem_info);
        serial_println!("[SKILL] HardwareInfo: {}", response);
        println!("[SKILL] HardwareInfo: {}", response);
        Ok(response.into_bytes())
    }
}

struct HwIdentifySkill;

impl Skill for HwIdentifySkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("hw_identify"),
            description: String::from("Identifies all PCI hardware using the Cortex LLM"),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: false,
            contracts: Vec::new(),
        }
    }
    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }
    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let devices = unsafe { crate::pci::scan_pci() };
        let mut report = String::new();
        let mut llm_query = alloc::format!("Identifique este hardware e explique o que posso fazer com cada dispositivo:\n");
        for dev in &devices {
            let class_desc = crate::pci::class_name(dev.class, dev.subclass);
            report.push_str(&alloc::format!(
                "{:02x}:{:02x}.{} {:04x}:{:04x} class={:02x}/{:02x} {}\n",
                dev.bus, dev.device, dev.function,
                dev.vendor_id, dev.device_id,
                dev.class, dev.subclass, class_desc,
            ));
            llm_query.push_str(&alloc::format!(
                "{:04x}:{:04x} class {:02x}/{:02x}\n",
                dev.vendor_id, dev.device_id, dev.class, dev.subclass,
            ));
        }
        serial_println!("[HW-ID] {} dispositivos encontrados. Enviando para LLM...", devices.len());
        let _ = EVENT_BUS.publish(crate::Event {
            id: 0,
            topic: alloc::string::String::from(cortex::TOPIC_LLM_REQUEST),
            payload: llm_query.into_bytes(),
            token: crate::CapabilityToken::Legacy(1),
        });
        Ok(report.into_bytes())
    }
}

lazy_static! {
    static ref EVENT_BUS: event_bus::EventBus = event_bus::EventBus::new();
    // Locks IRQ-safe: SELF_HEAL e RESPAWN_QUEUE são acessados de handlers de exceção
    static ref SKILL_REGISTRY: ticket_lock::TicketLock<SkillRegistry> = {
        let mut reg = SkillRegistry::new();
        reg.register(alloc::boxed::Box::new(EchoSkill));
        reg.register(alloc::boxed::Box::new(SystemStatusSkill));
        reg.register(alloc::boxed::Box::new(HardwareInfoSkill));
        reg.register(alloc::boxed::Box::new(net::NetDiagnosticSkill));
        reg.register(alloc::boxed::Box::new(HwIdentifySkill));
        reg.register(alloc::boxed::Box::new(audio::skills::TtsSkill));
        reg.register(alloc::boxed::Box::new(audio::skills::SttSkill));
        reg.register(alloc::boxed::Box::new(audio::settings::AudioGetSettingsSkill));
        reg.register(alloc::boxed::Box::new(audio::settings::AudioSetVolumeSkill));
        reg.register(alloc::boxed::Box::new(audio::settings::AudioToggleVoiceCloneSkill));
        reg.register(alloc::boxed::Box::new(audio::context::EmotionalContextSkill));
        reg.set_policy("*", skill_registry::ToolPolicy { enabled: true, auto_approve: false });
        ticket_lock::TicketLock::new(reg)
    };
    static ref TRUST_CACHE: ticket_lock::TicketLock<trust::TrustCache> = ticket_lock::TicketLock::new(trust::TrustCache::new());
    static ref SYSTEM_ARCH: spin::Mutex<Option<inventory::SystemArchitecture>> = spin::Mutex::new(None);
    static ref MEMORY_HIERARCHY: spin::Mutex<Option<mhi::MemoryHierarchy>> = spin::Mutex::new(None);
    static ref USAGE_TRACKER: ticket_lock::TicketLock<usage::UsageTracker> = ticket_lock::TicketLock::new(usage::UsageTracker::new());
    static ref EVENT_LOG: ticket_lock::TicketLock<conversation::EventLog> = ticket_lock::TicketLock::new(conversation::EventLog::new());
    static ref CONVERSATION_TRACKER: ticket_lock::TicketLock<hermes::ConversationTracker> = ticket_lock::TicketLock::new(hermes::ConversationTracker::new());
    static ref SELF_HEAL: crate::sync::irq_lock::IrqSafeLock<self_heal::SelfHeal> = crate::sync::irq_lock::IrqSafeLock::new(self_heal::SelfHeal::new());
    static ref RESPAWN_QUEUE: crate::sync::irq_lock::IrqSafeLock<alloc::vec::Vec<alloc::string::String>> = crate::sync::irq_lock::IrqSafeLock::new(alloc::vec::Vec::new());
    static ref SKILL_STORAGE: ticket_lock::TicketLock<skill_loader::SkillLoader> = {
        let loader = skill_loader::load_embedded_skills();
        ticket_lock::TicketLock::new(loader)
    };
    static ref PENDING_SKILL: crate::sync::irq_lock::IrqSafeLock<Option<(alloc::string::String, alloc::string::String)>> = crate::sync::irq_lock::IrqSafeLock::new(None);
    static ref FANOUT_POOL: ticket_lock::TicketLock<skill_registry::FanOutPool> = ticket_lock::TicketLock::new(skill_registry::FanOutPool::new());
    // Sprint 95-96: Cognitive + Memory globals
    static ref TRINITY: ticket_lock::TicketLock<TrinityRouter> = ticket_lock::TicketLock::new(trinity::init_trinity());
    static ref INTENT_PLANNER: ticket_lock::TicketLock<IntentPlanner> = ticket_lock::TicketLock::new(IntentPlanner::new());
    static ref SUCCESS_ENGINE: ticket_lock::TicketLock<SuccessEngine> = ticket_lock::TicketLock::new(SuccessEngine::new());
    static ref NEURAL_CACHE: ticket_lock::TicketLock<NeuralCache> = ticket_lock::TicketLock::new(NeuralCache::new());
    static ref FEEDBACK_LOOP: ticket_lock::TicketLock<FeedbackLoop> = ticket_lock::TicketLock::new(FeedbackLoop::new());
    static ref WORKFLOW_PREDICTOR: ticket_lock::TicketLock<WorkflowPredictor> = ticket_lock::TicketLock::new(WorkflowPredictor::new());
    static ref CODEBOOK_VQ: ticket_lock::TicketLock<CodebookVQ> = ticket_lock::TicketLock::new(CodebookVQ::new(256, 64));
    static ref REACT_LOOP: ticket_lock::TicketLock<ReActLoop> = ticket_lock::TicketLock::new(ReActLoop::new(10));
    static ref MCP_SERVER: ticket_lock::TicketLock<McpServer> = ticket_lock::TicketLock::new(McpServer::new());
    static ref AUTOSKILL_GEN: ticket_lock::TicketLock<AutoSkillGen> = ticket_lock::TicketLock::new(AutoSkillGen::new());
    static ref DYNAMIC_SCALER: ticket_lock::TicketLock<DynamicScaler> = ticket_lock::TicketLock::new(DynamicScaler::new());
    static ref SCHED_OPT: ticket_lock::TicketLock<SelfOptScheduler> = ticket_lock::TicketLock::new(SelfOptScheduler::new());
    static ref REPLAY_BUF: ticket_lock::TicketLock<ReplayBuffer> = ticket_lock::TicketLock::new(ReplayBuffer::new(10000));
    static ref BITNET_TRAINER: ticket_lock::TicketLock<BitNetTrainer> = ticket_lock::TicketLock::new(BitNetTrainer::new());
    static ref EPISODIC_MEM: ticket_lock::TicketLock<EpisodicMemory> = ticket_lock::TicketLock::new(EpisodicMemory::new(1000));
    static ref TASK_SPAWNER: ticket_lock::TicketLock<TaskSpawner> = ticket_lock::TicketLock::new(TaskSpawner::new());
    static ref WORKSPACE_ISO: ticket_lock::TicketLock<WorkspaceIsolation> = ticket_lock::TicketLock::new(WorkspaceIsolation::new());
    static ref DELTA_BRANCH: ticket_lock::TicketLock<DeltaBranch> = ticket_lock::TicketLock::new(DeltaBranch::new());
    static ref MATMUL_FREE_LM: ticket_lock::TicketLock<MatMulFreeLM> = ticket_lock::TicketLock::new(MatMulFreeLM::new());
    static ref TEAM_MEMORY: ticket_lock::TicketLock<crate::memory_systems::TeamMemory> = ticket_lock::TicketLock::new(crate::memory_systems::TeamMemory::new());
    static ref VECTOR_FS: ticket_lock::TicketLock<crate::vfs::VectorFs> = ticket_lock::TicketLock::new(crate::vfs::VectorFs::new(384));
}

// ---------------------------------------------------------------------------
// Agent trait implementations — Sprint 40: Agent-First Refactoring
// ---------------------------------------------------------------------------

/// SystemAgent — substitui system_daemon. Oneshot: ativa, aguarda SYSTEM_READY, conclui.
pub struct SystemAgent {
    receiver: Option<event_bus::Receiver>,
    done: bool,
}

const SYSTEM_MANIFEST: AgentManifest = AgentManifest {
    name: "system",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl SystemAgent {
    pub fn new() -> Self {
        SystemAgent { receiver: None, done: false }
    }
}

impl Agent for SystemAgent {
    fn manifest(&self) -> &AgentManifest { &SYSTEM_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if self.done { return AgentTickResult::Done; }
        if self.receiver.is_none() {
            self.receiver = Some(EVENT_BUS.subscribe("SYSTEM_READY"));
            serial_println!("[AGENT] SystemAgent ativo. Aguardando SYSTEM_READY...");
        }
        if let Some(event) = self.receiver.as_mut().unwrap().try_receive() {
            let reg = SKILL_REGISTRY.lock();
            let out = reg.execute_skill("echo", &event.payload, &event.token);
            drop(reg);
            if let Ok(output) = out {
                serial_println!("[AGENT] EchoSkill: {:?}", output);
            }
            serial_println!("[AGENT] SystemAgent: SYSTEM_READY confirmado. Concluido.");
            println!("[AGENT] SystemAgent: SYSTEM_READY confirmado.");
            self.done = true;
            AgentTickResult::Done
        } else {
            AgentTickResult::Pending
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    // Safe path: VGA + serial sem alocar
    {
        let mut writer = crate::vga_buffer::WRITER.lock();
        if let Some(ref mut w) = *writer { let _ = write!(w, "[PANIC] {}", info); }
    }
    {
        let mut serial = crate::serial::SERIAL.lock();
        if let Some(ref mut s) = *serial { let _ = write!(s, "[PANIC] {}", info); }
    }

    // Registrar causa inesperada e tentar salvar no boot log
    crate::shutdown::set_cause(crate::shutdown::ShutdownCause::Unexpected);
    crate::shutdown::write_persistent_shutdown_log(crate::shutdown::ShutdownCause::Unexpected);

    // Tentative path: SelfHeal + LLM (pode falhar se OOM)
    let alloc_ok = crate::allocator::try_alloc_check();
    if alloc_ok {
        let msg = alloc::format!("{}", info);
        let kind = if msg.contains("PageFault") { "PageFault" }
            else if msg.contains("DoubleFault") { "DoubleFault" } else { "Panic" };
        let class = self_heal::FailureClass::classify(kind, &msg);
        serial_println!("[SELF-HEAL] Class: {:?} — {}", class, class.default_recovery());
        let ctx = self_heal::ErrorContext {
            kind, message: msg.clone(), file: String::from(info.location().map_or("?", |l| l.file())),
            line: info.location().map_or(0, |l| l.line()), ring: 0,
            daemon: String::from("kernel"),
            tick: crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64,
        };
        let mut heal = SELF_HEAL.lock();
        let action = heal.analyze(&ctx, true);
        drop(heal);
        serial_println!("[PANIC] SelfHeal acionado. {:?}", action);
    } else {
        serial_println!("[PANIC] OOM detectado — SelfHeal ignorado (sem memoria).");
        serial_println!("[PANIC] Aumente HEAP_SIZE em allocator.rs ou reduza alocacoes no boot.");
    }

    loop { x86_64::instructions::hlt(); }
}

bootloader_api::entry_point!(kernel_main, config = &CONFIG);

/// Bootloader config: mapeamento de memoria fisica para acesso a VGA/APIC
const CONFIG: bootloader_api::BootloaderConfig = {
    let mut config = bootloader_api::BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config.kernel_stack_size = 512 * 1024;
    config
};

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // Probe serial port (sem lazy_static, funciona antes do heap)
    let serial_exists = unsafe { crate::serial::probe_port(0x3F8) };
    if serial_exists {
        unsafe { core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b'K', options(nostack, preserves_flags)); }
    }
    
    let pm_offset = boot_info.physical_memory_offset.into_option().unwrap_or(0);
    
    // Sonda framebuffer ANTES do VGA text mode para evitar escrever nos registros
    // VGA CRTC (0x3D4/0x3D5) em hardware Intel 6xx com UEFI GOP, o que causa xuvisco.
    display::fb::probe_uefi_framebuffer(boot_info);
    
    // Se framebuffer disponivel, NAO inicializa VGA text mode.
    // Desliga o VGA plane via sequenciador (0x3C4/0x3C5) em vez de
    // limpar 0xB8000 — o bootloader nao mapeia o VGA text buffer, e
    // escrever la causa page fault ANTES da IDT estar pronta.
    // _print() usa fb_print() primeiro; fallback VGA soh sem framebuffer.
    let has_fb = crate::display::fb::GPU.lock().is_some();
    if !has_fb {
        vga_buffer::init(pm_offset);
        crate::serial_println!("[BOOT] Sem framebuffer — usando VGA text mode.");
    } else {
        vga_buffer::disable_vga_plane();
        crate::serial_println!("[BOOT] FB ativo — VGA text mode desligado.");
    }
    
    crate::serial_println!("[BOOT] Kernel started. Serial:{} pm_offset={:#x}", serial_exists, pm_offset);
    
    interrupts::init_idt();
    crate::serial_println!("[BOOT] IDT loaded");
    
    let mut frame_allocator = memory::BitmapFrameAllocator::empty();
    frame_allocator.init(&boot_info.memory_regions);
    crate::serial_println!("[DBG2] Memory regions init: {} regions", boot_info.memory_regions.len());

    {
        let mut mapper = unsafe { memory::init_memory(pm_offset) };
        crate::serial_println!("[DBG3] init_memory OK");
        allocator::init_heap(&mut mapper, &mut frame_allocator)
            .expect("heap initialization failed");
        crate::serial_println!("[DBG4] heap init OK");
        crate::boot_logger::log("BOOT: Heap init OK");
    }

    simd::enable_simd();
    crate::boot_logger::log("BOOT: SIMD enabled");
    #[cfg(target_arch = "x86_64")]
    {
        let avx = crate::tensor::has_avx2();
        serial_println!("[SIMD] AVX2: {}", if avx { "SIM ✅" } else { "NAO ❌" });
    }

    tpm::init_tpm(pm_offset);
    crate::boot_logger::log("BOOT: TPM probe done");

    publish_boot_phase(BootPhase::SystemBringup, "System bringup — SIMD+heap prontos");

    // Diagnosticos como skill (nao inline) — SystemAgent executa depois
    // Box/Vec/Tensor/SiLU/RMSNorm/BitNet MLP agora sao DiagnosticSkill
    memory::init_global_allocator(frame_allocator);
    publish_boot_phase(BootPhase::Diagnostics, "Allocator global pronto");
    
    let slab_metrics = { let s = crate::slab::SLAB_ALLOCATOR.lock(); (s.metrics().0, s.metrics().1) };
    crate::serial_println!("[DBG6] slab metrics: {} {}", slab_metrics.0, slab_metrics.1);
    
    // Inicializa CortexAgent AGORA — o sistema nervoso acorda antes do HW discovery
    // para que o LLM possa participar das decisoes de hardware.
    publish_boot_phase(BootPhase::SystemBringup, "CortexAgent acordando (pre-HW)");
    let cortex_agent = agents::CortexAgent::new();
    // Cortex precisa de pelo menos 1 tick para carregar modelo
    // (o modelo carrega no primeiro tick, nao no construtor)
    
    publish_boot_phase(BootPhase::HardwareDiscovery, "Drivers de HW como agentes");
    
    // Init RTL8139 early — frame allocator minimamente fragmentado = 32KB RX OK
    unsafe { crate::net::init_driver_rtl8139(); }
    publish_boot_phase(BootPhase::HardwareDiscovery, "RTL8139 init");

    // Init e1000 early (fallback se RTL8139 nao encontrado)
    if crate::net::RTL8139.lock().is_none() {
        unsafe { crate::net::init_driver_e1000(); }
        publish_boot_phase(BootPhase::HardwareDiscovery, "E1000 init (fallback)");
    }
    
    let ata_found = {
        let ata_dev = unsafe { ata::AtaDriver::probe() };
        let is_some = ata_dev.is_some();
        *ATA_DRIVER.lock() = ata_dev;
        is_some
    };
    publish_boot_phase(BootPhase::HardwareDiscovery, &alloc::format!("ATA probe={}", if ata_found { "found" } else { "none" }));
    
    unsafe { crate::xhci::init_xhci(); }
    let _usb_msc = unsafe { crate::usb_msc::UsbMassStorage::probe() };
    publish_boot_phase(BootPhase::HardwareDiscovery, "xHCI+USB probe done");

    // Boot log: init after ATA probe
    {
        let ata_guard = crate::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = crate::fat32::read_mbr(ata);
            crate::boot_logger::init(Some(ata), &parts);
            // Verificar assinatura do kernel (l� KERNEL~1 do FAT)
            verify_kernel_from_disk(ata, &parts);
            drop(parts);
        } else {
            crate::boot_logger::init(None, &[]);
        }
        drop(ata_guard);
        crate::boot_logger::log("BOOT: ATA+FAT init OK");
    }

    // Init VFS + mounts
    {
        use crate::vfs::VfsRegistry;
        let vfs = VfsRegistry::new();
        *crate::vfs::VFS.lock() = Some(vfs);
        // Mount points are registered at boot by each agent
        crate::vfs::init_standard_mounts();
    }
    let vfs_guard = crate::vfs::VFS.lock();
    let mcount = vfs_guard.as_ref().map_or(0, |v| v.mount_table().len());
    crate::serial_println!("[VFS] Init OK. {} mounts.", mcount);
    crate::boot_logger::log(&alloc::format!("BOOT: VFS {} mounts", mcount));

    // Init Filesystem Agents
    crate::fs::init_fs_agents();
    crate::boot_logger::log("BOOT: FS agents OK");

    // Init DiskIntelligenceAgent (substitui mount_partitions manual)
    let mut disk_agent = crate::disk_agent::DiskIntelligenceAgent::new();
    if let Some(ref ata) = *crate::ATA_DRIVER.lock() {
        let ctrl = crate::disk_agent::controller::AtaCtrl::new(ata.clone());
        disk_agent.register_controller(Box::new(ctrl));
        crate::boot_logger::log("BOOT: DiskAgent ATA controller registered");
    } else {
        crate::boot_logger::log("BOOT: No ATA device for DiskAgent");
    }
    if let Some(msc) = unsafe { crate::usb_msc::UsbMassStorage::probe() } {
        let ctrl = crate::disk_agent::controller::UsbMscCtrl::new(msc);
        disk_agent.register_controller(Box::new(ctrl));
        crate::boot_logger::log("BOOT: DiskAgent USB-MSC controller registered");
    }
    if let Some(nvme) = unsafe { crate::disk_agent::nvme::NvmeDriver::probe() } {
        let ctrl = crate::disk_agent::controller::NvmeCtrl::new(nvme);
        disk_agent.register_controller(Box::new(ctrl));
        crate::boot_logger::log("BOOT: DiskAgent NVMe controller registered");
    }
    let disk_agent_box = Box::new(disk_agent);

    crate::boot_logger::log("BOOT: DiskAgent ready");

    // Init Desktop Apps
    crate::apps::init_apps();
    crate::boot_logger::log("BOOT: Desktop apps OK");

    // Audio: inicializa configuracoes de som + neural TTS (GPU offload)
    audio::init_audio();
    audio::skills::init_neural_tts();

    // Carrega modelos do FAT32: BGE.BIN
    unsafe {
        let ata_guard = crate::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = crate::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                    if let Some(bge_data) = fs.read_file("BGE.BIN") {
                        if crate::memory_systems::load_bge(&bge_data) {
                            serial_println!("[BGE] Embedding model loaded from FAT!");
                            crate::boot_logger::log("BOOT: BGE embedding loaded");
                        }
                    }
                }
            }
        }
    }

    // GPU: detecta hardware e inicializa backend
    unsafe {
        let gpus = crate::gpu::detect::detect_all();
        if !gpus.is_empty() {
            if let Some(g) = crate::gpu::detect::best_compute_gpu(&gpus) {
                crate::gpu::vram::init_vram_tier(g);
            }
            crate::gpu::backend::init_backend(&gpus);
            serial_println!("[GPU] {} GPU(s) detectadas. Backend: {}",
                gpus.len(), crate::gpu::backend::gpu_status());
            crate::boot_logger::log(&alloc::format!("BOOT: GPU {} backend", gpus.len()));
        } else {
            serial_println!("[GPU] Nenhuma GPU detectada.");
        }
    }

    // Skill Observer: registra observação inicial
    crate::skill_observer::watch_task("boot", &["PCI scan", "GPU init", "Agent registry"], 0);

    let mut registry = agent_core::AgentRegistry::new();
    registry.register(Box::new(agents::PlatformAgent::new()));
    registry.register(Box::new(agents::MemoryAgent::new()));
    registry.register(Box::new(agents::BootSelfHealAgent));
    registry.register(Box::new(agents::BootTrustAgent));
    registry.register(Box::new(crate::memory_agent::MemoryAgent::new()));
    registry.register(Box::new(agents::NetDriverAgent));
    registry.register(Box::new(agents::UsbDriverAgent));
    registry.register(Box::new(audio::hda::HdaAudioAgent::new()));
    registry.register(Box::new(audio::usb::UsbAudioAgent::new()));
    registry.register(Box::new(agents::GpuDriverAgent));
    registry.register(Box::new(agents::FsBridgeAgent::new()));
    registry.register(Box::new(agents::HwDetectAgent));
    registry.register(disk_agent_box);
    
    // HwRegistry: detecta hardware e cria HwAgents
    let mut hw_reg = crate::hw_agents::HwRegistry::new();
    unsafe { hw_reg.detect_all(); }
    serial_println!("[HW-AGENTS] {} dispositivos detectados como HwAgents.", hw_reg.agents.len());
    
    serial_println!("[BOOT] {} boot agents registrados. Executando init_phase...", registry.agents.len());
    registry.init_phase();

    // CortexAgent ja foi criado antes do HW discovery — registrar primeiro
    // para que o LLM esteja disponivel para decisoes de hardware
    registry.register(Box::new(cortex_agent));
    
    // Runtime agents — HermesAgent acorda logo apos o Cortex
    serial_println!("[BOOT] Registering SystemAgent...");
    registry.register(Box::new(SystemAgent::new()));
    serial_println!("[BOOT] Registering MonitorAgent...");
    registry.register(Box::new(agents::MonitorAgent::new()));
    serial_println!("[BOOT] Registering HwBridgeAgent...");
    registry.register(Box::new(agents::HwBridgeAgent));
    serial_println!("[BOOT] Registering NetAgent...");
    let net_agent = Box::new(agents::NetAgent::new());
    serial_println!("[BOOT] NetAgent manifest: name={}, auto_start={}, schedule={:?}",
        net_agent.manifest().name, net_agent.manifest().auto_start, net_agent.manifest().schedule);
    registry.register(net_agent);
    serial_println!("[BOOT] Registering InputAgent...");
    registry.register(Box::new(agents::InputAgent::new()));
    serial_println!("[BOOT] Registering HermesAgent...");
    registry.register(Box::new(agents::HermesAgent::new()));
    
    // The Agency: 30+ agentes especialistas
    serial_println!("[BOOT] Registering agency agents...");
    agents::register_agency_agents(&mut registry);
    
    // HW Agents: um agente por dispositivo PCI
    serial_println!("[BOOT] Registering HW agents...");
    agents::register_hw_agents(&mut registry);
    
    // DisplayAgent + Apps
    // Re-mapeia framebuffer como UC antes do DisplayAgent comecar a desenhar
    display::fb::fb_remap_uc();
    serial_println!("[BOOT] Registering DisplayAgent...");
    registry.register(Box::new(display::agent::DisplayAgent::new()));
    serial_println!("[BOOT] Registering JarvisAgent...");
    registry.register(Box::new(audio::jarvis::JarvisAgent::new()));
    serial_println!("[BOOT] Registering JarvisVoiceAgent...");
    registry.register(Box::new(audio::voice::JarvisVoiceAgent::new()));
    serial_println!("[BOOT] Registering AudioMixerAgent...");
    registry.register(Box::new(audio::mixer::AudioMixerAgent::new()));
    serial_println!("[BOOT] Registering CronAgent...");
    let mut cron = cron::CronAgent::new();
    cron.init_defaults();
    registry.register(Box::new(cron));
    serial_println!("[BOOT] Registering McpAgent...");
    registry.register(Box::new(mcp::McpAgent::new()));
    serial_println!("[BOOT] Registering SecurityAgent...");
    registry.register(Box::new(security::SecurityAgent::new()));
    serial_println!("[BOOT] Registering SafetyAgent...");
    registry.register(Box::new(safety::SafetyAgent::new()));
    serial_println!("[BOOT] Registering OptimizerAgent...");
    registry.register(Box::new(optimizer::OptimizerAgent::new()));
    registry.register(Box::new(agents::mouse_agent::MouseAgent::new()));
    registry.register(Box::new(browser_agent::BrowserAgent::new()));
    registry.register(Box::new(wifi_agent::WifiAgent::new()));
    serial_println!("[BOOT] Registering WifiAgent...");
    registry.register(Box::new(boot_log_agent::BootLogAgent::new()));
    registry.register(Box::new(agents::log_analyst_agent::LogAnalystAgent::new()));
    
    // DiagnosticSkill registrada para SystemAgent executar
    // (substitui os testes inline Box/Vec/Tensor/SiLU)
    let diag_skill = agents::DiagnosticSkill::new();
    SKILL_REGISTRY.lock().register(alloc::boxed::Box::new(diag_skill));
    
    // Ramdisk: carrega modelo .bitnet grande se disponivel
    // Se o ramdisk estiver vazio/pequeno, tenta QEMU loader em 4GB
    let mut model_loaded = false;
    let ramdisk_data_opt = match boot_info.ramdisk_addr {
        bootloader_api::info::Optional::Some(addr) => {
            let len = boot_info.ramdisk_len as usize;
            if len > 1024 {
                Some(unsafe { core::slice::from_raw_parts(addr as *const u8, len) })
            } else {
                serial_println!("[RAMDISK] Ramdisk too small ({} bytes) — trying QEMU loader.", len);
                None
            }
        }
        _ => None,
    };
    if let Some(data) = ramdisk_data_opt {
        let mut m = [0u8; 4]; m.copy_from_slice(&data[..4]);
        let magic = u32::from_le_bytes(m);
        if magic == 0xBE11BE11 {
            serial_println!("[RAMDISK] .bitnet model found ({} bytes). Loading...", data.len());
            if let Some(big_model) = crate::cortex::load_model(data) {
                crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                serial_println!("[RAMDISK] Big model loaded. CortexAgent upgraded.");
                crate::boot_logger::log("BOOT: Ramdisk .bitnet model loaded");
                model_loaded = true;
            } else {
                serial_println!("[RAMDISK] .bitnet load FAILED — keeping micro model.");
            }
        } else {
            serial_println!("[RAMDISK] Unknown magic {:02X?} — skipping model load.", &magic);
        }
    }
    if !model_loaded {
        // Try FAT filesystem (HW real)
        unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                    if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                        if let Some(fat_data) = fs.read_file("BITNET.BIN") {
                            if let Some(big_model) = crate::cortex::load_model(&fat_data) {
                                crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                                serial_println!("[FAT] BitNet model loaded from FAT! CortexAgent upgraded.");
                                crate::boot_logger::log("BOOT: FAT BitNet model loaded");
                                model_loaded = true;
                            }
                        }
                    }
                }
            }
        }
    }
    if !model_loaded {
        // Try QEMU generic loader (dev apenas)
        let load_addr: u64 = 0x100000000;
        let pm_offset = boot_info.physical_memory_offset.into_option().unwrap_or(0);
        // Check if 4GB is within bootloader-mapped memory by scanning memory_regions
        let mem_has_4gb = boot_info.memory_regions.iter().any(|r| r.start <= load_addr && r.end > load_addr);
        if mem_has_4gb {
            let probe_ptr = (load_addr + pm_offset) as *const u8;
            let raw0 = unsafe { core::ptr::read_volatile(probe_ptr) };
            let raw1 = unsafe { core::ptr::read_volatile(probe_ptr.add(1)) };
            let raw2 = unsafe { core::ptr::read_volatile(probe_ptr.add(2)) };
            let raw3 = unsafe { core::ptr::read_volatile(probe_ptr.add(3)) };
            serial_println!("[RAMDISK] Probe 4GB: raw=[0x{:02x},0x{:02x},0x{:02x},0x{:02x}]", raw0, raw1, raw2, raw3);
            let qemu_magic = u32::from_le_bytes([raw0, raw1, raw2, raw3]);
            if qemu_magic == 0xBE11BE11 {
                // load_model() will auto-expand heap based on header
                serial_println!("[RAMDISK] QEMU loader: .bitnet v3 OK, loading model...");
                // Now load model using full memory region slice
                let mut model_len = 0usize;
                for r in boot_info.memory_regions.iter() {
                    if r.start <= load_addr && r.end > load_addr {
                        model_len = (r.end - load_addr) as usize;
                        break;
                    }
                }
                if model_len > 1024 {
                    let model_data = unsafe { core::slice::from_raw_parts(probe_ptr, model_len) };
                    serial_println!("[RAMDISK] QEMU loader: attempting to load {} MB model...", model_len / (1024*1024));
                    if let Some(big_model) = crate::cortex::load_model(model_data) {
                        crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                        serial_println!("[RAMDISK] QEMU loader: Big model loaded. CortexAgent upgraded.");
                        crate::boot_logger::log("BOOT: QEMU loader .bitnet model loaded");
                        model_loaded = true;
                    } else {
                        serial_println!("[RAMDISK] QEMU loader: .bitnet load FAILED — keeping micro model.");
                    }
                } else {
                    serial_println!("[RAMDISK] QEMU loader: model region too small ({model_len} bytes)");
                }
            } else {
                serial_println!("[RAMDISK] No model at phys 0x{load_addr:x} — trying 0x120000000...");
                // Try alternative QEMU loader address (BitNet @ 4.5GB)
                let load_addr2: u64 = 0x120000000;
                let mem_has = boot_info.memory_regions.iter().any(|r| r.start <= load_addr2 && r.end > load_addr2);
                if mem_has {
                    let probe2 = (load_addr2 + pm_offset) as *const u32;
                    let magic2 = unsafe { core::ptr::read_volatile(probe2) };
                    if magic2 == 0xBE11BE11 {
                        let model_len2 = 250usize * 1024 * 1024;
                        let model_data2 = unsafe { core::slice::from_raw_parts(probe2 as *const u8, model_len2) };
                        if let Some(big_model) = crate::cortex::load_model(model_data2) {
                            crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                            serial_println!("[RAMDISK] BitNet model loaded from 0x120000000! CortexAgent upgraded.");
                            model_loaded = true;
                        }
                    }
                }
            }
        } else {
            serial_println!("[RAMDISK] 4GB not in memory map (use -m 6G) — using embedded micro.bitnet.");
        }
        if !model_loaded {
            crate::boot_logger::log("BOOT: No ramdisk, micro.bitnet active");
        }
    }

    // Sprint 95-96: Cognitive + Memory status
    serial_println!("[COG] {}", INTENT_PLANNER.lock().status());
    serial_println!("[COG] {}", SUCCESS_ENGINE.lock().status());
    serial_println!("[COG] {}", FEEDBACK_LOOP.lock().status());
    serial_println!("[COG] {}", NEURAL_CACHE.lock().status());
    serial_println!("[COG] {}", WORKFLOW_PREDICTOR.lock().status());
    serial_println!("[COG] {}", CODEBOOK_VQ.lock().status());
    serial_println!("[COG] {}", REACT_LOOP.lock().status());
    serial_println!("[COG] {}", MCP_SERVER.lock().status());
    serial_println!("[COG] {}", AUTOSKILL_GEN.lock().status());
    serial_println!("[COG] {}", DYNAMIC_SCALER.lock().status());
    serial_println!("[COG] {}", SCHED_OPT.lock().status());
    serial_println!("[COG] {}", REPLAY_BUF.lock().status());
    serial_println!("[COG] {}", BITNET_TRAINER.lock().status());
    serial_println!("[COG] {}", EPISODIC_MEM.lock().status());
    serial_println!("[COG] {}", TASK_SPAWNER.lock().status());
    serial_println!("[COG] {}", WORKSPACE_ISO.lock().status());
    serial_println!("[COG] {}", DELTA_BRANCH.lock().status());
    serial_println!("[COG] {}", MATMUL_FREE_LM.lock().status());
    serial_println!("[COG] {}", TEAM_MEMORY.lock().status());
    serial_println!("[COG] {}", VECTOR_FS.lock().status());
    serial_println!("[COG] {}", crate::memory_systems::bge_status());
    publish_boot_phase(BootPhase::AgentFleet, &alloc::format!("{} agents + DiagnosticSkill registrados", registry.agents.len()));
    serial_println!("[SCHEDULER] {} runtime agents. Iniciando scheduler...", registry.agents.len());
    serial_println!("[SCHEDULER] DEBUG: About to call registry.run()");
    registry.run(
        || { x86_64::instructions::hlt(); },
        || {
            let q = RESPAWN_QUEUE.lock().clone();
            if !q.is_empty() { RESPAWN_QUEUE.lock().clear(); }
            q
        },
        |name| {
            serial_println!("[SCHEDULER] Respawning agent '{}'...", name);
            let agent: Option<Box<dyn Agent>> = match name {
                "monitor" => Some(Box::new(agents::MonitorAgent::new())),
                "hw_bridge" => Some(Box::new(agents::HwBridgeAgent)),
                "network_agent" => Some(Box::new(agents::NetAgent::new())),
                "input" => Some(Box::new(agents::InputAgent::new())),
                "cortex_llm" => Some(Box::new(agents::CortexAgent::new())),
                "intent_router" => Some(Box::new(agents::HermesAgent::new())),
                "hermes_console" => Some(Box::new(display::agent::DisplayAgent::new())),
                "display" => Some(Box::new(display::agent::DisplayAgent::new())),
                "cron" => Some(Box::new(cron::CronAgent::new())),
                "mcp" => Some(Box::new(mcp::McpAgent::new())),
                "security" => Some(Box::new(security::SecurityAgent::new())),
                "safety" => Some(Box::new(safety::SafetyAgent::new())),
                "optimizer" => Some(Box::new(optimizer::OptimizerAgent::new())),
                "mouse" => Some(Box::new(agents::mouse_agent::MouseAgent::new())),
                _ => None,
            };
            agent
        },
    );
}

// ── Boot Phase Events ─────────────────────────────────────────
// Publicados no EventBus para que HermesAgent, CortexAgent e BootLogAgent
// possam acompanhar o progresso do boot e tomar decisoes.

pub const TOPIC_BOOT_PHASE: &str = "BOOT_PHASE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BootPhase {
    SafeHarbor,          // Display + Serial + IDT (minimo para sobreviver)
    MemoryCore,          // Frame allocator + Page tables + Heap
    SystemBringup,       // SIMD + SystemAgent acorda
    Diagnostics,         // Testes de sanidade (como skill)
    HardwareDiscovery,   // PCI + ACPI + SMP
    DriverInit,          // RTL8139, ATA, xHCI, GPU
    AgentFleet,          // Todos os agentes registrados
    Runtime,             // AgentScheduler::run()
    Panic,               // Boot falhou
}

pub fn publish_boot_phase(phase: BootPhase, msg: &str) {
    let payload = alloc::format!("[BOOT:{:?}] {}", phase, msg);
    serial_println!("{}", payload);
    crate::boot_logger::log(&payload);
    let _ = EVENT_BUS.publish(crate::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_BOOT_PHASE),
        payload: payload.into_bytes(),
        token: crate::CapabilityToken::Legacy(1),
    });
}

pub(crate) fn scancode_to_ascii(scancode: u8) -> Option<char> {
    match scancode {
        0x1E => Some('A'), 0x30 => Some('B'), 0x2E => Some('C'),
        0x20 => Some('D'), 0x12 => Some('E'), 0x21 => Some('F'),
        0x22 => Some('G'), 0x23 => Some('H'), 0x17 => Some('I'),
        0x24 => Some('J'), 0x25 => Some('K'), 0x26 => Some('L'),
        0x32 => Some('M'), 0x31 => Some('N'), 0x18 => Some('O'),
        0x19 => Some('P'), 0x10 => Some('Q'), 0x13 => Some('R'),
        0x1F => Some('S'), 0x14 => Some('T'), 0x16 => Some('U'),
        0x2F => Some('V'), 0x11 => Some('W'), 0x2D => Some('X'),
        0x15 => Some('Y'), 0x2C => Some('Z'),
        0x39 => Some(' '),
        0x02 => Some('1'), 0x03 => Some('2'), 0x04 => Some('3'),
        0x05 => Some('4'), 0x06 => Some('5'), 0x07 => Some('6'),
        0x08 => Some('7'), 0x09 => Some('8'), 0x0A => Some('9'),
        0x0B => Some('0'),
        0x0C => Some('-'), 0x0D => Some('='),
        0x1A => Some('['), 0x1B => Some(']'),
        0x27 => Some(';'), 0x28 => Some('\''),
        0x29 => Some('`'), 0x2B => Some('\\'),
        0x33 => Some(','), 0x34 => Some('.'), 0x35 => Some('/'),
        _ => None,
    }
}

fn verify_kernel_from_disk(ata: &crate::ata::AtaDriver, parts: &[crate::fat32::Partition]) {
    for part in parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            if let Some(fat32) = unsafe { crate::fat32::Fat32Reader::new(ata, part) } {
                if let Some(data) = unsafe { fat32.read_file("KERNEL~1") } {
                    if !crate::identity::verify_kernel_signature(&data) {
                        crate::serial_println!("[SEC] *** ASSINATURA DO KERNEL INVALIDA! ***");
                        crate::serial_println!("[SEC] HALT por seguranca.");
                        loop { core::hint::spin_loop() }
                    } else {
                        crate::serial_println!("[SEC] Assinatura do kernel OK.");
                        crate::tpm::tpm_extend_pcr(crate::tpm::TPM_PCR_KERNEL, &data);
                    }
                    return;
                }
            }
        }
    }
    crate::serial_println!("[SEC] Kernel nao assinado (sem FAT ou KERNEL~1 nao encontrado).");
}

// All old async fn daemons removed — migrated to native agents in agents.rs





