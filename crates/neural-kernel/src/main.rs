#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use bootloader::bootinfo::BootInfo;
use core::sync::atomic::Ordering;
use core::task::{Context, Poll};
use event_bus::{CapabilityToken, Event, Receiver};
use skill_registry::{McpManifest, Skill, SkillRegistry};
use memory::BitmapFrameAllocator;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

mod acpi;
mod agents;
mod allocator;
mod apic;
mod ata;
mod cortex;
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
mod safety;
mod security;
mod virtio_net;
mod virtio_gpu;

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
        let _ = write!(serial, "[PANIC] {}", info);
    }

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

bootloader::entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    vga_buffer::init(boot_info.physical_memory_offset);
    interrupts::init_idt();

    let mut frame_allocator = BitmapFrameAllocator::empty();
    frame_allocator.init(&boot_info.memory_map);

    {
        let mut mapper = unsafe { memory::init_memory(boot_info.physical_memory_offset) };
        allocator::init_heap(&mut mapper, &mut frame_allocator)
            .expect("heap initialization failed");
    }

    simd::enable_simd();

    println!("[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.");
    serial_println!("[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.");

    println!("[TEST] Forcando Breakpoint (int3)...");
    serial_println!("[TEST] Forcando Breakpoint (int3)...");
    x86_64::instructions::interrupts::int3();

    let boxed_val = Box::new(41);
    serial_println!("[TEST] Box::new(41) = {}", *boxed_val);
    println!("[TEST] Box::new(41) = {}", *boxed_val);

    let mut vec = Vec::new();
    vec.push(10);
    vec.push(20);
    vec.push(30);
    serial_println!("[TEST] Vec = {:?}", vec);
    println!("[TEST] Vec = {:?}", vec);

    let a_data = vec![1.0_f32, 2.0_f32, 3.0_f32];
    let a = tensor::Tensor::from_row_major((1, 3), a_data).unwrap();
    let b_data = vec![4.0_f32, 5.0_f32, 6.0_f32];
    let b = tensor::Tensor::from_row_major((3, 1), b_data).unwrap();
    if let Some(c) = a.matmul(&b) {
        serial_println!("[TEST] Tensor Matmul Result: shape ({}, {}), data: {:?}", c.shape.0, c.shape.1, c.data);
        println!("[TEST] Tensor Matmul Result: shape ({}, {}), data: {:?}", c.shape.0, c.shape.1, c.data);
    }

    let mut tensor = tensor::Tensor::from_row_major((1, 3), vec![-1.0, 0.0, 1.0]).unwrap();
    tensor.apply(nn::silu);
    serial_println!("[TEST] SiLU([-1, 0, 1]) = {:?}", tensor.data);
    println!("[TEST] SiLU([-1, 0, 1]) = {:?}", tensor.data);

    nn::rms_norm(&mut tensor, 1.0, 1e-6);
    serial_println!("[TEST] RMSNorm(SiLU(...), weight=1.0) = {:?}", tensor.data);
    println!("[TEST] RMSNorm(SiLU(...), weight=1.0) = {:?}", tensor.data);

    let emb = tensor::Tensor::from_row_major((1, 3), vec![1.0, -0.5, 0.3]).unwrap();
    let w_data = vec![1.0, 0.0, 1.0, -1.0, 0.0, -1.0];
    let w = tensor::Tensor::from_row_major((2, 3), w_data).unwrap();
    let linear = nn::Linear::new(w, None);
    let mut logits = linear.forward(&emb);
    logits.apply(nn::silu);
    let decision = nn::argmax(&logits);
    serial_println!("[ROUTER] Intencao processada. Acao escolhida: {} (0=Daemon, 1=Halt)", decision);
    println!("[ROUTER] Intencao processada. Acao escolhida: {} (0=Daemon, 1=Halt)", decision);

    let bit_input = tensor::Tensor::from_row_major((1, 3), vec![1.5, -0.5, 2.0]).unwrap();
    let weights_f32 = tensor::Tensor::from_row_major(
        (3, 2), vec![1.5_f32, -1.8, 0.2, 2.1, -3.0, 0.0],
    ).unwrap();
    let packed_weights = tensor::quantize_to_packed(&weights_f32, 0.5);
    let compressed_size = packed_weights.packed_data.len();
    let bit_linear = nn::BitLinear::new(packed_weights, None);
    let bit_output = bit_linear.forward(&bit_input);
    serial_println!("[BITNET] Inferencia 2-bit concluida. Tamanho comprimido: {} bytes. Output: {:?}",
        compressed_size, bit_output.data);
    println!("[BITNET] Inferencia 2-bit concluida. Tamanho comprimido: {} bytes. Output: {:?}",
        compressed_size, bit_output.data);

    // --- Stress test: 1000 alloc/desaloc no Bitmap Frame Allocator ---
    serial_println!("[KERNEL] Bitmap Allocator: iniciando stress test (1000 iteracoes)...");
    println!("[KERNEL] Bitmap Allocator: iniciando stress test (1000 iteracoes)...");

    {
        let mut allocated_frames: Vec<Option<x86_64::structures::paging::PhysFrame>> = Vec::new();

        for i in 0..1000 {
            let frame = frame_allocator.allocate_frame();
            match frame {
                Some(f) => {
                    allocated_frames.push(Some(f));
                }
                None => {
                    serial_println!("[KERNEL] Stress: falhou alocar na iteracao {}", i);
                    println!("[KERNEL] Stress: falhou alocar na iteracao {}", i);
                    break;
                }
            }

            if i % 2 == 0 && !allocated_frames.is_empty() {
                if let Some(Some(f)) = allocated_frames.last() {
                    unsafe {
                        frame_allocator.deallocate_frame(*f);
                    }
                }
                allocated_frames.pop();
            }
        }
    }

    let ram_tensor = frame_allocator.hardware_context_tensor();
    serial_println!("[KERNEL] Bitmap Allocator operante. 1000 iteracoes estaveis. Status RAM Tensor: [{:.6}, {:.6}]",
        ram_tensor[0], ram_tensor[1]);
    println!("[KERNEL] Bitmap Allocator operante. 1000 iteracoes estaveis. Status RAM Tensor: [{:.6}, {:.6}]",
        ram_tensor[0], ram_tensor[1]);

    memory::init_global_allocator(frame_allocator);

    let slab_metrics = { let s = crate::slab::SLAB_ALLOCATOR.lock(); (s.metrics().0, s.metrics().1) };
    serial_println!("[SLAB] Alocador slab operacional. Allocs: {}, Deallocs: {}",
        slab_metrics.0, slab_metrics.1);

    // Boot phase agents — cada um é um Agent Oneshot que executa init
    // Tenta detectar framebuffer UEFI (hardware real)
    display::fb::probe_uefi_framebuffer(boot_info.physical_memory_offset);

    // Probe ATA (SDHC interno) para escrita de log
    let ata = unsafe { ata::AtaDriver::probe() };
    if ata.is_some() {
        serial_println!("[ATA] Controladora ATA/SATA detectada para escrita de log.");
        serial_println!("[ATA] Boot log sera escrito no setor LBA {} do SDHC.", LOG_SECTOR);
    } else {
        serial_println!("[ATA] Nenhuma controladora ATA via PCI. Log via serial apenas.");
    }
    *ATA_DRIVER.lock() = ata;

    let mut registry = agent_core::AgentRegistry::new();
    registry.register(Box::new(agents::PlatformAgent::new()));
    registry.register(Box::new(agents::MemoryAgent::new()));
    registry.register(Box::new(agents::BootSelfHealAgent));
    registry.register(Box::new(agents::BootTrustAgent));
    registry.register(Box::new(agents::NetDriverAgent));
    registry.register(Box::new(agents::UsbDriverAgent));
    registry.register(Box::new(agents::GpuDriverAgent));
    registry.register(Box::new(agents::HwDetectAgent));
    serial_println!("[BOOT] {} boot agents registrados. Executando init_phase...", registry.agents.len());
    registry.init_phase();

    // Runtime agents — polling loop contínuo
    registry.register(Box::new(SystemAgent::new()));
    registry.register(Box::new(agents::MonitorAgent::new()));
    registry.register(Box::new(agents::HwBridgeAgent));
    registry.register(Box::new(agents::NetAgent::new()));
    registry.register(Box::new(agents::InputAgent::new()));
    registry.register(Box::new(agents::CortexAgent::new()));
    registry.register(Box::new(agents::HermesAgent::new()));
    // DisplayAgent — gerencia framebuffer (se disponível) ou VGA
    serial_println!("[DISPLAY] Inicializando DisplayAgent.");
    // Nota: framebuffer via bootloader depende da versão. 
    // bootloader 0.9+ expõe BootInfo::frame_buffer (Option<FrameBuffer>).
    // Será integrado quando disponível. Por enquanto, VGA text mode funciona.
    // DisplayAgent + CronAgent + McpAgent
    registry.register(Box::new(display::agent::DisplayAgent::new()));
    let mut cron = cron::CronAgent::new();
    cron.init_defaults();
    registry.register(Box::new(cron));
    registry.register(Box::new(mcp::McpAgent::new()));
    registry.register(Box::new(security::SecurityAgent::new()));
    registry.register(Box::new(safety::SafetyAgent::new()));
    registry.register(Box::new(optimizer::OptimizerAgent::new()));
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
                _ => None,
            };
            agent
        },
    );
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

// All old async fn daemons removed — migrated to native agents in agents.rs
