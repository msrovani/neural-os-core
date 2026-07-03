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
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

mod acpi;
mod agents;
mod allocator;
mod apic;
mod ata;
mod cortex;
mod fat;
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
mod apps;
mod skill_gen;
mod voice_skill;
mod browser_agent;
mod verify;
mod hal;
mod bench;
mod gpu;
mod boot_logger;
mod boot_log_agent;
mod shutdown;
mod tpm;
mod disk_agent;

use lazy_static::lazy_static;

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
        }
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
        }
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
        }
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
        }
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
    // A camada de texto VGA (0xB8000) fica overlaid sobre o framebuffer
    // em QEMU -vga std e Intel 6xx, causando xuvisco.
    // _print() usa fb_print() primeiro; fallback VGA soh sem framebuffer.
    let has_fb = crate::display::fb::GPU.lock().is_some();
    if !has_fb {
        vga_buffer::init(pm_offset);
        crate::serial_println!("[BOOT] Sem framebuffer — usando VGA text mode.");
    } else {
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
    let mut cortex_agent = agents::CortexAgent::new();
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
            let parts = crate::fat::read_mbr(ata);
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
    let disk_agent_box = Box::new(disk_agent);

    crate::boot_logger::log("BOOT: DiskAgent ready");

    // Init Compositor FIRST (apps precisam de janelas)
    *crate::display::compositor::COMPOSITOR.lock() = Some(crate::display::compositor::Compositor::new());
    crate::serial_println!("[COMPOSITOR] Inicializado.");
    crate::boot_logger::log("BOOT: Compositor OK");

    // Init Desktop Apps (criam janelas no compositor)
    crate::apps::init_apps();
    crate::boot_logger::log("BOOT: Desktop apps OK");

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
    registry.register(Box::new(agents::NetDriverAgent));
    registry.register(Box::new(agents::UsbDriverAgent));
    registry.register(Box::new(agents::GpuDriverAgent));
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
    registry.register(Box::new(SystemAgent::new()));
    registry.register(Box::new(agents::MonitorAgent::new()));
    registry.register(Box::new(agents::HwBridgeAgent));
    registry.register(Box::new(agents::NetAgent::new()));
    registry.register(Box::new(agents::InputAgent::new()));
    registry.register(Box::new(agents::HermesAgent::new()));
    
    // The Agency: 30+ agentes especialistas
    agents::register_agency_agents(&mut registry);
    
    // HW Agents: um agente por dispositivo PCI
    agents::register_hw_agents(&mut registry);
    
    // DisplayAgent + Apps
    registry.register(Box::new(display::agent::DisplayAgent::new()));
    let mut cron = cron::CronAgent::new();
    cron.init_defaults();
    registry.register(Box::new(cron));
    registry.register(Box::new(mcp::McpAgent::new()));
    registry.register(Box::new(security::SecurityAgent::new()));
    registry.register(Box::new(safety::SafetyAgent::new()));
    registry.register(Box::new(optimizer::OptimizerAgent::new()));
    registry.register(Box::new(agents::mouse_agent::MouseAgent::new()));
    registry.register(Box::new(browser_agent::BrowserAgent::new()));
    registry.register(Box::new(boot_log_agent::BootLogAgent::new()));
    registry.register(Box::new(agents::log_analyst_agent::LogAnalystAgent::new()));
    
    // DiagnosticSkill registrada para SystemAgent executar
    // (substitui os testes inline Box/Vec/Tensor/SiLU)
    let diag_skill = agents::DiagnosticSkill::new();
    SKILL_REGISTRY.lock().register(alloc::boxed::Box::new(diag_skill));
    
    publish_boot_phase(BootPhase::AgentFleet, &alloc::format!("{} agents + DiagnosticSkill registrados", registry.agents.len()));
    serial_println!("[SCHEDULER] {} runtime agents. Iniciando scheduler...", registry.agents.len());
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

fn verify_kernel_from_disk(ata: &crate::ata::AtaDriver, parts: &[crate::fat::Partition]) {
    for part in parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            if let Some(fat32) = unsafe { crate::fat::Fat32Reader::new(ata, part) } {
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
