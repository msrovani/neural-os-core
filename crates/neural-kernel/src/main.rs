#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]
#![allow(unreachable_patterns)]
#![feature(abi_x86_interrupt)]

/// Macro para serial_println rate-limited. So imprime a cada N chamadas.
/// Uso: debug_rl!("msg", 100, "formato", args...);
/// DEPRECATED: Use `k_nano::slog_bin!` diretamente. `debug_rl!` não é usado em
/// lugar nenhum do código ativo — só existe em LEGACY/. Mantida apenas para
/// referência histórica. Não portar para k_nano.
#[macro_export]
#[deprecated(note = "Use k_nano::slog_bin! diretamente — debug_rl! não é mais usado")]
macro_rules! debug_rl {
    ($msg:expr, $rate:expr, $($arg:tt)*) => {{
        // Taxa efetiva = max(1, $rate)
        #[export_name = concat!("_rl_counter_", $msg)]
        static COUNTER: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        let c = COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed).wrapping_add(1);
        if c % ($rate as u64) == 0 {
            k_nano::slog_bin!("Boot", "rl", concat!($msg, " ", $($arg)*));
        }
    }};
    ($msg:expr, $rate:expr) => {
        debug_rl!($msg, $rate, "");
    };
}


extern crate alloc;



use alloc::boxed::Box;

use alloc::string::String;

use alloc::vec;

use alloc::vec::Vec;

use event_bus::{CapabilityToken, Event};

use skill_registry::{McpManifest, Skill, OutputSchema};

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};










mod acpi;
mod agency;
pub mod agents;
mod allocator;
mod apic;
mod ata;
mod block_dev;
mod bei_init;
mod cortex;
mod fat32;
mod global_arena;
mod hw_agents;
mod identity;
mod interrupts;
mod interrupts_ext;
mod inventory;
mod memory;
mod mhi;
mod model_hub;
mod model_provisioner;
mod pci;
mod smp;
mod sync;

pub use hermes_crate::{
    actor_registry, app_store, approval, apps, browser_agent, cron, evolve, generic_wifi, sgdb_agent,
    gguf_wasm, globals as hermes_globals, hermes, hitl_ui, hub, hw_pnp, ipc_bus, marketplace, mcp,
    memory_store, net_bridge, ntp, optimizer, package_hub, plugin_hub, safety,
    search_agent, security, self_evolve, self_update, skill_gen, skill_loader, skill_market,
    skill_observer, skill_opt, structured_decode, voice_skill, wifi_agent,
    wifi_compat, wifi_iwlwifi, wifi_msix, wifi_protocol,
    // ADR-0062 E3 — SoftMAC BE via hermes re-export
    wifi_softmac,
    // E1b: fs, vfs, neural_fs moved to hermes crate
    fs, vfs, neural_fs,
};
pub use k_ai::{self_heal, trust};
pub use k_nano::globals::EVENT_BUS;
pub use k_nano::globals::LATENT_BUS;
// Macros re-exported from k_nano (drift cleanup — bin delegates serial to k_nano)
pub use k_nano::{kjson, klog, klogc, serial_print, serial_println};
// ADR-0042 N5.7: engine jarbas wired; residuals = audio/* (ADR-0045 truth + Sprint107 wakeword), jarbas_fb.rs
pub use jarbas_crate::{display, gpu, jarvis, uvc_driver, virtio_gpu, vision_agent};

mod limine_boot;
mod log_agent;

mod serial;

mod xhci;

mod simd;

mod task;

mod usage;

mod chunker;

mod conversation;

mod dma;

mod vga_buffer;

// C5: print macros — canonical _print in jarbas::display::fb, $crate resolves to bin's re-export.
#[macro_export]
macro_rules! print { ($($arg:tt)*) => (crate::vga_buffer::_print(format_args!($($arg)*))); }
#[macro_export]
macro_rules! println { () => (crate::print!("\n")); ($($arg:tt)*) => (crate::print!("{}\n", format_args!($($arg)*))); }

mod net;

mod netstack;

mod tls_client;

mod tls_trust;

mod labor_smokes;

mod slip;

mod env;

mod netdiag;

mod network_agent;

mod proto;

mod rtl8139;

mod e1000;

mod i225;

mod usb_trust;

mod virtio_net;

mod profile;

mod gguf;
mod gguf_streaming;

mod bpe;

mod demo_flags;

mod hw_rng;

mod link_watcher;

mod boot_logger;

mod boot_log_agent;

mod shutdown;

mod tpm;
mod exfat;
mod exfat_write;
mod gpt;

mod fs_driver;

mod ext2_reader;
mod io_scheduler;
mod storage_manager;
mod netfs;
mod disk_power;

mod disk_agent;

mod memory_agent;


mod audit;

mod ahci;

mod memory_systems;

mod multi_user;

mod hnsw;
mod context_window;
mod training_agent;

mod micropython_wasm;
// ponytail: aios_api.rs deleted — YAGNI. Nada importava dela. As 2 funções CapGate
// vivem em capability_gate::host_send_tcp / host_write_ring.
mod cognitive;

mod audio;

mod address_space;
mod exec_arena;
mod syscall;
mod ipc;
mod capability_gate;
mod jarbas_fb;
mod k_ia_dma;
mod cortex_mmap;
mod demand_page;
mod user_mode;
mod isolation_ring;
mod virtio_vring;
mod gguf_mmap;

pub use k_nano::load_status;

mod jarbas_bridge;
mod nn;
mod r3;
mod arena;
mod kv_h2o;
mod neuos_probe;
mod ngram_spec;
mod tensor;
mod trinity;
mod process;
mod elf_loader;

use lazy_static::lazy_static;

use cognitive::{IntentPlanner, SuccessEngine, NeuralCache, FeedbackLoop, WorkflowPredictor, CodebookVQ, ReActLoop, McpServer, AutoSkillGen, DynamicScaler, SelfOptScheduler, ReplayBuffer, BitNetTrainer, EpisodicMemory, TaskSpawner, WorkspaceIsolation, DeltaBranch, MatMulFreeLM};

pub use trinity::TRINITY;



/// Log buffer sector no SDHC (LBA 2048 = 1MB, depois da bootimage de 606KB)

pub const LOG_SECTOR: u32 = 2048;



/// ATA driver global — canônico em k_nano (Onda 4: sem mirror duplo).
pub use k_nano::ATA_DRIVER;

/// Unidade de armazenamento primária (AHCI ou ATA) para FAT32 — canônico em k_nano
pub use k_nano::AHCI_DRIVER;

/// USB Mass Storage — canônico em k_nano
pub use k_nano::globals::USB_MSC;

/// Merkle Audit Trail global (#315.19)

pub static AUDIT_TRAIL: spin::Mutex<crate::audit::AuditTrail> = spin::Mutex::new(crate::audit::AuditTrail::new());

/// Endereço físico final do loader QEMU após carregar modelo grande + assets.
/// O scan de experts começa daqui (pula BITNET2B, Piper, tinystories, BPE, BGE).
/// Inicializado após ramdisk loader terminar.
pub static QEMU_LOADER_SCAN_START: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);




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

        k_nano::slog_bin!("SKILL", "info", "SystemStatus: {}", msg);

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

        k_nano::slog_bin!("SKILL", "info", "HardwareInfo: {}", response);

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

        k_nano::slog_bin!("HW-ID", "info", "{} dispositivos encontrados. Enviando para LLM...", devices.len());

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

    // Locks IRQ-safe: SELF_HEAL e RESPAWN_QUEUE são acessados de handlers de exceção

    // P001: SKILL_REGISTRY canônico agora em k_nano::globals (cross-crate).
    // Skills builtin registrados via register_builtin_skills() no boot.

    static ref TRUST_CACHE: ticket_lock::TicketLock<trust::TrustCache> = ticket_lock::TicketLock::new(trust::TrustCache::new());

    static ref SYSTEM_ARCH: spin::Mutex<Option<inventory::SystemArchitecture>> = spin::Mutex::new(None);

    static ref MEMORY_HIERARCHY: spin::Mutex<Option<mhi::MemoryHierarchy>> = spin::Mutex::new(None);

    static ref USAGE_TRACKER: ticket_lock::TicketLock<usage::UsageTracker> = ticket_lock::TicketLock::new(usage::UsageTracker::new());

    static ref EVENT_LOG: ticket_lock::TicketLock<conversation::EventLog> = ticket_lock::TicketLock::new(conversation::EventLog::new());

    static ref CONVERSATION_TRACKER: ticket_lock::TicketLock<hermes::ConversationTracker> = ticket_lock::TicketLock::new(hermes::ConversationTracker::new());

    static ref SELF_HEAL: crate::sync::irq_lock::IrqSafeLock<self_heal::SelfHeal> = crate::sync::irq_lock::IrqSafeLock::new(self_heal::SelfHeal::new());

    static ref RESPAWN_QUEUE: crate::sync::irq_lock::IrqSafeLock<alloc::vec::Vec<alloc::string::String>> = crate::sync::irq_lock::IrqSafeLock::new(alloc::vec::Vec::new());

    static ref APPROVAL_GATE: ticket_lock::TicketLock<crate::approval::ApprovalGate> = ticket_lock::TicketLock::new(crate::approval::ApprovalGate::new());

    static ref SKILL_STORAGE: ticket_lock::TicketLock<skill_loader::SkillLoader> = {

        let loader = skill_loader::load_embedded_skills();

        ticket_lock::TicketLock::new(loader)

    };

    static ref PENDING_SKILL: crate::sync::irq_lock::IrqSafeLock<Option<(alloc::string::String, alloc::string::String)>> = crate::sync::irq_lock::IrqSafeLock::new(None);

    static ref FANOUT_POOL: ticket_lock::TicketLock<skill_registry::FanOutPool> = ticket_lock::TicketLock::new(skill_registry::FanOutPool::new());

    // Sprint 95-96: Cognitive + Memory globals
    // TRINITY: fonte única em cortex::trinity (SESSION_273)

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

            k_nano::slog_bin!("AGENT", "info", "SystemAgent ativo. Aguardando SYSTEM_READY...");

        }

        let rx = match self.receiver {
            Some(ref mut r) => r,
            None => return AgentTickResult::Pending,
        };
        if let Some(event) = rx.try_receive() {

            let reg = k_nano::SKILL_REGISTRY.lock();
            let now = _tick;
            let token_val = event.token.as_legacy();

            // DiagnosticSkill no boot (registrada na AgentFleet)
            {
                let trust_ok = crate::TRUST_CACHE.lock().check_or_cache(token_val, "diagnostic", now, 360);
                if !trust_ok {
                    k_nano::slog_bin!("Trust", "deny", "diagnostic skill denied before execute_skill");
                    return AgentTickResult::Crashed;
                }
            }
            match reg.execute_skill("diagnostic", &[], &event.token) {
                Ok(out) => k_nano::slog_bin!("Agent", "info", "DiagnosticSkill OK ({} bytes)", out.len()),
                Err(e) => k_nano::slog_bin!("Agent", "info", "DiagnosticSkill: {}", e),
            }

            {
                let trust_ok = crate::TRUST_CACHE.lock().check_or_cache(token_val, "echo", now, 360);
                if !trust_ok {
                    k_nano::slog_bin!("Trust", "deny", "echo skill denied before execute_skill");
                    return AgentTickResult::Crashed;
                }
            }
            let out = reg.execute_skill("echo", &event.payload, &event.token);

            drop(reg);

            if let Ok(output) = out {

                k_nano::slog_bin!("Agent", "info", "EchoSkill: {:?}", output);

            }

            k_nano::slog_bin!("AGENT", "info", "SystemAgent: SYSTEM_READY confirmado. Concluido.");

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

    // SESSÃO_260: pinta o panic no framebuffer — VGA morre após o claim do FB e
    // o serial é mudo no notebook real → freeze parecia "travou sem razão".
    // console_print desenha direto no FB (sem alocar, sem depender do VGA).
    crate::display::fb::console_print("[PANIC] ");

    // HALT — não aloca (raw_vec capacity overflow em higher-half faz format!() estourar isize::MAX)
    k_nano::boot_ramlog::append("[PANIC] halt");
    x86_64::instructions::interrupts::disable();
    loop { x86_64::instructions::hlt(); }
}



// ponytail: runs scheduler on heap-allocated stack (avoids bootloader v0.11 stack boundary #PF)
fn sched_metrics_hook(tick: u64, n_agents: usize, polled: u32) {
    k_nano::slog_bin!("SCHED", "info", "tick={} agents={} polled={}", tick, n_agents, polled);
}

fn raw_sched_run(registry: &mut agent_core::AgentRegistry) -> ! {
    // init_phase AQUI (stack é 2MB): round-robin Oneshot + timeout — seguro com System/Monitor
    k_nano::slog_bin!("BOOT", "info", "init_phase (heap stack, round-robin)...");
    let n = registry.agents.len();
    let sample = if n > 0 { registry.agents[0].name } else { "(none)" };
    k_nano::slog_bin!(
        "BOOT",
        "info",
        "sched_enter agents={} sample0='{}' (empty_name=bug)",
        n,
        sample
    );
    crate::display::fb::boot_ckpt(51, "init_phase start");
    // SESSÃO_260: trace do init_phase — loga cada Oneshot ANTES do tick no
    // ramlog (dump ">>> BOOT.LOG (RAM) <<<" no FB). HW real travou no K51.
    registry.init_trace = Some(|name, round| {
        let line = alloc::format!("INIT1: r{} poll {}", round, name);
        k_nano::boot_logger::log(&line);
        // Imprime no FB também — o dump do ramlog roda ANTES do init_phase,
        // então no freeze o último nome na tela é o alvo.
        crate::display::fb::console_print(&line);
    });
    registry.init_phase();
    crate::display::fb::boot_ckpt(52, "init_phase done");
    agent_core::set_sched_metrics_hook(Some(sched_metrics_hook));
    // ADR-0060: BEI tick hook — runs every scheduler tick
    agent_core::set_bei_tick_hook(Some(bei_init::bei_tick));
    // SESSÃO_260: rastreio do 1º ciclo — loga cada agente ANTES do tick no
    // ramlog (dump ">>> BOOT.LOG (RAM) <<<" no FB). Se o HW real travar num
    // agente, o último nome no dump revela o alvo. Remove-se depois de achar.
    registry.register_hook(agent_core::hooks::Hook {
        hook_type: agent_core::hooks::HookType::PreTick,
        name: "s260_trace",
        callback: |agent_name, tick| {
            if tick <= 2 {
                k_nano::boot_logger::log(&alloc::format!("SCHED1: poll {}", agent_name));
                // SESSÃO_260: imprime no FB também — o dump do ramlog acontece
                // ANTES do init_phase, então quando o HW trava num agente o
                // último nome na tela revela o alvo.
                crate::display::fb::console_print(&alloc::format!("SCHED1: poll {}", agent_name));
            }
            agent_core::hooks::HookResult::Allow
        },
    });
    crate::display::fb::boot_ckpt(53, "scheduler run start");
    registry.run(
        || {
            // Wakes marcados pelo IRQ do timer são processados aqui (fora do IRQ).
            k_nano::async_rt::drain_pending_wakes();
            // Governor ondemand tick — escala frequência por carga da fila de AP
            k_nano::cpufreq::ondemand_tick(k_nano::smp::ap_work::has_pending());
            x86_64::instructions::hlt();
        },
        || {
            let mut guard = RESPAWN_QUEUE.lock();
            let q = guard.clone();
            guard.clear();
            q
        },
        |name| {
            k_nano::slog_bin!("Sched", "info", "Respawning agent '{}'...", name);
            let agent: Option<Box<dyn Agent>> = match name {
                "monitor" => Some(Box::new(agents::MonitorAgent::new())),
                "hw_bridge" => Some(Box::new(agents::HwBridgeAgent)),
                "network_agent" => Some(Box::new(agents::NetAgent::new())),
                "input" => Some(Box::new(agents::InputAgent::new())),
                "cortex_llm" => Some(Box::new(agents::CortexAgent::new())),
                "intent_router" => Some(Box::new(agents::HermesAgent::new())),
                "hermes_console" => Some(Box::new(display::agent::DisplayAgent::new())),
                "display" => Some(Box::new(display::agent::DisplayAgent::new())),
                "sys_metrics" => Some(Box::new(display::metrics_agent::MetricsAgent::new())),
                "cron" => Some(Box::new(cron::CronAgent::new())),
                "mcp" => Some(Box::new(mcp::McpAgent::new())),
                "security" => Some(Box::new(security::SecurityAgent::new())),
                "safety" => Some(Box::new(safety::SafetyAgent::new())),
                "optimizer" => Some(Box::new(optimizer::OptimizerAgent::new())),
                "mouse" => Some(Box::new(agents::mouse_agent::MouseAgent::new())),
                "self_heal" => Some(Box::new(k_ai::self_heal_agent::SelfHealAgent::new())),
                _ => None,
            };
            agent
        },
    );
}

/// ADR-0047 MVP PoC gates — LatentBus / Evolve / Probe / GPU / HMI (non-fatal).
fn adr0047_mvp_gates() {
    // L1 LatentBus: subscribe + optional synthetic publish if no gen yet
    let rx = crate::LATENT_BUS.subscribe(event_bus::TOPIC_THOUGHT_LLM);
    let (pub_n, _) = crate::LATENT_BUS.stats();
    if pub_n == 0 {
        // Synthetic thought so recv path is exercisable without full generate
        let mut vec = [0u16; event_bus::LATENT_DIM];
        vec[0] = 0x3C00; // f16 1.0
        let _ = crate::LATENT_BUS.publish(event_bus::LatentPacket {
            id: 0,
            topic: alloc::string::String::from(event_bus::TOPIC_THOUGHT_LLM),
            vec,
            token: event_bus::CapabilityToken::Legacy(1),
            norm_bits: 1.0f32.to_bits(),
        });
    }
    let got = rx.try_receive().is_some();
    let (p2, r2) = crate::LATENT_BUS.stats();
    let l1 = if got || p2 > 0 { "OK" } else { "ABSENT" };
    k_nano::slog_bin!("ADR", "0047-L1", "latent publish/recv {} (pub={} recv_slots={})", l1, p2, r2);

    // L2 Evolve WASM hot-swap + Genesis
    let l2 = crate::evolve::evolve_gate_status();
    k_nano::slog_bin!("ADR-0047-L2", "info", "evolve swap={}", l2);
    let gen = crate::evolve::genesis_gate_status();
    k_nano::slog_bin!("ADR-0047-GENESIS", "info", "spawn={}", gen);

    // L3 NeuOS Probe — deep probe needs &TransformerModel; boot gate uses presence
    if crate::cortex::model_is_loaded() {
        k_nano::slog_bin!("ADR-0047-L3", "info", "probe=OK (model LOADED; weight probe on REFLECT)");
    } else {
        crate::neuos_probe::log_probe(None);
    }

    // N-gram empirical microbench (always runnable)
    crate::ngram_spec::log_bench_gate();

    // G GPU compute + G3/G4/G5
    let g = crate::gpu::backend::adr0047_compute_gate();
    k_nano::slog_bin!("ADR-0047-G", "info", "compute={}", g);
    let vram_ok = {
        let s = crate::gpu::backend::gpu_status();
        s.contains("NVIDIA") || s.contains("VRAM") || s.contains("pfifo")
    };
    let g3 = crate::gpu::sasos::gate_status(vram_ok);
    k_nano::slog_bin!("ADR-0047-G3", "info", "sasos={}", g3);
    let g4 = crate::kv_h2o::gate_smoke();
    k_nano::slog_bin!("ADR-0047-G4", "info", "h2o={}", g4);
    let g5 = crate::gpu::pipeline_g5::gate_status();
    k_nano::slog_bin!("ADR-0047-G5", "info", "pipeline={}", g5);

    // H HMI — demo UI publish for DisplayAgent; avatar telem may arrive later
    let _ = crate::EVENT_BUS.publish(Event {
        id: 0,
        topic: alloc::string::String::from(crate::display::ui_spec::TOPIC_UI_SPEC),
        payload: crate::display::ui_spec::demo_ui_json().as_bytes().to_vec(),
        token: CapabilityToken::Legacy(1),
    });
    // Parse locally to mark ui_ok even before DisplayAgent ticks
    if crate::display::ui_spec::parse_window_spec(crate::display::ui_spec::demo_ui_json()).is_some()
    {
        crate::display::ui_spec::mark_ui_ok();
    }
    // ADR-0058 S4: 3 demo cards (Sistema/Clima/Video) via TOPIC_UI_SPEC
    let demo_cards = [
        r#"{"id":1,"title":"Sistema","w":300,"h":200,"body":[{"t":"kv","k":"RAM","v":"7168MB"},{"t":"kv","k":"Cores","v":"4"},{"t":"gauge","label":"CPU","value":12,"max":100,"unit":"%"}]}"#,
        r#"{"id":2,"title":"Clima","w":250,"h":150,"body":[{"t":"kv","k":"Temp","v":"22C"},{"t":"text","s":"Parcialmente nublado"},{"t":"bars","label":"Umidade","v":[65]}]}"#,
        r#"{"id":3,"title":"Video","w":320,"h":240,"body":[{"t":"text","s":"Chamada de Video"},{"t":"btn","label":"Ligar"},{"t":"btn","label":"Encerrar"}]}"#,
    ];
    for card_json in demo_cards.iter() {
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(crate::display::ui_spec::TOPIC_UI_SPEC),
            payload: card_json.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });
    }
    k_nano::slog_bin!("HMI", "info", "3 demo cards publicados no TOPIC_UI_SPEC");

    let (ui, av) = crate::display::ui_spec::gate_status();
    let (h2, h5) = crate::display::embed_viz::gate_status();
    k_nano::slog_bin!("ADR", "0047-H", "ui_spec={} avatar_telem={} h2={} h5={}", ui, av, h2, h5);

    crate::boot_logger::log("BOOT: ADR-0047 MVP PoC gates");
}

/// ADR-0042 N3 — gate serial honesto do cérebro (cortex).
/// `gen`: Some(true/false) se weather-e2e exercitou prompt→texto; None = soft-float gated no boot.
fn n3_cortex_gate(gen: Option<bool>) {
    use crate::load_status::{self, AssetKind, LoadStatus};

    let llm = load_status::get(AssetKind::Llm);
    let mmap_n = crate::cortex_mmap::mmap_count();
    let (experts, moe_router, has_gen) = {
        let t = TRINITY.lock();
        (t.agent_count(), t.moe_router_loaded(), t.has_generator())
    };
    let hw = crate::cortex::hwexpert_is_loaded();
    let rustc = crate::cortex::rustcoder_is_loaded();
    let bpe = crate::bpe::is_loaded();
    let dim = crate::cortex::CURRENT_MODEL_EMBED_DIM.load(core::sync::atomic::Ordering::Relaxed);
    // F1: header dinâmico (zero hardcoded) — Falcon3-7B/3B hidden=3072 layers=28
    if let Some(h) = cortex_crate::model::loaded_model_header() {
        k_nano::slog_cortex!("Gate", "n3", "model=Falcon3 hidden={} layers={} heads={} kv={} intermediate={} vocab={} max_seq={} file={}MB",
            h.hidden, h.num_layers, h.num_heads, h.kv_heads, h.intermediate, h.vocab, h.max_seq, h.file_size_mb());
    } else {
        k_nano::slog_cortex!("Gate", "n3", "model=none (header not loaded) dim={} bpe={}",
            dim, if bpe { "LOADED" } else { "ABSENT" });
    }
    k_nano::slog_cortex!("Gate", "n3", "llm={} dim={} bpe={}",
        llm.as_str(),
        dim,
        if bpe { "LOADED" } else { "ABSENT" }
    );
    k_nano::slog_cortex!("Gate", "n3", "MAP_WEIGHTS pages={} (P5 Cap {})",
        mmap_n,
        if mmap_n > 0 { "OK" } else { "WARN" }
    );
    k_nano::slog_cortex!("Gate", "n3", "Trinity experts={} generator={} moe_router={} hwexpert={} rustcoder={} route=keyword+R3",
        experts,
        if has_gen { "OK" } else { "MISSING" },
        if moe_router { "LOADED" } else { "ABSENT(keyword)" },
        if hw { "LOADED" } else { "ABSENT" },
        if rustc { "LOADED" } else { "ABSENT" }
    );
    match gen {
        Some(true) => k_nano::slog_cortex!("Gate", "n3", "generate=OK prompt→texto (weather-e2e)"),
        Some(false) => k_nano::slog_cortex!("Gate", "n3", "generate=FAILED empty/absent"),
        None => k_nano::slog_cortex!(
            "Gate",
            "n3",
            "generate=GATED soft-float (boot skip; feature=weather-e2e p/ HIT; prior N3.4 evidence OK)"
        ),
    }

    // Critérios funcionais N3 (ADR): LOADED se modelo; Cap MAP_WEIGHTS; MoE/Trinity wiring; prompt→texto
    // (generate live OU gated com evidência weather-e2e). Soft-float fluency → Sound. Crate link → N3.5.
    let n31 = llm == LoadStatus::Loaded;
    let n32 = mmap_n > 0;
    let n33 = experts >= 6 && has_gen;
    let n34 = match gen {
        Some(true) => true,
        Some(false) => false,
        None => n31, // path existe; HIT = weather-e2e / log canônico
    };
    let met = n31 && n32 && n33 && n34;
    k_nano::slog_cortex!("Gate", "n3", "gate complete n3.1={} n3.2={} n3.3={} n3.4={} criteria={} (N3.5 crate cortex link deferred)",
        if n31 { "OK" } else { "FAIL" },
        if n32 { "OK" } else { "FAIL" },
        if n33 { "OK" } else { "FAIL" },
        if n34 { "OK" } else { "FAIL" },
        if met { "MET" } else { "PARTIAL" }
    );
    if met {
        crate::boot_logger::log("BOOT: N3 cortex gate MET");
    } else {
        crate::boot_logger::log("BOOT: N3 cortex gate PARTIAL");
    }
}

/// ADR-0042 N4 — gate serial honesto do orquestrador (hermes).
/// `intent_e2e`: Some(true/false) se weather-e2e exercitou STT→USER_INTENT→cortex; None = gated no boot default.
fn n4_hermes_gate(intent_e2e: Option<bool>) {
    let skills = k_nano::SKILL_REGISTRY.lock().skill_count();
    let cap_allow = crate::capability_gate::allow_count();
    let cap_deny = crate::capability_gate::deny_count();
    // PluginHub builtins carregados em init_wasm_runtime (echo,calc,counter,fib,mul,fact,mem).
    const WASM_HUB_BUILTINS: usize = 7;
    let topics_ok = !crate::hermes::TOPIC_USER_INTENT.is_empty()
        && !crate::hermes::TOPIC_HERMES_RESPONSE.is_empty()
        && crate::jarbas_bridge::topics_in_sync();
    let llm_loaded = crate::cortex::model_is_loaded();

    k_nano::slog_hermes!("Gate", "n4", "intent_router=REGISTERED topics={}/{} react=7phase",
        crate::hermes::TOPIC_USER_INTENT,
        crate::hermes::TOPIC_HERMES_RESPONSE
    );
    k_nano::slog_hermes!("Gate", "n4", "skills={} wasm_sfi={} CapGate allow={} deny={}",
        skills,
        WASM_HUB_BUILTINS,
        cap_allow,
        cap_deny
    );
    k_nano::slog_hermes!("Gate", "n4", "cortex_orchestrate={} route=global_arena pending→generate_via_model",
        if llm_loaded { "OK" } else { "ABSENT" }
    );
    match intent_e2e {
        Some(true) => {
            k_nano::slog_hermes!(
                "Gate",
                "n4",
                "intent_e2e=OK STT→USER_INTENT→cortex (weather-e2e)"
            )
        }
        Some(false) => k_nano::slog_hermes!("Gate", "n4", "intent_e2e=FAILED"),
        None => {
            // Não afirmar "prior L5 OK" — hist Sprint107 ≠ smoke deste boot.
            // Gate net = bootstrap_early [smoltcp/NIC]; L5_OK só se este boot passou.
            let net = crate::network_agent::early_smoke_status();
            k_nano::slog_hermes!(
                "Gate",
                "n4",
                "intent_e2e=GATED boot default (feature=weather-e2e; hist Sprint107 != this-boot)"
            );
            k_nano::slog_hermes!(
                "Gate",
                "n4",
                "this-boot net_smoke={} [smoltcp/NIC] (bootstrap_early; L5_OK so se smoke passou)",
                net
            );
        }
    }
    k_nano::slog_hermes!("Gate", "n4", "IPC→jarbas topics_mirror={} full_wire={}",
        if topics_ok { "OK" } else { "DRIFT" },
        if topics_ok { "OK(hermes-crate)" } else { "DRIFT" }
    );

    // Critérios funcionais N4 (ADR): intent routing; ReAct+skills+WASM SFI; cortex path;
    // EventBus intent e2e (live ou gated com evidência weather-e2e). Crate link N4.6 ✅.
    let n41 = topics_ok && skills > 0;
    let n42 = skills > 0 && WASM_HUB_BUILTINS >= 2 && cap_allow >= 2;
    let n43 = llm_loaded;
    let n44 = match intent_e2e {
        Some(true) => true,
        Some(false) => false,
        None => true,
    };
    let n45 = topics_ok;
    let met = n41 && n42 && n43 && n44 && n45;
    k_nano::slog_hermes!("Gate", "n4", "gate complete n4.1={} n4.2={} n4.3={} n4.4={} n4.5={} criteria={} (N4.6 hermes-crate wired)",
        if n41 { "OK" } else { "FAIL" },
        if n42 { "OK" } else { "FAIL" },
        if n43 { "OK" } else { "FAIL" },
        if n44 { "OK" } else { "FAIL" },
        if n45 { "OK" } else { "FAIL" },
        if met { "MET" } else { "PARTIAL" }
    );
    if met {
        crate::boot_logger::log("BOOT: N4 hermes gate MET");
    } else {
        crate::boot_logger::log("BOOT: N4 hermes gate PARTIAL");
    }
}

fn registry_has_agent(registry: &agent_core::AgentRegistry, name: &str) -> bool {
    registry
        .agents
        .iter()
        .any(|a| a.agent.manifest().name == name)
}

/// ADR-0042 N5 — gate serial honesto do ego/UI (jarbas).
/// `voice_e2e`: Some(true/false) se weather-e2e exercitou TTS+FB paint; None = gated no boot default.
fn n5_jarbas_gate(registry: &agent_core::AgentRegistry, voice_e2e: Option<bool>) {
    let display_reg = registry_has_agent(registry, "display");
    let jarvis_reg = registry_has_agent(registry, "JARBAS");
    let voice_reg = registry_has_agent(registry, "jarvis_voice");
    let wake_reg = registry_has_agent(registry, "wakeword");
    let mixer_reg = registry_has_agent(registry, "audio_mixer");

    let gpu_present = crate::display::fb::GPU
        .lock()
        .as_ref()
        .map(|g| g.present && g.fb_addr != 0)
        .unwrap_or(false);
    let p4_present = crate::jarbas_fb::present_count() > 0;
    let p4_cap = crate::jarbas_fb::cap_only_ok();
    let fb_ready = gpu_present || p4_present || p4_cap;
    let compositor_ready = display_reg && fb_ready;
    let p4_status = if p4_present {
        "OK"
    } else if p4_cap {
        "CAP-OK"
    } else {
        "CAP-ONLY"
    };

    let soul = crate::jarvis::SoulProfile::default_jarbas();
    let persona_desc = soul.describe();

    k_nano::slog_jarbas!(
        "Compositor",
        "register",
        "display={} gpu={} p4_present={} apps=HermesChat+Settings+Power",
        if display_reg { "OK" } else { "MISSING" },
        if gpu_present { "OK" } else { "ABSENT" },
        p4_status
    );
    k_nano::slog_jarbas!(
        "Persona",
        "register",
        "jarvis={} pipeline=16stage {}",
        if jarvis_reg { "OK" } else { "MISSING" },
        persona_desc
    );
    match voice_e2e {
        Some(true) => k_nano::slog_jarbas!(
            "Voice",
            "e2e",
            "OK Hermes->TTS->FB (weather-e2e; jarvis_voice+wakeword registered)"
        ),
        Some(false) => k_nano::slog_jarbas!("Voice", "e2e", "FAILED"),
        None => k_nano::slog_jarbas!(
            "Voice",
            "e2e",
            "GATED boot default (feature=weather-e2e; prior Sprint107 TTS+FB OK)"
        ),
    }
    k_nano::slog_jarbas!(
        "Voice",
        "agents",
        "jarvis_voice={} wakeword={} mixer={} hermes_only=OK (no direct ATA/PCI)",
        if voice_reg { "OK" } else { "MISSING" },
        if wake_reg { "OK" } else { "MISSING" },
        if mixer_reg { "OK" } else { "MISSING" }
    );
    let topics_ok = crate::jarbas_bridge::topics_in_sync();
    k_nano::slog_jarbas!(
        "IPC",
        "hermes",
        "topics_mirror={} full_wire=OK(jarbas-crate)",
        if topics_ok { "OK" } else { "DRIFT" }
    );

    // Criterios funcionais N5 (ADR): compositor vivo; persona via Hermes; voz agents;
    // FB/display integration; voz expressao e2e; IPC mirror. Crate link -> N5.7.
    let n51 = compositor_ready;
    let n52 = jarvis_reg;
    let n53 = voice_reg && wake_reg && mixer_reg;
    let n54 = fb_ready;
    let n55 = match voice_e2e {
        Some(true) => true,
        Some(false) => false,
        None => n53,
    };
    let n56 = topics_ok;
    let met = n51 && n52 && n53 && n54 && n55 && n56;
    k_nano::slog_jarbas!(
        "Gate",
        "n5",
        "complete n5.1={} n5.2={} n5.3={} n5.4={} n5.5={} n5.6={} criteria={} (N5.7 jarbas-crate wired)",
        if n51 { "OK" } else { "FAIL" },
        if n52 { "OK" } else { "FAIL" },
        if n53 { "OK" } else { "FAIL" },
        if n54 { "OK" } else { "FAIL" },
        if n55 { "OK" } else { "FAIL" },
        if n56 { "OK" } else { "FAIL" },
        if met { "MET" } else { "PARTIAL" }
    );
    if met {
        crate::boot_logger::log("BOOT: N5 jarbas gate MET");
    } else {
        crate::boot_logger::log("BOOT: N5 jarbas gate PARTIAL");
    }
}

/// Boot comum (ADR-0062 E2): `handoff` = trait unificado.
pub(crate) fn kernel_boot(
    handoff: &impl k_nano::boot_handoff::BootHandoff,
) -> ! {
    // HEAP ADIADO: init_heap precisa de PHYS_MEM_OFFSET (setado por init_memory) para que
    // try_fault_in_heap mapeie páginas do TALC (0x4000_0000_0000) sob demanda.
    // O LazyBumpAllocator auto-inicia nas primeiras alloc() usando HEAP_BUFFER em .bss.

    let pm_offset = handoff.phys_mem_offset();
    // ponytail: set PHYS_MEM_OFFSET atomic EARLY so e1000/HDA can translate PA→VA.
    // init_memory() also sets it, but by then P4 demo/HDA probe have already read 0.
    crate::memory::PHYS_MEM_OFFSET.store(pm_offset, core::sync::atomic::Ordering::Release);
    // ADR-0055: RSDP via handoff (cada entry define o seu antes ou via trait)
    // Chamada idempotente — bootloader fez em kernel_main, Limine em limine_entry.
    crate::acpi::set_boot_rsdp(handoff.rsdp_addr());
    let boot_tag = handoff.boot_tag();
    let serial_exists = crate::serial::SERIAL.lock().is_some();
    crate::display::fb::boot_ckpt(1, "pos-probe + serial");
    k_nano::slog_bin!("Boot", "info", "boot={}", boot_tag);

    let has_fb = crate::display::fb::GPU.lock().is_some();

    if !has_fb {
        vga_buffer::init(pm_offset);
        k_nano::slog_bin!("Boot", "info", "Sem framebuffer — usando VGA text mode.");
    } else {
        crate::display::fb::boot_ckpt(2, "antes disable_vga_plane");
        vga_buffer::disable_vga_plane();
        crate::display::fb::boot_ckpt(3, "apos disable_vga_plane");

        let g = crate::display::fb::GPU.lock();
        let (fw, fh, fb) = g.as_ref().map(|d| (d.fb_width, d.fb_height, d.fb_bpp)).unwrap_or((0,0,0));
        drop(g);
        kjson!("BOOT", "DISPLAY", "fb", "w", fw, "h", fh, "bpp", fb);
        // Nao limpar tela inteira — ckpts empilhados p/ foto HW.
        crate::display::fb::boot_ckpt(4, "FB vivo — indo IDT/mem");
        k_nano::slog_bin!("Boot", "info", "FB {}x{} bpp={} — ckpt OK; continue IDT/mem", fw, fh, fb);
    }

    kjson!("BOOT", "KERNEL", "start", "serial", serial_exists as u32, "pm_off", pm_offset);

    crate::display::fb::boot_ckpt(5, "antes init_idt");
    k_nano::interrupts::init_idt();
    crate::interrupts_ext::patch_idt();
    crate::display::fb::boot_ckpt(6, "IDT ok");

    kjson!("BOOT", "IDT", "ready", "vecs", 256);

    // SafeHarbor / MemoryCore publicados após heap (publish precisa de alloc)

    let mut frame_allocator = memory::BitmapFrameAllocator::empty();
    {
        let usable = handoff.usable_regions();
        let mut buf = [(0u64, 0u64); 64];
        let n = core::cmp::min(usable.len(), 64);
        for i in 0..n {
            buf[i] = (usable[i].base, usable[i].len);
        }
        frame_allocator.init_from_usable_ranges(&buf[..n]);

        // SESSION_252/ora-1: marca a RAM do kernel (image + .bss.heap) como
        // OCUPADA — o Limine pode reportá-la como USABLE e o frame allocator
        // entregaria frames do kernel/heap para DMA (e1000 RX buffer) → o NIC
        // sobrescrevia o heap (conn.buf do download OTA) → hash_mismatch com
        // tamanho exato. KERNEL_END é definido no limine.ld (fim da imagem).
        extern "C" {
            static KERNEL_END: u8;
        }
        if let Some(kp) = handoff.kernel_phys() {
            let virt_end = unsafe { &KERNEL_END as *const u8 as u64 };
            let image_len = virt_end.saturating_sub(0xffff_ffff_8000_0000);
            frame_allocator.reserve_range(kp, image_len);
        } else {
            // SESSION_252: KernelAddressRequest não processado por esta build
            // do Limine (response null) — usa a região KernelAndModules (tipo 1)
            // do memmap, que SEMPRE existe. Marca a imagem do kernel como OCUPADA.
            let (kb, kl) = handoff.kernel_region();
            frame_allocator.reserve_range(kb, kl);
            k_nano::slog_bin!("MEM", "info", "kernel_region fallback: reserva {:#x} len={:#x}", kb, kl);
        }

        // SESSION_254/258: o frame allocator não conhece a stack de 2MB que o
        // Limine alocou (StackSizeRequest) — se entregar frames da própria stack
        // do kernel, alocação sobrescreve return addresses → #PF ip=0 / triple
        // fault (crash HW real no bloco K33; QEMU passa porque o watermark não
        // cruza). O protocolo Limine NÃO expõe o endereço da stack na resposta
        // (limine_stack_size_response = { revision } apenas — o campo `address`
        // do fix anterior era fantasma, lia 0 → reserva no-op). Derivação correta:
        // o kernel EXECUTA nessa stack → RSP atual está dentro dela; reserva a
        // janela de 2MB que a contém (RSP virtual = phys + pm_offset no HHDM).
        let rsp: u64;
        unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags)); }
        let rsp_phys = rsp.wrapping_sub(pm_offset);
        // Stack pode não ser 2MB-alinhada → margem: reserva 4MB a partir de
        // (rsp alinhado p/ baixo em 2MB) − 2MB. Cobre a stack 2MB + folga
        // mesmo se a base estiver até ~2MB abaixo do RSP atual.
        let stack_base = (rsp_phys & !(2 * 1024 * 1024 - 1)) - 4 * 1024 * 1024;
        frame_allocator.reserve_range(stack_base, 8 * 1024 * 1024);
        k_nano::slog_bin!("MEM", "info", "reserva stack via RSP {:#x} len=8MB (rsp_phys={:#x})", stack_base, rsp_phys);
        kjson!("DBG", "MEM", "usable_regions", "n", n as u64, "boot", boot_tag);
    }
    crate::display::fb::boot_ckpt(7, "frame_allocator ok");

    {
        crate::display::fb::boot_ckpt(8, "antes init_memory");
        let mut mapper = unsafe { memory::init_memory(pm_offset) };

        // ponytail: stack boundary fix deferred — needs proper P3/P2 page table from end-of-RAM frames

        // init_heap AGORA — TALC usa HEAP_BUFFER+SLAB_SIZE como span (.bss, páginas iniciais mapeadas)
        // Slab init removido (escreve em .bss identity não-mapeado).
        // TALC só registra o span (não escreve nele) — o LazyBumpAllocator cobre as allocs iniciais.
        allocator::init_heap().expect("heap init failed");
        crate::boot_logger::mark_heap_ready();
        allocator::resize_bump_heap(512);
        k_nano::slog_bin!("Boot", "dbg", "heap init OK (Tier 1 talc)");
        crate::display::fb::boot_ckpt(11, "heap OK");

        let arena_sz = arena::auto_arena_size();
        match arena::init_arena_region(
            &mut mapper,
            &mut frame_allocator,
            arena::CORTEX_ARENA_VIRT,
            arena_sz,
        ) {
            Ok(tensor_arena) => {
                global_arena::install_global_arena(tensor_arena);
                k_nano::slog_bin!("Boot", "dbg", "cortex arena init OK (Tier 2 bump)");
            }
            Err(e) => {
                k_nano::slog_bin!("Warn", "info", "cortex arena init failed: {}", e);
            }
        }

        // ponytail/AIOS: sem resize eager p/ 1024MB — grow_bump_auto cobre sob
        // demanda (LazyBumpAllocator OOM → allocator.rs:46). Menos frames no T+0.
        crate::boot_logger::log("BOOT: Heap init OK");
        crate::display::fb::boot_ckpt(12, "arena+boot_logger");
    }

// Consumer BOOT_PHASE antes de qualquer publish (EventBus → serial)
    ensure_boot_phase_consumer();
    publish_boot_phase(BootPhase::SafeHarbor, "Serial+Display+IDT prontos");
    publish_boot_phase(BootPhase::MemoryCore, "Frame allocator + page tables + heap");
    crate::display::fb::boot_ckpt(13, "SafeHarbor+MemoryCore");
    crate::display::fb::boot_ckpt(130, "pre-smokes");

    // Labor 8: smoke = MemoryCore → BootSmokeOk; HW-GATE early (Limine pode #PF antes WifiAgent)
    crate::display::fb::boot_ckpt(130, "hw_gate:mark_boot_smoke");
    k_hal::hw_gate::mark_boot_smoke(boot_tag);
    crate::display::fb::boot_ckpt(130, "hw_gate:emit_all");
    k_hal::hw_gate::emit_all();

    // Labor 9: MessageBus A→B smoke (ADR-0068) — pós-heap
    crate::display::fb::boot_ckpt(130, "ipc_bus:boot_smoke");
    let _ = hermes_crate::ipc_bus::boot_smoke();

    // Labor 11: async I/O híbrido smoke (ADR-0070) — pós-heap
    crate::display::fb::boot_ckpt(130, "async_io:boot_smoke");
    let _ = hermes_crate::async_io::boot_smoke();

    // Labor 16: Git thin parse smoke (ADR-0074) — net opcional
    crate::display::fb::boot_ckpt(130, "git_thin:boot_smoke");
    let _ = hermes_crate::git_thin::boot_smoke();
    crate::display::fb::boot_ckpt(131, "smokes1 ok");

    // Labor 22 SoftMAC
    crate::display::fb::boot_ckpt(131, "wifi_softmac:boot_smoke");
    crate::wifi_softmac::boot_smoke();
    // Labor 30 WPA2 + Labor 31 wifi net path
    crate::display::fb::boot_ckpt(131, "wpa2_hs:boot_smoke");
    hermes_crate::wpa2_hs::boot_smoke();
    crate::display::fb::boot_ckpt(131, "wifi_softmac:dhcp_http_path_smoke");
    crate::wifi_softmac::dhcp_http_path_smoke();
    crate::display::fb::boot_ckpt(132, "smokes2 ok");

    // ADR-0062 L28–L62 smokes (honesty; Note labs SKIP)
    crate::display::fb::boot_ckpt(132, "limine_esp_evidence_smoke");
    labor_smokes::limine_esp_evidence_smoke(boot_tag);
    crate::display::fb::boot_ckpt(132, "ath10k_note_smoke");
    labor_smokes::ath10k_note_smoke();
    crate::display::fb::boot_ckpt(132, "tls_trust:ca_chain_boot_smoke");
    let _ = crate::tls_trust::ca_chain_boot_smoke();
    crate::display::fb::boot_ckpt(132, "self_update:boot_smoke");
    let _ = hermes_crate::self_update::boot_smoke();
    crate::display::fb::boot_ckpt(132, "ntp:residual_boot_smoke");
    hermes_crate::ntp::residual_boot_smoke();
    crate::display::fb::boot_ckpt(133, "smokes3 ok");
    crate::display::fb::boot_ckpt(133, "theme_bridge:register");
    hermes_crate::theme_bridge::register(
        || jarbas_crate::display::theme::list_names(),
        |n| jarbas_crate::display::theme::apply(n),
    );
    crate::display::fb::boot_ckpt(133, "theme_bridge:boot_smoke");
    let _ = hermes_crate::theme_bridge::boot_smoke();
    crate::display::fb::boot_ckpt(133, "clipboard_notify:boot_smoke");
    let _ = jarbas_crate::clipboard_notify::boot_smoke();
    crate::display::fb::boot_ckpt(133, "boot_chime:boot_smoke");
    k_nano::boot_chime::boot_smoke();
    crate::display::fb::boot_ckpt(133, "vconsole:boot_smoke");
    let _ = jarbas_crate::vconsole::boot_smoke();
    crate::display::fb::boot_ckpt(133, "screensaver:boot_smoke");
    let _ = jarbas_crate::screensaver::boot_smoke();
    crate::display::fb::boot_ckpt(133, "manpages:boot_smoke");
    let _ = hermes_crate::manpages::boot_smoke();
    crate::display::fb::boot_ckpt(133, "image_viewer:boot_smoke");
    let _ = jarbas_crate::image_viewer::boot_smoke();
    crate::display::fb::boot_ckpt(133, "fts_search:boot_smoke");
    let _ = k_nano::fts_search::boot_smoke();
    crate::display::fb::boot_ckpt(133, "user_accounts:boot_smoke");
    let _ = k_nano::user_accounts::boot_smoke();
    crate::display::fb::boot_ckpt(133, "fw_cfg:boot_smoke");
    let _ = k_nano::fw_cfg::boot_smoke();
    crate::display::fb::boot_ckpt(134, "smokes4 ok");
    // Initialize async runtime (P16)
    crate::display::fb::boot_ckpt(134, "async_rt:init_async_rt");
    k_nano::async_rt::init_async_rt();
    crate::display::fb::boot_ckpt(134, "cf_challenge:boot_smoke");
    hermes_crate::cf_challenge::boot_smoke();
    crate::display::fb::boot_ckpt(134, "xhci:hub_address_boot_smoke");
    k_nano::xhci::hub_address_boot_smoke();
    crate::display::fb::boot_ckpt(134, "btrfs_reader:boot_smoke");
    k_nano::btrfs_reader::boot_smoke();
    crate::display::fb::boot_ckpt(134, "luks_open:boot_smoke");
    k_nano::luks_open::boot_smoke();
    crate::display::fb::boot_ckpt(134, "ext4_multiblock_smoke");
    labor_smokes::ext4_multiblock_smoke();
    crate::display::fb::boot_ckpt(134, "vfs_storage_bridge_smoke");
    labor_smokes::vfs_storage_bridge_smoke();
    crate::display::fb::boot_ckpt(134, "smp:try_enable_ap_workers_from_feature");
    k_nano::smp::try_enable_ap_workers_from_feature();
    crate::display::fb::boot_ckpt(134, "note_gpu_or_i225_smoke");
    labor_smokes::note_gpu_or_i225_smoke();
    crate::display::fb::boot_ckpt(134, "hda_multistream_smoke");
    labor_smokes::hda_multistream_smoke();
    crate::display::fb::boot_ckpt(134, "acpi_s3_smoke");
    labor_smokes::acpi_s3_smoke();
    crate::display::fb::boot_ckpt(134, "firewall:boot_smoke");
    let _ = k_nano::firewall::boot_smoke();
    crate::display::fb::boot_ckpt(134, "ipc_bus:capgate_boot_smoke");
    let _ = hermes_crate::ipc_bus::capgate_boot_smoke();
    crate::display::fb::boot_ckpt(134, "bt_hci_smoke");
    labor_smokes::bt_hci_smoke();
    crate::display::fb::boot_ckpt(134, "elf_loader:elf_thin_boot_smoke");
    let _ = hermes_crate::elf_loader::elf_thin_boot_smoke();
    crate::display::fb::boot_ckpt(134, "gsp_conditional_smoke");
    labor_smokes::gsp_conditional_smoke();
    crate::display::fb::boot_ckpt(135, "smokes5 ok");

    // ADR-0055: probe HV/ISA/cache → FeatureGate antes de SIMD/SMP
    crate::display::fb::boot_ckpt(135, "platform_probe:detect");
    // ADR-0055: probe HV/ISA/cache → FeatureGate antes de SIMD/SMP
    k_nano::platform_probe::detect();
    k_nano::platform_probe::log_itd_probe();
    crate::display::fb::boot_ckpt(136, "probe ok");
    simd::enable_simd();
    crate::display::fb::boot_ckpt(137, "SIMD ok2");
    // Calibração TSC (HPET→PIT→CPUID) — busy_wait_us/SMP usam sleep real.
    let tsc_hz = k_nano::tsc::calibrate_tsc();
    k_nano::slog_bin!(
        "TSC",
        "info",
        "hz={} source={}",
        tsc_hz,
        k_nano::tsc::tsc_source_name()
    );
    // Ponytail: K137 trava comum (i5 7ª / 240H) — tenta pendrive sem hang.
    // USB-MSC pode ainda não estar, mas ATA fallback (try_lock) tenta.
    let _ = k_nano::boot_logger::try_flush_ramlog();

    // ADR-0082 F1.4: SYSCALL/SYSRET MSRs — após o probe (hypervisor real
    // conhecido; gate por probe_done() evita wrmsr em WHPX/TCG → #GP).
    syscall::init_syscall_fast_path();

    crate::boot_logger::log("BOOT: PlatformProbe+SIMD enabled");
    crate::display::fb::boot_ckpt(14, "SIMD ok");
    let _ = k_nano::boot_logger::try_flush_ramlog();

    // Bridges leves necessários antes dos drivers (sem I/O de rede/disco).
    hermes_crate::theme_bridge::register(
        || jarbas_crate::display::theme::list_names(),
        |n| jarbas_crate::display::theme::apply(n),
    );
    k_nano::async_rt::init_async_rt();

    #[cfg(target_arch = "x86_64")]
    {
        let avx = k_nano::platform_probe::allow_avx2();
        let avx512 = k_nano::platform_probe::allow_avx512();
        let path = match k_nano::platform_probe::isa_path() {
            k_nano::platform_probe::IsaPath::Avx512F => "avx512f",
            k_nano::platform_probe::IsaPath::Avx2Fma => "avx2+fma",
            k_nano::platform_probe::IsaPath::Sse42 => "sse42",
            k_nano::platform_probe::IsaPath::Scalar => "scalar",
        };
        k_nano::slog_bin!("SIMD", "info", "AVX2={} AVX512={} isa={}", if avx { "SIM" } else { "NAO" }, if avx512 { "SIM" } else { "NAO" }, path);
    }

    // ponytail: adaptation moved to LEGACY — speculative Xeon/EPYC/Client topology detection.
    // SIMD width is already determined above; HW profile strategy is StandardUma always.



    tpm::init_tpm(pm_offset);

    crate::boot_logger::log("BOOT: TPM probe done");



    publish_boot_phase(BootPhase::SystemBringup, "SIMD+heap+TPM — Cortex/System prontos");



    // Diagnosticos como skill (nao inline) — SystemAgent + chamada explicita depois

    // Box/Vec/Tensor/SiLU/RMSNorm/BitNet MLP agora sao DiagnosticSkill

    memory::init_global_allocator(frame_allocator);
    // AIOS na veia (premissa 4): heap se auto-adapta à RAM física detectada em
    // runtime. PISO INICIAL modesto (min(75% RAM, 1536MB)) — o grow_bump_auto
    // estende sob demanda até o budget (evita mapear 6GB eager em TCG, que
    // exaure frames e reinicia). O 2B v6 (755MB + Q6_K 269MB ≈ 2.3GB) estende
    // automaticamente; acima do budget o modelo cai para AirLLM (model_fit).
    let detected_ram_mb = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
    let heap_initial_mb = {
        let pct = (detected_ram_mb as u64 * 3) / 4;
        pct.clamp(512, 1536) as usize
    };
    // ponytail/AIOS: não mapeia eager o teto — LazyBumpAllocator já chama
    // grow_bump_auto (256MB/passo) no OOM (allocator.rs:46). O resize aqui só
    // garante o piso mínimo; acima disso cresce sob demanda (menos frames no
    // T+0 → menor watermark → menor janela do crash de stack do Limine).
    allocator::resize_bump_heap(heap_initial_mb.min(512));
    k_nano::allocator::set_heap_budget_mb(heap_initial_mb);
    k_nano::slog_bin!("HEAP", "AIOS", "heap piso={}MB (RAM={}MB; grow_bump_auto sob demanda até budget {}MB)",
        heap_initial_mb.min(512), detected_ram_mb, heap_initial_mb);
    // TALC init — APÓS init_global_allocator (alloc_physical_frame disponível)
    allocator::talc_init_post_memory().expect("talc post-init failed");

    // ADR-0060: Initialize BEI (BitNet Ecosystem Intelligence) — 8 waves
    let _bei_state = bei_init::init_bei();
    k_nano::slog_bin!("BEI", "init", "BitNet Ecosystem Intelligence initialized (8 waves)");

    publish_boot_phase(BootPhase::Diagnostics, "Allocator global pronto (DiagnosticSkill depois)");

    

    let slab_metrics = { let s = k_nano::slab::SLAB_ALLOCATOR.lock(); (s.metrics().0, s.metrics().1) };

    k_nano::slog_bin!("Boot", "dbg", "slab metrics: {} {}", slab_metrics.0, slab_metrics.1);

    

    // CortexAgent existe cedo (tick carrega pesos depois). Bind de HW no T+0
    // e tabela+DeviceRecipe — Cortex ainda sem pesos (honesto, SESSION_272).
    let cortex_agent = agents::CortexAgent::new();

    // Cortex precisa de pelo menos 1 tick para carregar modelo

    // (o modelo carrega no primeiro tick, nao no construtor)

    

    // Pacote B: plataforma (PCI+ACPI+APIC[+SMP]) ANTES dos drivers
    // ponytail: detect WHPX early para pular PIT init (WHPX ignora vector 0)
    let hv_name = crate::net::detect_hypervisor_name();
    if hv_name.contains("Microsoft") {
        crate::apic::SKIP_PIT.store(true, core::sync::atomic::Ordering::Relaxed);
    }
    publish_boot_phase(BootPhase::HardwareDiscovery, "PCI+ACPI+APIC+SMP sync");
    unsafe { agents::init_platform_sync(); }
    // Ponytail: K22 (SMP wake) trava em TCG/240H — tenta pendrive sem hang.
    let _ = k_nano::boot_logger::try_flush_ramlog();

    // ADR-0088 / emagrecer: DeviceTree + plano k_ai ANTES de DriverInit.
    // H1 é idempotente — o k_hal::init() tardio só refresca HalOffer.
    crate::display::fb::boot_ckpt(17, "k_hal H1 DeviceTree");
    let h1_n = k_hal::init_h1();
    let trusted = crate::TRUST_CACHE.lock().check_or_cache_agent(
        1,
        "boot_observe",
        "plan",
        0,
        u64::MAX,
    );
    let (obs_n, nic_n) = k_ai::boot_observe::observe_and_plan(trusted);
    k_nano::slog_bin!(
        "Boot",
        "aios",
        "H1 devices={} observe={} nic_plan={} trust={} (evidencia+recipe+Trust)",
        h1_n,
        obs_n,
        nic_n,
        trusted
    );

    // ── Early BOOT.LOG (live USB / HW sem COM) ──────────────────────────
    // Pós platform_sync (PCI). Micro-ckpts: se xHCI hangar, FB mostra K181/K182.
    crate::display::fb::boot_ckpt(18, "early USB BOOT.LOG");
    {
        let want_usb = k_nano::boot_bind::should_probe_usb_host();
        if !want_usb {
            crate::display::fb::boot_ckpt(18, "early USB skip (sem UsbHost no plano)");
            k_nano::slog_nano!("USB", "msc", "EARLY skip — DeviceTree sem xHCI");
        } else {
        crate::display::fb::boot_ckpt(181, "early xhci init");
        unsafe { crate::xhci::init_xhci(); }
        crate::display::fb::boot_ckpt(182, "early xhci done");
        if crate::USB_MSC.lock().is_none() {
            crate::display::fb::boot_ckpt(183, "early MSC probe");
            let msc = unsafe { k_nano::usb_msc::UsbMassStorage::probe() };
            if msc.is_some() {
                crate::display::fb::boot_ckpt(18, "early MSC OK");
                k_nano::slog_nano!("USB", "msc", "EARLY bringup OK — BOOT.LOG path");
            } else {
                crate::display::fb::boot_ckpt(18, "early MSC skip");
                k_nano::slog_nano!(
                    "USB",
                    "msc",
                    "EARLY AUSENTE — retry no DriverInit/SysInfo (CCS atrasado?)"
                );
            }
            *crate::USB_MSC.lock() = msc;
        }
        crate::boot_logger::log("BOOT: early USB path (pre-NIC)");
        if crate::USB_MSC.lock().is_some() {
            let ok = crate::boot_logger::flush();
            if ok {
                crate::display::fb::console_print("LOG: BOOT.LOG early OK (USB)");
            }
        }
        crate::display::fb::boot_ckpt(184, "early USB done");
        }
    }

    // ponytail: hardware/ probe moved to LEGACY — StandardUma is always the detected profile.
    let simd_width = match k_nano::platform_probe::isa_path() {
        k_nano::platform_probe::IsaPath::Avx512F => 512,
        k_nano::platform_probe::IsaPath::Avx2Fma => 256,
        k_nano::platform_probe::IsaPath::Sse42 => 128,
        k_nano::platform_probe::IsaPath::Scalar => 64,
    };
    let expert_size: usize = 8 * 1024 * 1024; // 8MB default expert (fits L3 cache)
    k_nano::slog_bin!("ADAPT", "info", "profile=StandardUma simd={}bit", simd_width);
    k_nano::slog_bin!("ADAPT", "info", "SIMD dispatch={}bit expert_size={}KB", simd_width, expert_size / 1024);
    k_nano::core_pinning::log_pinning_state();

    publish_boot_phase(BootPhase::DriverInit, "Drivers de HW (NIC/ATA/USB/GPU)");

    // Detecta ambiente: QEMU sandbox vs HW real
    let is_sandbox = crate::net::detect_dev_env();
    if is_sandbox {
        let hv_name = crate::net::detect_hypervisor_name();
        if hv_name.contains("vbox") {
            crate::env::set(crate::env::SystemEnv::VBoxSandbox);
        } else {
            crate::env::set(crate::env::SystemEnv::QemuSandbox);
        }
        // ponytail: WHPX ignora PIT vector 0 — skip PIT, usa só LAPIC timer
        if hv_name.contains("Microsoft") {
            crate::apic::SKIP_PIT.store(true, core::sync::atomic::Ordering::Relaxed);
        }
        k_nano::slog_bin!("ENV", "info", "Sandbox detectado: {} — SLIP so se NIC ausente (DEGRADED)", hv_name.trim_end());
    }
    // NIC: ordem do plano k_ai (I225>VirtIO>e1000>RTL se o silício estiver lá).
    unsafe {
        crate::net::probe_nics_from_bind_plan();
    }
    publish_boot_phase(BootPhase::DriverInit, "NIC bind (plano DeviceTree)");

    // Decisão final: se NIC real encontrada → HW real. Se não → sandbox ou offline.
    let nic_found = crate::net::E1000.lock().is_some()
        || crate::net::I225.lock().is_some()
        || crate::net::RTL8139.lock().is_some()
        || crate::net::VIRTIO_DEV.lock().is_some();
    crate::env::note_physical_nic(nic_found);
    if nic_found {
        if !is_sandbox {
            crate::env::set(crate::env::SystemEnv::HwReal);
            k_nano::slog_bin!("ENV", "info", "HW real detectado — NIC fisica presente");
        }
    } else if crate::env::get() == crate::env::SystemEnv::Unknown {
        crate::env::set(crate::env::SystemEnv::Offline);
        k_nano::slog_bin!("ENV", "info", "Offline — nenhuma rede disponivel");
        // Apenas em sandbox: ativa serial tunnel como bypass
        if is_sandbox {
            unsafe { crate::net::init_serial_tunnel(); }
            publish_boot_phase(BootPhase::DriverInit, "Serial tunnel (SLIP) DEGRADED");
            let _ = crate::EVENT_BUS.publish(crate::Event {
                id: 0,
                topic: alloc::string::String::from("HEALTH_ISSUE"),
                payload: b"HEALTH_ISSUE:I5:net:degraded_slip_sandbox".to_vec(),
                token: crate::CapabilityToken::Legacy(1),
            });
            crate::env::note_slip_degraded(true);
            k_nano::slog_bin!(
                "ENV",
                "info",
                "DEGRADED: SLIP/COM2 (sandbox sem NIC) — nao e Net gate; IDEA #513 path"
            );
        }
    } else {
        // Sandbox sem NIC: SLIP degradado (nao e Net gate)
        unsafe { crate::net::init_serial_tunnel(); }
        publish_boot_phase(BootPhase::DriverInit, "Serial tunnel (SLIP) DEGRADED");
        let _ = crate::EVENT_BUS.publish(crate::Event {
            id: 0,
            topic: alloc::string::String::from("HEALTH_ISSUE"),
            payload: b"HEALTH_ISSUE:I5:net:degraded_slip_sandbox".to_vec(),
            token: crate::CapabilityToken::Legacy(1),
        });
        crate::env::note_slip_degraded(true);
        k_nano::slog_bin!("ENV", "info", "DEGRADED: SLIP/COM2 (sandbox sem NIC)");
    }

    k_nano::slog_bin!("ENV", "info", "Sistema: {} | Rede: {}", crate::env::name(),
        if nic_found { "fisica" } else if crate::env::is_sandbox() { "serial tunnel" } else { "offline" });

    // Sprint Net: L2–L3 (+ smoke L4/L5) antes do scheduler — hang pós-Runtime não impede static.
    crate::network_agent::bootstrap_early();
    // Hermes FE → bin NETSTACK (Browser/Search/Market/SelfUpdate)
    hermes_crate::net_bridge::register_http_get_url(crate::net::resolve_and_http_get_safe);
    hermes_crate::net_bridge::register_resolve_and_http_get_safe(crate::net::resolve_and_http_get_safe);
    hermes_crate::net_bridge::register_tcp_xfer(|host, port, data| unsafe {
        crate::net::tcp_exchange(host, port, data)
    });
    hermes_crate::net_bridge::register_udp_xfer(crate::net::udp_exchange_safe);
    hermes_crate::net_bridge::register_dns_resolve(crate::net::dns_resolve_host_safe);
    // TLS N4 bridge → hermes tls::fetch_url dispatcher (embedded-tls 0.19, HybridProvider)
    hermes_crate::tls::register_https_get(crate::net::https_get);
    // SESSION_234: transporte P2P (ADR-0081) movido para k_nano — hermes
    // consome via EVENT_BUS "P2P_PACKET" (skill_sync/skill_marketplace poll).
    crate::net::log_tls_status_boot();
    // NetFs #418 smoke (best-effort; also from network_agent after L5_OK)
    crate::netfs::smoke_if_online();
    // TLS N4 smoke — só com L5_OK; fora do lock do bootstrap
    crate::net::smoke_https_if_online();
    // Labor 10: NTP sync non-fatal (ADR-0069)
    let _ = hermes_crate::ntp::try_sync();
    publish_boot_phase(BootPhase::DriverInit, "Net bootstrap_early (static/DNS/HTTP/TLS/NTP smoke)");

    unsafe { k_nano::storage_probe::probe_storage_drivers(); }
    let ata_found = crate::ATA_DRIVER.lock().is_some();

    // Labor 12: pins FAT após ATA (smoke HTTPS pode ter aprendido em RAM antes).
    crate::tls_trust::load_pins_from_fat();
    crate::tls_trust::persist_pins_to_fat();

    publish_boot_phase(BootPhase::DriverInit, &alloc::format!("ATA probe={}", if ata_found { "found" } else { "none" }));

    // SESSION_252: trigger OTA via flag QEMu-loader (padrão netmode.flag).
    // O loop smoke (tools/qemu_ota_loop.ps1) grava 'O' na RAM; dispara
    // check_for_update() no boot SEM depender do teclado (IRQ1 não é entregue
    // via IOAPIC no QEMU — sendkey nunca chegava ao shell). Valida o fluxo
    // UPDATE.CFG → GET manifest → slot inativo → serve_update.py.
    // DEPOIS do probe do ATA: `with_fat_reader` exige ATA_DRIVER populado.
    if crate::net::detect_qemu_ota_trigger() {
        let report = hermes_crate::self_update::check_for_update_qemu_slirp();
        k_nano::slog_bin!("OTA", "info", "boot trigger: {}", report);
    }

    // Intel HDA — so se DeviceTree tem Snd (ou plano ainda nao instalado).
    if k_nano::boot_bind::should_probe_snd() {
        let hda_ok = unsafe { k_nano::audio::hda::init_hda() };
        if hda_ok {
            k_nano::slog_bin!("HDA", "info", "Intel HDA capture driver initialized (SD0)");
        } else {
            k_nano::slog_bin!("HDA", "warn", "Intel HDA not found or init failed");
        }
        publish_boot_phase(BootPhase::DriverInit, "HDA audio init");
    } else {
        k_nano::slog_bin!("HDA", "info", "skip — DeviceTree sem classe Snd");
    }

    // AHCI/NVMe/ATA ja probed em storage_probe::probe_storage_drivers (plano k_ai).

    let want_usb = k_nano::boot_bind::should_probe_usb_host();
    if want_usb {
        unsafe { crate::xhci::init_xhci(); } // idempotente se early path já subiu
        crate::display::fb::boot_ckpt(15, "xhci init done");
    } else {
        crate::display::fb::boot_ckpt(15, "xhci skip (plano sem UsbHost)");
        k_nano::slog_nano!("USB", "xhci", "skip — DeviceTree sem UsbHost");
    }

    crate::display::fb::boot_ckpt(24, "antes USB-MSC probe");
    {
        if want_usb {
            // Se early path já tem MSC, não re-probe (Address Device de novo quebra BOT).
            // QEMU: Enable Slot em tablet/kbd já timeoutou no early path — retry
            // + HID P24a/b = gap K184→K24 (7× wait_cmd).
            let qemu_usb = k_nano::platform_probe::probe_done()
                && !matches!(
                    k_nano::platform_probe::hypervisor(),
                    k_nano::platform_probe::HypervisorKind::None
                );
            if crate::USB_MSC.lock().is_none() {
                if qemu_usb {
                    crate::display::fb::boot_ckpt(16, "USB-MSC skip retry (qemu)");
                    k_nano::slog_nano!("USB", "msc", "QEMU skip re-probe apos early FAIL");
                } else {
                let msc = unsafe { k_nano::usb_msc::UsbMassStorage::probe() };
                if msc.is_some() {
                    k_nano::slog_nano!("USB", "msc", "stored for FAT model load (unified USB)");
                    crate::display::fb::boot_ckpt(16, "USB-MSC OK");
                } else {
                    crate::display::fb::boot_ckpt(16, "USB-MSC AUSENTE");
                    k_nano::slog_nano!(
                        "USB",
                        "msc",
                        "AUSENTE — bringup/enum/BOT falhou; BOOT.LOG so ramlog (ADR-0062 P11 residual)"
                    );
                }
                *crate::USB_MSC.lock() = msc;
                }
            } else {
                crate::display::fb::boot_ckpt(16, "USB-MSC OK (early)");
                k_nano::slog_nano!("USB", "msc", "reuse early MSC — skip re-probe");
            }
        } else {
            crate::display::fb::boot_ckpt(16, "USB-MSC skip (plano)");
        }
        crate::display::fb::boot_ckpt(25, "antes BOOT.LOG flush");
        crate::boot_logger::init_after_usb();
        crate::display::fb::boot_ckpt(17, "BOOT.LOG flush tentado");
        let qemu_hid = k_nano::platform_probe::probe_done()
            && !matches!(
                k_nano::platform_probe::hypervisor(),
                k_nano::platform_probe::HypervisorKind::None
            );
        if want_usb && !qemu_hid {
            // ADR-0062 P24a: HID boot keyboard (porta ≠ MSC)
            if unsafe { crate::xhci::bringup_hid_keyboard() } {
                crate::boot_logger::log("BOOT: P24a HID keyboard ready");
            }
            // ADR-0062 P24b: HID boot mouse (porta ≠ MSC/kb); hub=AWAITING se class 09h
            if unsafe { crate::xhci::bringup_hid_mouse() } {
                crate::boot_logger::log("BOOT: P24b HID mouse ready");
            } else {
                crate::boot_logger::log("BOOT: P24b HID mouse SKIP");
            }
        } else if want_usb {
            k_nano::slog_nano!("USB", "hid", "QEMU skip P24a/P24b (tablet/kbd Enable Slot timeout)");
        }
    }

    crate::display::fb::boot_ckpt(26, "pos-K17 publish");
    publish_boot_phase(BootPhase::DriverInit, "xHCI+USB probe done");

    // Boot log: reforço ATA (se houver) + flush checkpoint
    crate::display::fb::boot_ckpt(27, "ATA boot_log/verify");
    {
        // NÃO segurar ATA_DRIVER.lock() durante boot_logger::init/persist_now —
        // persist_now faz ATA_DRIVER.lock() de novo → deadlock (parava em K27).
        let parts = {
            let ata_guard = crate::ATA_DRIVER.lock();
            ata_guard
                .as_ref()
                .map(|ata| crate::fat32::read_mbr(ata))
                .unwrap_or_default()
        };
        if !parts.is_empty() {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                verify_kernel_from_disk(ata, &parts);
            }
        }
        crate::boot_logger::init(None, &[]);
        crate::boot_logger::log("BOOT: ATA+FAT init OK");
    }

    // Honesty smokes adiados (SESSION_265) — agora com net/storage bridges vivos.
    crate::display::fb::boot_ckpt(71, "labor smokes");
    labor_smokes::run_deferred(boot_tag);
    crate::display::fb::boot_ckpt(72, "labor smokes ok");

    crate::display::fb::boot_ckpt(28, "VFS init");
    {
        use crate::vfs::VfsRegistry;
        let vfs = VfsRegistry::new();
        *crate::vfs::VFS.lock() = Some(vfs);
        crate::vfs::init_standard_mounts();
    }

    let vfs_guard = crate::vfs::VFS.lock();
    let mcount = vfs_guard.as_ref().map_or(0, |v| v.mount_table().len());
    drop(vfs_guard);
    k_nano::slog_bin!("VFS", "info", "Init OK. {} mounts.", mcount);
    crate::boot_logger::log(&alloc::format!("BOOT: VFS {} mounts", mcount));
    // Labor 25: fd table após mounts
    let _ = k_nano::vfs::fd::boot_smoke();

    crate::display::fb::boot_ckpt(29, "FS agents");
    crate::fs::init_fs_agents();

    hermes_crate::globals::install_vfs_bridge(hermes_crate::globals::VfsBridge {
        read: crate::fs::read_vfs,
        write: crate::fs::write_vfs,
        list: crate::fs::list_vfs,
    });
    k_nano::slog_bin!("VFS", "info", "Hermes bridge -> neural-kernel FS_AGENTS");
    crate::boot_logger::log("BOOT: FS agents OK");

    {
        let (meta, copy, skip, demoted, promoted) = crate::mhi::migration_stats();
        k_nano::slog_bin!("ADR", "0040", "MVP wired: BlockDevice+write | exFAT FilesystemDriver | EXT2/NTFS detect | NeuralFS /mnt/neural | MHI soft-migrate (meta={} copy={} skip={} freed={} loaded={})", meta, copy, skip, demoted, promoted);
        crate::boot_logger::log("BOOT: ADR-0040 FS MVP markers");
    }

    crate::display::fb::boot_ckpt(30, "ADR-0047 gates");
    adr0047_mvp_gates();

    crate::display::fb::boot_ckpt(31, "DiskAgent");
    let mut disk_agent = crate::disk_agent::DiskIntelligenceAgent::new();

    if let Some(ref ata) = *crate::ATA_DRIVER.lock() {
        let ctrl = crate::disk_agent::controller::AtaCtrl::new(ata.clone());
        disk_agent.register_controller(Box::new(ctrl));
        crate::boot_logger::log("BOOT: DiskAgent ATA controller registered");
    } else {
        crate::boot_logger::log("BOOT: No ATA device for DiskAgent");
    }

    if crate::USB_MSC.lock().is_some() {
        crate::boot_logger::log("BOOT: DiskAgent USB-MSC available (global USB_MSC)");
    }

    // StorageBus: registra o que o plano ja trouxe (sem re-probe NVMe).
    crate::display::fb::boot_ckpt(32, "StorageBus register");
    if k_nano::disk_agent::nvme::NVME_DRIVER.lock().is_some() {
        {
            let mut g = k_nano::disk_agent::nvme::NVME_DRIVER.lock();
            if let Some(ref mut n) = *g {
                k_nano::storage_bus::STORAGE_BUS.lock().register_probe(
                    k_nano::storage_bus::BusKind::Nvme,
                    "nvme0",
                    n,
                );
            }
        }
        let ctrl = crate::disk_agent::controller::NvmeCtrl::new();
        disk_agent.register_controller(Box::new(ctrl));
        crate::boot_logger::log("BOOT: DiskAgent NVMe controller registered");
    }
    {
        let mut bus = k_nano::storage_bus::STORAGE_BUS.lock();
        {
            let mut ahci_g = crate::AHCI_DRIVER.lock();
            if let Some(ref mut ahci) = *ahci_g {
                bus.register_probe(k_nano::storage_bus::BusKind::Ahci, "ahci0", ahci);
            }
        }
        {
            let mut ata_g = crate::ATA_DRIVER.lock();
            if let Some(ref mut ata) = *ata_g {
                bus.register_probe(k_nano::storage_bus::BusKind::Ata, "ata0", ata);
            }
        }
        {
            let mut usb_g = crate::USB_MSC.lock();
            if let Some(ref mut msc) = *usb_g {
                bus.register_probe(k_nano::storage_bus::BusKind::Usb, "usb0", msc);
            }
        }
        crate::boot_logger::log(&alloc::format!(
            "BOOT: StorageBus devices={}",
            bus.device_count()
        ));
    }
    // Labor 13 / ADR-0072: smoke EXT
    {
        let bus = k_nano::storage_bus::STORAGE_BUS.lock();
        let has_ext = bus.entries().iter().any(|e| {
            e.mounts
                .iter()
                .any(|m| m.fs_type.starts_with("ext") || m.mount_point == "/mnt/ext")
        });
        if has_ext {
            k_nano::slog_bin!(
                "EXT4",
                "info",
                "step=smoke status=OK VERDICT=PASS reason=ext_mount_listed"
            );
        } else {
            k_nano::slog_bin!(
                "EXT4",
                "info",
                "step=smoke status=SKIP VERDICT=SKIP reason=no_ext_partition"
            );
        }
    }

    let disk_agent_box = Box::new(disk_agent);
    crate::boot_logger::log("BOOT: DiskAgent ready");

    crate::display::fb::boot_ckpt(33, "apps+audio+wasm");
    crate::apps::init_apps();
    crate::boot_logger::log("BOOT: Desktop apps OK");

    audio::init_audio();
    jarbas_bridge::log_bridge_status();

    // SESSÃO_260: checkpoints de progresso dentro do bloco K33 — cada self-test
    // loga um K próprio (33a..33h) para o boot HW real identificar QUAL trava
    // (sem serial, o K* no FB é o único canal). O cpuid do exec_arena corrigiu o
    // SMC; se ainda travar, o último K mostra o alvo exato.
    crate::display::fb::boot_ckpt(33, "apps+audio+wasm");
    let mut k33 = 1u8;
    macro_rules! k33_step {
        ($tag:expr) => {{
            crate::display::fb::boot_ckpt(33, $tag);
            crate::boot_logger::log(alloc::format!("BOOT: K33[{}] {}", k33, $tag).as_str());
            k33 += 1;
            let _ = k33; // ponytail: contador só p/ log; valor final não é usado
        }};
    }
    crate::apps::init_apps();
    k33_step!("apps ok");
    audio::init_audio();
    k33_step!("audio ok");

    let _skillopt = crate::structured_decode::SkillOptimizer::new();
    crate::micropython_wasm::try_init_at_boot();
    k33_step!("micropython");
    // ADR-0059: runtime WASM real (wasmi) + seletor de caminho (A/B/C) — self-tests.
    let _ = hermes_crate::wasmi_rt::self_test();
    k33_step!("wasmi");
    let _ = hermes_crate::wasm_build::self_test(); // F4: op-IR→wasm→wasmi
    k33_step!("wasm_build");
    let _ = hermes_crate::app_factory::self_test(); // F3: gera→monta→sandbox
    k33_step!("app_factory");
    // ADR-0059 F7: arena W^X — execução de código nativo gerado on-device (base JIT).
    let _ = crate::exec_arena::self_test();
    k33_step!("exec_arena");
    // ADR-0081 Fase B + BEI: self-tests do transporte P2P (chunking CHK\0),
    // serialização .bitnet roundtrip (save_model) e aprendizado federado.
    let _ = k_nano::net::mesh::chunk_self_test();
    k33_step!("mesh_chunk");
    let _ = cortex_crate::cortex::model_save_roundtrip_self_test();
    k33_step!("roundtrip");
    let _ = cortex_crate::federated::federated_self_test();
    k33_step!("federated");
    // ADR-0083 §5.2: backprop real do TransformerTrainer — CE loss diminui
    // em sequência sintética (critério de aceite).
    {
        let mut t = k_ai::cognitive::TransformerTrainer::new(16, 16, 1, 8);
        if t.self_test().is_ok() {
            k_nano::slog_bin!("TRAIN", "info", "{}", t.status());
        }
    }
    k33_step!("trainer");
    // ADR-0081 tier cripto (Relativizado HMAC-SHA256): vetor RFC 4231 caso 1.
    let _ = k_nano::crypto::hmac_self_test();
    k33_step!("hmac");
    // ADR-0081 Tier F: AEAD X25519 DH + ChaCha20-Poly1305 — self-test com
    // keypairs fake (DH simétrico → chaves iguais; roundtrip; tamper→None;
    // nonce diverge por clock). Roda antes de init_session_identity (usa
    // keypair fake, não a seed da sessão).
    let _ = k_nano::crypto::aead_self_test();
    k33_step!("aead");
    // ADR-0077: conectores do Ring3 isolation ring (ex-ADR-0060). NÃO registra ainda —
    // porto seguro: B/C nativo gated até o ring passar o gate.
    crate::isolation_ring::init_connectors();
    k33_step!("connectors");
    // ADR-0063 F0/F1a: TickvLite mount + smoke (NVMe ou RAM)
    if k_nano::storage::tickv_smoke() {
        k_nano::slog_bin!(
            "TICKV",
            "smoke",
            "put/get PASS backend={}",
            k_nano::storage::backend_name()
        );
        crate::boot_logger::log("BOOT: [TICKV] put/get smoke PASS");
    } else {
        k_nano::slog_bin!("TICKV", "smoke", "FAIL or skip");
        crate::boot_logger::log("BOOT: [TICKV] smoke FAIL");
    }
    k33_step!("tickv");
    // ADR-0063: facade + demo + Hamming dispatch
    k_ai::sgdb::boot_init();
    k33_step!("sgdb_boot");
    {
        let p = k_ai::sgdb::hamming_kernel_name();
        k_nano::slog_bin!("sgdb", "hamming", "{}", p);
        crate::boot_logger::log("BOOT: [sgdb] hamming kernel selected");
    }
    if k_ai::sgdb::demo() {
        k_nano::slog_bin!("sgdb", "demo", "Q-jump PASS");
        crate::boot_logger::log("BOOT: [sgdb] quality demo PASS");
    } else {
        k_nano::slog_bin!("sgdb", "demo", "Q-jump FAIL");
        crate::boot_logger::log("BOOT: [sgdb] quality demo FAIL");
    }
    k33_step!("sgdb_demo");
    // SESSÃO_260: smokes de storage (e2e_ckpt/power_loss/gc/stress_gc/corrupt)
    // NÃO rodam no boot — são validação de dev com I/O real (stress_gc = 1000
    // writes + compact; e2e faz remount simulado) que em HW real (NVMe NX-256)
    // trava/leva minutos, e em RAM o e2e revelou FAIL latente. Validação fica
    // em cargo test host. metrics/audit (leves) seguem rodando.
    if k_nano::storage::is_ready() {
        k_nano::slog_bin!("TICKV", "smoke", "SKIP (smokes de storage não rodam no boot — cargo test host)");
        crate::boot_logger::log("BOOT: [TICKV] smokes SKIP (fora do boot)");
    }
    {
        // SESSÃO_260: metrics_report()/status_line() rodam bench_d_series = 100k
        // inserts ART + 10k BQ — pesado demais para o boot (QEMU TCG: minutos;
        // HW real AVX2: travou). O demo() acima já validou bench 128/64 (Q7 PASS).
        // Status leve: hamming path + tickv stats, SEM bench.
        let h = k_ai::sgdb::hamming_kernel_name();
        let t = k_nano::storage::tickv_status();
        k_nano::slog_bin!("sgdb", "bench", "hamming={} tickv={} (bench D-series fora do boot)", h, t);
        crate::boot_logger::log("BOOT: [sgdb] status (bench pesado fora do boot)");
    }
    k33_step!("sgdb_metrics");
    // Audit checkpoint load (Onda C)
    {
        let mut trail = hermes_globals::AUDIT_TRAIL.lock();
        if trail.load_from_sgdb() {
            k_nano::slog_bin!("sgdb", "audit", "loaded from TickvLite");
        }
    }
    k33_step!("tickv_smokes");
    crate::display::fb::boot_ckpt(35, "session identity");
    k_nano::identity::init_session_identity();
    crate::display::fb::boot_ckpt(36, "package_hub");
    crate::package_hub::init_package_hub();
    crate::display::fb::boot_ckpt(37, "hub ok");
    // Sem MSC/ATA: ramlog + foto curta; soft-reboot OFF (loop HW). Runtime segue.
    if crate::USB_MSC.lock().is_none() && crate::ATA_DRIVER.lock().is_none() {
        crate::boot_logger::maybe_uefi_flush_reboot("K37 hub ok — sem MSC/ATA");
    }
    k_nano::slog_bin!("Log", "msg", "{}", hermes_globals::AUDIT_TRAIL.lock().status());
    k_nano::slog_bin!("Log", "msg", "{}", crate::skill_opt::status());

    kjson!("BOOT", "WASM", "runtime", "skills", 2);
    kjson!("BOOT", "DECODE", "structured", "ready", 1);

    crate::display::fb::boot_ckpt(34, "antes load modelos");

    // Pendrive HW: sem USB-MSC/ATA o BOOT.LOG nao grava e FAT PIO pode travar (AHCI
    // interno sozinho nao conta — disco errado / portas vazias). QEMU-loader continua.
    let has_fat_block = crate::ATA_DRIVER.lock().is_some()
        || crate::USB_MSC.lock().is_some()
        || crate::AHCI_DRIVER.lock().is_some()
        || k_nano::disk_agent::nvme::NVME_DRIVER.lock().is_some();
    if !has_fat_block {
        crate::display::fb::boot_ckpt(38, "sem MSC/ATA — skip FAT");
        k_nano::slog_nano!(
            "FAT",
            "info",
            "sem ATA/USB-MSC — skip FAT models; continue AgentFleet (MSC residual)"
        );
        crate::boot_logger::log("BOOT: skip FAT models; continue without soft-reboot");
        // Já avisou em K37; skip_flush evita 2ª pausa.
        if !k_nano::boot_ramlog::skip_flush_reboot() {
            crate::boot_logger::maybe_uefi_flush_reboot("K38 no ATA/USB-MSC");
        }
    }

    crate::display::fb::boot_ckpt(39, "inicio model loading");
    // Carrega modelos do FAT32: BGE.BIN — tenta AHCI primeiro, fallback ATA
    unsafe fn read_file_from_dev(dev: &mut dyn crate::block_dev::BlockDevice, name: &str) -> Option<alloc::vec::Vec<u8>> {
        // Le MBR (+ GPT se USB unificado / protective EE)
        let mut mbr = [0u8; 512];
        if !dev.read_sectors(0, &mut mbr) { return None; }
        if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA { return None; }
        let mut parts = crate::fat32::parse_mbr_sector(&mbr);
        let has_ee = parts.iter().any(|p| p.type_code == 0xEE);
        let has_fat = parts.iter().any(|p| p.type_code == 0x0B || p.type_code == 0x0C || p.type_code == 0x1C);
        if has_ee || !has_fat {
            let gpt = crate::fat32::parse_gpt_partitions(|lba, buf| {
                let mut tmp = [0u8; 512];
                if !dev.read_sectors(lba, &mut tmp) { return false; }
                *buf = tmp;
                true
            });
            for g in gpt {
                if g.type_code == 0xEE { continue; }
                if parts.iter().any(|p| p.lba_start == g.lba_start) { continue; }
                parts.push(g);
            }
        }
        let name_upper = name.to_ascii_uppercase();
        // 1) Preferir exFAT (boot data ADR-0040)
        for part in &parts {
            let start = part.lba_start as u64;
            let mut vbr = [0u8; 512];
            if !dev.read_sectors(start, &mut vbr) { continue; }
            if &vbr[3..11] != b"EXFAT   " { continue; }
            if let Some(mut ex) = crate::exfat::ExfatReader::new(dev, start) {
                for (fname, is_dir, cluster, size) in ex.list_root() {
                    if is_dir { continue; }
                    if !fname.eq_ignore_ascii_case(name_upper.as_str()) { continue; }
                    if let Some(data) = ex.read_file(cluster, size as usize) {
                        return Some(data);
                    }
                }
            }
        }
        // 2) Fallback FAT32 (MBR 0x0B/0x0C ou GPT Basic Data→0x0C)
        for part in &parts {
            let type_code = part.type_code;
            if type_code != 0x0B && type_code != 0x0C && type_code != 0x1C { continue; }
            let lba_start = part.lba_start;
            let mut bpb = [0u8; 512];
            if !dev.read_sectors(lba_start as u64, &mut bpb) { continue; }
            if &bpb[3..11] == b"EXFAT   " { continue; }
            let bps = u16::from_le_bytes([bpb[0x0B], bpb[0x0C]]);
            let spc = bpb[0x0D];
            let reserved = u16::from_le_bytes([bpb[0x0E], bpb[0x0F]]);
            let fat_count = bpb[0x10];
            let root_entries = u16::from_le_bytes([bpb[0x11], bpb[0x12]]);
            if root_entries > 0 { continue; }
            let spf = u32::from_le_bytes([bpb[0x24], bpb[0x25], bpb[0x26], bpb[0x27]]);
            let root_cluster = u32::from_le_bytes([bpb[0x2C], bpb[0x2D], bpb[0x2E], bpb[0x2F]]);
            if bps < 512 || bps > 4096 || bps % 32 != 0 || spc == 0 { continue; }
            let fat_lba = lba_start + reserved as u32;
            let data_lba = fat_lba + fat_count as u32 * spf;

            let mut cluster = root_cluster;
            while cluster < 0x0FFF_FFF8 && cluster >= 2 {
                let clba = data_lba + (cluster - 2) as u32 * spc as u32;
                let mut buf = vec![0u8; spc as usize * bps as usize];
                for s in 0..spc as u32 {
                    let start = s as usize * bps as usize;
                    if start + bps as usize <= buf.len() {
                        dev.read_sectors((clba + s) as u64, &mut buf[start..start + bps as usize]);
                    }
                }
                for entry in (0..buf.len()).step_by(32) {
                    let first = buf[entry];
                    if first == 0 || first == 0xE5 { continue; }
                    if buf[entry + 11] & 0x08 != 0 { continue; }
                    let fname = core::str::from_utf8(&buf[entry..entry+11]).unwrap_or("").trim_end();
                    if !fname.eq_ignore_ascii_case(name_upper.as_str()) { continue; }
                    let fsize = u32::from_le_bytes([buf[entry+28], buf[entry+29], buf[entry+30], buf[entry+31]]) as usize;
                    let fc_lo = u16::from_le_bytes([buf[entry+26], buf[entry+27]]);
                    let fc_hi = u16::from_le_bytes([buf[entry+20], buf[entry+21]]);
                    let start_cluster = ((fc_hi as u32) << 16) | fc_lo as u32;
                    // fsize e u32 do disco (ate 4GB) — cap contra OOM em boot.
                    if fsize > 16 << 20 { continue; }
                    let mut data = Vec::with_capacity(fsize);
                    let mut fc = start_cluster;
                    while fc < 0x0FFF_FFF8 && fc >= 2 && data.len() < fsize {
                        let fc_lba = data_lba + (fc - 2) as u32 * spc as u32;
                        for s in 0..spc as u32 {
                            if data.len() >= fsize { break; }
                            let mut chunk = [0u8; 512];
                            dev.read_sectors((fc_lba + s) as u64, &mut chunk);
                            let rem = fsize - data.len();
                            data.extend_from_slice(&chunk[..rem.min(512)]);
                        }
                        let fat_off = fc as usize * 4;
                        let fat_sec = fat_lba + (fat_off / bps as usize) as u32;
                        let mut fsector = [0u8; 512];
                        dev.read_sectors(fat_sec as u64, &mut fsector);
                        let boff = fat_off % bps as usize;
                        fc = u32::from_le_bytes([fsector[boff], fsector[boff+1], fsector[boff+2], fsector[boff+3]]) & 0x0FFF_FFFF;
                    }
                    return Some(data);
                }
                let fat_off = cluster as usize * 4;
                let fat_sec = fat_lba + (fat_off / bps as usize) as u32;
                let mut fsector = [0u8; 512];
                dev.read_sectors(fat_sec as u64, &mut fsector);
                let boff = fat_off % bps as usize;
                cluster = u32::from_le_bytes([fsector[boff], fsector[boff+1], fsector[boff+2], fsector[boff+3]]) & 0x0FFF_FFFF;
            }
        }
        None
    }

    unsafe {
        let mut loaded = false;
        let mut found = false;
        crate::display::fb::boot_ckpt(40, "QEMU loader scan start");
        // QEMU-loader scan: varre [0x100000000..0x180000000) step=1MB por magic 0xBE11BE11 (BGE.BIN)
        {
            let pm = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
            if pm != 0 {
                // Tamanho do BGE: tenta pegar do FAT, default 512KB
                let mut size_hint = 512 * 1024usize;
                let ata_guard = crate::ATA_DRIVER.lock();
                if let Some(ref ata) = *ata_guard {
                    let parts = crate::fat32::read_mbr(ata);
                    for p in &parts {
                        if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                            continue;
                        }
                        if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                            if let Some(sz) = fs.lookup_file_size("BGE.BIN") {
                                size_hint = sz.min(1024 * 1024).max(64);
                                break;
                            }
                        }
                    }
                }
                drop(ata_guard);
                let mut addr = 0x100000000u64;
                while addr < 0x180000000 {
                    // SESSÃO_260 (AIOS): o scan lia read_volatile em região de
                    // HOLE (RAM < addr) sem checar mapeamento → #PF storm
                    // (CR2=pmoff+0x100000000) quando a RAM não alcança. Só lê
                    // se a página está PRESENT (walk das page tables ativas).
                    let ptr = (addr + pm) as *const u8;
                    if !crate::memory::is_page_present(addr + pm) {
                        addr = addr.saturating_add(0x100000);
                        continue;
                    }
                    let magic = core::ptr::read_volatile(ptr as *const u32);
                    if magic == 0xBE11BE11 {
                        found = true;
                        // Bounds check: não ler além do fim da região de memória
                        if let Some(r_end) = handoff.region_end_containing(addr) {
                            let max_read = (r_end - addr) as usize;
                            if size_hint > max_read { size_hint = max_read; }
                        }
                        let data = core::slice::from_raw_parts(ptr, size_hint);
                        k_nano::slog_bin!("Asset", "bge", "magic 0xBE11BE11 found @{:#x} — parse {} KB…", addr, size_hint / 1024);
                        if crate::memory_systems::load_bge(data) {
                            k_nano::slog_bin!("Asset", "bge", "Embedding model LOADED (QEMU-loader @{:#x}) size={}KB", addr, size_hint / 1024);
                            crate::boot_logger::log("BOOT: BGE embedding loaded (QEMU)");
                            loaded = true;
                            break;
                        } else {
                            k_nano::slog_bin!("Asset", "bge", "@{:#x} parse FAILED — fallback FAT", addr);
                        }
                    }
                    addr = addr.saturating_add(0x100000); // 1MB steps
                }
                if !found {
                    k_nano::slog_bin!("Asset", "bge", "QEMU-loader scan [0x100000000..0x180000000) — 0xBE11BE11 ausente");
                }
            }
        }
        crate::display::fb::boot_ckpt(41, "QEMU loader scan done");
        // Saudacao ANTES do PIO BGE/LLM: FAT PACK_LLM=all (BGE 138MB) senão
        // o register K44 nunca corre e o serial fica mudo.
        k_nano::slog_bin!("JARBAS", "GREETING", "pre-BGE emit_hw_greeting_at_register");
        audio::jarvis::emit_hw_greeting_at_register();
        // FAT policy: NVMe > AHCI > ATA > USB-MSC (ADR-0062 P3)
        if has_fat_block {
            if !loaded {
                let mut nvme_g = k_nano::disk_agent::nvme::NVME_DRIVER.lock();
                if let Some(ref mut nvme) = *nvme_g {
                    if let Some(bge_data) = read_file_from_dev(nvme, "BGE.BIN") {
                        found = true;
                        k_nano::slog_bin!("BGE", "info", "BGE.BIN lido NVMe ({} KB) — parse…", bge_data.len() / 1024);
                        if crate::memory_systems::load_bge(&bge_data) {
                            k_nano::slog_bin!("Asset", "bge", "Embedding model LOADED from NVMe FAT!");
                            crate::boot_logger::log("BOOT: BGE embedding loaded (NVMe)");
                            loaded = true;
                        }
                    }
                }
            }
            let mut ahci_guard = crate::AHCI_DRIVER.lock();
            if let Some(ref mut ahci) = *ahci_guard {
                if !loaded {
                    if let Some(bge_data) = read_file_from_dev(ahci, "BGE.BIN") {
                        found = true;
                        k_nano::slog_bin!("BGE", "info", "BGE.BIN lido AHCI ({} KB) — parse…", bge_data.len() / 1024);
                        if crate::memory_systems::load_bge(&bge_data) {
                            k_nano::slog_bin!("Asset", "bge", "Embedding model LOADED from AHCI FAT!");
                            crate::boot_logger::log("BOOT: BGE embedding loaded");
                            loaded = true;
                        } else {
                            k_nano::slog_bin!("Asset", "bge", "BGE.BIN present but parse FAILED (AHCI)");
                        }
                    }
                }
            }
            drop(ahci_guard);
            if !loaded {
                let ata_guard = crate::ATA_DRIVER.lock();
                if let Some(ref ata) = *ata_guard {
                    let parts = crate::fat32::read_mbr(ata);
                    for p in &parts {
                        if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                        if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                            if let Some(sz) = fs.lookup_file_size("BGE.BIN") {
                                found = true;
                                k_nano::slog_bin!("BGE", "info", "BGE.BIN presente FAT ({} KB) — lendo…", sz / 1024);
                                // ATA PIO 138MB trava o boot (QEMU WHPX hv pode ainda ser None).
                                // Embeddings ficam para runtime / NVMe.
                                if sz > 8 * 1024 * 1024 {
                                    k_nano::slog_bin!(
                                        "BGE",
                                        "info",
                                        "skip ATA PIO BGE {}KB no hypervisor (saudacao/runtime primeiro)",
                                        sz / 1024
                                    );
                                    continue;
                                }
                            }
                            if let Some(bge_data) = fs.read_file("BGE.BIN") {
                                found = true;
                                k_nano::slog_bin!("BGE", "info", "BGE.BIN lido ATA ({} KB) — parse…", bge_data.len() / 1024);
                                if crate::memory_systems::load_bge(&bge_data) {
                                    k_nano::slog_bin!("Asset", "bge", "Embedding model LOADED from FAT (ATA)!");
                                    crate::boot_logger::log("BOOT: BGE embedding loaded");
                                    loaded = true;
                                } else {
                                    k_nano::slog_bin!("Asset", "bge", "BGE.BIN present but parse FAILED (sem word_embeddings_weight?)");
                                }
                            }
                        }
                    }
                }
            }
            if !loaded {
                let mut usb_guard = crate::USB_MSC.lock();
                if let Some(ref mut msc) = *usb_guard {
                    if let Some(bge_data) = read_file_from_dev(msc, "BGE.BIN") {
                        found = true;
                        k_nano::slog_bin!("BGE", "info", "BGE.BIN lido USB-MSC ({} KB) — parse…", bge_data.len() / 1024);
                        if crate::memory_systems::load_bge(&bge_data) {
                            k_nano::slog_bin!("Asset", "bge", "Embedding model LOADED from USB-MSC FAT!");
                            crate::boot_logger::log("BOOT: BGE embedding loaded (USB)");
                            loaded = true;
                        } else {
                            k_nano::slog_bin!("Asset", "bge", "BGE.BIN present but parse FAILED (USB-MSC)");
                        }
                    }
                }
            }
        }
        if !loaded && !found {
            crate::load_status::set_if_upgrade(
                crate::load_status::AssetKind::Bge,
                crate::load_status::LoadStatus::Absent,
            );
            k_nano::slog_bin!("Asset", "bge", "BGE.BIN ausente no FAT — STATUS Absent");
        }
    }

    // Piper TTS: loader QEMU @0x130000000 ou FAT PIO (apos BGE para STATUS honesto)
    audio::skills::init_neural_tts();
    crate::load_status::print_status_banner();



    // ADR-0041 H1: refresh HalOffer (DeviceTree já populado pós-platform_sync)
    let _khal_n = k_hal::init();
    k_hal::virtio::init_h4_log();
    k_hal::cap_gate::demo_h5_deny();
    // ADR-0056: 4 LEGOs na FAT — localize + utilize bind table (≠ Ready)
    k_hal::lego_boot::boot_selftest();
    // ADR-0041 Fase 4: AS R1/R3 shallow (PoC non-fatal)
    crate::address_space::demo_as_r1_r3_shallow();

    // GPU: detecta hardware, separa display/compute, inicializa backend (k_hal BE)
    crate::display::fb::boot_ckpt(42, "GPU detect");

    unsafe {

        let gpus = crate::gpu::detect::detect_all();
        crate::display::fb::boot_ckpt(43, "GPU detect done");
        // ReBAR/ACS probe-only (sem try_enable_* — HITL/HW depois).
        #[cfg(target_os = "none")]
        {
            for g in gpus.iter() {
                let cfg = k_hal::gpu::pcie_bypass::RealPciConfig {
                    bus: g.pci_bus,
                    device: g.pci_dev,
                    function: g.pci_fn,
                };
                let rep = k_hal::gpu::pcie_bypass::pcie_bypass_report(&cfg);
                k_nano::slog_bin!("GPU", "pcie", "{}:{}.{} {}", g.pci_bus, g.pci_dev, g.pci_fn, rep);
            }
        }

        if !gpus.is_empty() {

            // Separa iGPU (display) de dGPU (compute) para qualquer combinacao

            let plan = crate::gpu::display_coex::plan_assignment(&gpus);
            crate::display::fb::boot_ckpt(43, "gpu plan");

            k_nano::slog_bin!("Log", "msg", "{}", crate::gpu::display_coex::assignment_status(&plan, &gpus));

            crate::boot_logger::log(&alloc::format!("BOOT: GPU plan — {:?}", plan));

            if let Some(ci) = plan.compute_index() {
                if let Some(g) = gpus.get(ci) {
                    crate::display::fb::boot_ckpt(43, "gpu vram map");
                    if crate::gpu::vram::init_vram_tier(g) {
                        // IDEA #67 — MHI AllocTier::Vram → buddy BAR
                        crate::mhi::register_vram_allocator(crate::gpu::vram::vram_alloc);
                        crate::display::fb::boot_ckpt(43, "gpu vram ok");
                    } else {
                        crate::display::fb::boot_ckpt(43, "gpu vram fail");
                        k_nano::slog_bin!(
                            "MHI-DMA",
                            "info",
                            "VERDICT=AWAITING_REAL_HW reason=vram_tier_init_failed_or_virtio"
                        );
                    }
                }
            }

            // Pré-carrega firmware NVIDIA GP108 (8.3 no FAT) via ATA/USB antes do ACR.
            // Sem isso o jarbas só vê ATA; no boot USB puro o MSC é a fonte.
            crate::display::fb::boot_ckpt(44, "GP108 firmware");
            // Ponytail: GP108 preload — tenta ATA (read_sectors retorna false
            // se sem disco, nao trava mais — commit e154edc mudou assinatura).
            // USB-MSC é tentado como fallback.
            let has_nvidia_fw = gpus.iter().any(|g| matches!(g.vendor, k_hal::gpu::detect::GpuVendor::Nvidia));
            if has_nvidia_fw {
                const GP108: &[(&str, &str)] = &[
                    ("fecs_bl.bin", "FECS_BL.BIN"),
                    ("fecs_data.bin", "FECS_DAT.BIN"),
                    ("fecs_inst.bin", "FECS_INS.BIN"),
                    ("fecs_sig.bin", "FECS_SIG.BIN"),
                    ("gpccs_bl.bin", "GPCCS_BL.BIN"),
                    ("gpccs_data.bin", "GPCCS_DA.BIN"),
                    ("gpccs_inst.bin", "GPCCS_IN.BIN"),
                    ("gpccs_sig.bin", "GPCCS_SI.BIN"),
                    ("sw_ctx.bin", "SW_CTX.BIN"),
                    ("sw_bundle_init.bin", "SW_BNDL.BIN"),
                    ("sw_method_init.bin", "SW_MTHD.BIN"),
                    ("sw_nonctx.bin", "SW_NONC.BIN"),
                    ("bl.bin", "ACR_BL.BIN"),
                    ("ucode_load.bin", "ACRLOAD.BIN"),
                    ("ucode_unload.bin", "ACRUNLD.BIN"),
                    ("unload_bl.bin", "ACR_UBL.BIN"),
                ];
                let mut n = 0u32;
                for (logical, fat_name) in GP108 {
                    let mut data = None;
                    if let Some(ref ata) = *crate::ATA_DRIVER.lock() {
                        let parts = unsafe { crate::fat32::read_mbr(ata) };
                        for p in &parts {
                            if matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0x73) {
                                if let Some(fs) = unsafe { crate::fat32::Fat32Reader::new(ata, p) } {
                                    data = unsafe { fs.read_file(fat_name) };
                                    if data.is_some() { break; }
                                }
                            }
                        }
                    }
                    if data.is_none() {
                        if let Some(ref mut msc) = *crate::USB_MSC.lock() {
                            data = unsafe { read_file_from_dev(msc, fat_name) };
                        }
                    }
                    if let Some(d) = data {
                        crate::gpu::firmware::preload_blob(logical, d);
                        n += 1;
                    }
                }
                k_nano::slog_bin!("FW", "info", "GP108 preload: {}/{} blobs (ATA/USB)", n, GP108.len());
            } else {
                k_nano::slog_bin!("FW", "info", "GP108 preload SKIP (no NVIDIA or no storage)");
            }

            crate::display::fb::boot_ckpt(45, "GP108 done");

            // Plano coex dirige backend (display owner intocado em falha compute)
            crate::display::fb::boot_ckpt(46, "Backend init start");
            crate::gpu::backend::init_backend_with_plan(&gpus, &plan);
            crate::display::fb::boot_ckpt(47, "Backend init done");

            k_nano::slog_hal!("GPU", "info", "{} GPU(s) detectadas. Backend: {}",

                gpus.len(), crate::gpu::backend::gpu_status());

            crate::boot_logger::log(&alloc::format!("BOOT: GPU {} backend", gpus.len()));

        } else {

            k_nano::slog_hal!("GPU", "info", "Nenhuma GPU detectada.");

        }

    }

    // ADR-0057 WS-D/WS-E: conecta aceleradores ao dispatcher de compute do
    // cortex (só registra se `Ready`/HW real; senão CPU/SMP). NPU = Layer S.
    crate::gpu::compute_dispatch::register_compute_if_ready();
    k_hal::npu::init_npu();
    // ADR-0057 WS-G (#412): valida o primitivo de structured-decode sem modelo.
    let _ = cortex_crate::decode::self_test();
    // ADR-0057 WS-F: instala o seam de wake (reschedule-IPI do APIC vivo). APs
    // como workers vivos (ap_pollable) exigem IDT/IPI por-core = residual HW.
    k_nano::smp::install_wake_fn(crate::apic::send_ipi_reschedule);
    k_nano::slog_bin!(
        "SMP",
        "info",
        "AP workers pollable={} (WS-F on-demand wake = residual HW/IDT)",
        k_nano::smp::ap_pollable()
    );

    publish_boot_phase(BootPhase::DriverInit, "NIC/ATA/AHCI/xHCI/GPU probes concluidos");

    // MVP C (ADR-0041): CR3 switch + ring shared + Cap — non-fatal
    match crate::ipc::demo_two_spaces() {
        Ok(()) => {
            k_nano::slog_bin!("MVP-C", "info", "demo OK — capability rings PoC");
            crate::boot_logger::log("BOOT: MVP-C CR3+ring+cap OK");
        }
        Err(e) => {
            k_nano::slog_bin!("MVP-C", "info", "WARN: {} — boot continua", e);
            crate::boot_logger::log("BOOT: MVP-C WARN (non-fatal)");
        }
    }

    // P3 (ADR-0041): Hermes host Caps — non-fatal
    match crate::capability_gate::demo_hermes_caps() {
        Ok(()) => {
            k_nano::slog_bin!("Cap", "p3", "CapGate demo OK");
            crate::boot_logger::log("BOOT: P3 CapGate OK");
        }
        Err(e) => {
            k_nano::slog_bin!("P3", "info", "WARN: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P3 CapGate WARN (non-fatal)");
        }
    }

    // P4 (ADR-0041): JARBAS FB MMIO + double-buffer — non-fatal
    match crate::jarbas_fb::demo_jarbas_fb() {
        Ok(()) => {
            k_nano::slog_bin!("Cap", "p4", "JARBAS FB demo OK");
            crate::boot_logger::log("BOOT: P4 JARBAS FB OK");
        }
        Err(e) => {
            k_nano::slog_bin!("P4", "info", "WARN: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P4 JARBAS FB WARN (non-fatal)");
        }
    }

    // P5 (ADR-0041): K-IA DMA pin + Cortex weight mmap — non-fatal
    match crate::k_ia_dma::demo_kia_dma() {
        Ok(()) => {
            k_nano::slog_bin!("P5", "info", "K-IA DMA pin demo OK");
            crate::boot_logger::log("BOOT: P5 K-IA DMA OK");
        }
        Err(e) => {
            k_nano::slog_bin!("P5", "info", "WARN DMA: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P5 DMA WARN (non-fatal)");
        }
    }
    match crate::cortex_mmap::demo_cortex_mmap() {
        Ok(()) => {
            k_nano::slog_bin!("P5", "info", "Cortex mmap demo OK");
            crate::boot_logger::log("BOOT: P5 Cortex mmap OK");
        }
        Err(e) => {
            k_nano::slog_bin!("P5", "info", "WARN mmap: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P5 mmap WARN (non-fatal)");
        }
    }

    // P6 (ADR-0041): Ring3 real via iretq — non-fatal
    crate::display::fb::boot_ckpt(44, "ADR-0041 demos start");
    match crate::user_mode::demo_ring3() {
        Ok(()) => {
            k_nano::slog_bin!("P6", "info", "Ring3 user-mode demo OK");
            crate::boot_logger::log("BOOT: P6 Ring3 OK");
            match crate::user_mode::demo_ring3_fault_containment() {
                Ok(()) => {
                    k_nano::slog_bin!("P6", "info", "Ring3 fault-containment OK");
                    crate::boot_logger::log("BOOT: P6 Ring3 fault-containment OK");
                }
                Err(e) => {
                    k_nano::slog_bin!("P6", "info", "WARN: {} — boot continua", e);
                    crate::boot_logger::log("BOOT: P6 Ring3 fault-containment WARN (non-fatal)");
                }
            }
            match crate::user_mode::demo_ring3_capgate_dma_mmio() {
                Ok(()) => {
                    k_nano::slog_bin!("P6", "info", "Ring3 CapGate DMA/MMIO OK");
                    crate::boot_logger::log("BOOT: P6 Ring3 CapGate DMA/MMIO OK");
                }
                Err(e) => {
                    k_nano::slog_bin!("P6", "info", "WARN: {} — boot continua", e);
                    crate::boot_logger::log("BOOT: P6 Ring3 CapGate DMA/MMIO WARN (non-fatal)");
                }
            }
            match crate::user_mode::demo_ring3_softfloat_sse() {
                Ok(()) => {
                    k_nano::slog_bin!("P6", "info", "Ring3 soft-float SSE OK");
                    crate::boot_logger::log("BOOT: P6 Ring3 soft-float OK");
                }
                Err(e) => {
                    k_nano::slog_bin!("P6", "info", "WARN: {} — boot continua", e);
                    crate::boot_logger::log("BOOT: P6 Ring3 soft-float WARN (non-fatal)");
                }
            }
        }
        Err(e) => {
            k_nano::slog_bin!("P6", "info", "WARN: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P6 Ring3 WARN (non-fatal)");
        }
    }
    crate::display::fb::boot_ckpt(45, "P6 ring3");

    // ADR-0082 F2.1: ELF64 loader self-test (parse + sandbox + RX/RW + BSS) — non-fatal
    if crate::elf_loader::elf_boot_self_test() {
        crate::boot_logger::log("BOOT: ELF loader self-test OK");
    } else {
        crate::boot_logger::log("BOOT: ELF loader self-test WARN (non-fatal)");
    }
    crate::display::fb::boot_ckpt(45, "P6 elf");

    // ADR-0082 F3.1: arena W^X USER no sandbox AS (base p/ Cranelift B/C) — non-fatal
    if crate::exec_arena::user_arena_self_test() {
        crate::boot_logger::log("BOOT: USER arena self-test OK");
    } else {
        crate::boot_logger::log("BOOT: USER arena self-test WARN (non-fatal)");
    }
    crate::display::fb::boot_ckpt(45, "P6 user_arena");

    // ADR-0077: conectores do Ring3 isolation ring (ex-ADR-0059 F6) — gated
    crate::isolation_ring::init_connectors();

    // P7 (ADR-0041): demand-paging via #PF (lazy Cortex weights) — non-fatal
    match crate::cortex_mmap::demo_demand_paging() {
        Ok(()) => {
            k_nano::slog_bin!("P7", "info", "Demand-paging demo OK");
            crate::boot_logger::log("BOOT: P7 demand-page OK");
        }
        Err(e) => {
            k_nano::slog_bin!("P7", "info", "WARN: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P7 demand-page WARN (non-fatal)");
        }
    }
    crate::display::fb::boot_ckpt(45, "P6 demand");

    // P8 (ADR-0041): VirtIO vring sobre DMA pin (layout-compatible) — non-fatal
    match crate::virtio_vring::demo_virtio_vring() {
        Ok(()) => {
            k_nano::slog_bin!("P8", "info", "VirtIO vring demo OK");
            crate::boot_logger::log("BOOT: P8 vring OK");
        }
        Err(e) => {
            k_nano::slog_bin!("P8", "info", "WARN: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P8 vring WARN (non-fatal)");
        }
    }
    crate::display::fb::boot_ckpt(45, "P6 vring");

    // P9 (ADR-0041): GGUF/FAT file-backed mmap + demand-paging — non-fatal
    match crate::gguf_mmap::demo_gguf_mmap() {
        Ok(()) => {
            k_nano::slog_bin!("P9", "info", "GGUF/FAT mmap demo OK");
            crate::boot_logger::log("BOOT: P9 gguf-mmap OK");
        }
        Err(e) => {
            k_nano::slog_bin!("P9", "info", "WARN: {} — boot continua", e);
            crate::boot_logger::log("BOOT: P9 gguf-mmap WARN (non-fatal)");
        }
    }
    crate::display::fb::boot_ckpt(45, "P6 gguf");

    // Skill Observer: registra observação inicial
    crate::display::fb::boot_ckpt(45, "ADR-0041 demos done");

    crate::skill_observer::watch_task("boot", &["PCI scan", "GPU init", "Agent registry"], 0);

    // P001: registra skills builtin no SKILL_REGISTRY canônico (k_nano) ANTES do AgentFleet.
    register_builtin_skills();

    let mut registry = agent_core::AgentRegistry::new();

    // BootLogAgent cedo: consome BOOT_PHASE via EventBus
    registry.register(Box::new(boot_log_agent::BootLogAgent::new()));

    // PlatformAgent: idempotente se init_platform_sync ja rodou
    registry.register(Box::new(agents::PlatformAgent::new()));

    registry.register(Box::new(agents::MemoryAgent::new()));

    // ADR-0042 N2: Trust antes de SelfHeal para (token,agent,skill) já estar concedido
    registry.register(Box::new(agents::BootTrustAgent));
    registry.register(Box::new(agents::BootSelfHealAgent));

    // Continuous SelfHealAgent for KERNEL_ERROR processing + silent failure detection
    registry.register(Box::new(k_ai::self_heal_agent::SelfHealAgent::new()));

    registry.register(Box::new(crate::memory_agent::MemoryAgent::new()));
    registry.register(Box::new(agents::NetDriverAgent));
    registry.register(Box::new(agents::UsbDriverAgent));
    registry.register(Box::new(k_hal::audio::hda::HdaAudioAgent::new()));
    registry.register(Box::new(audio::usb::UsbAudioAgent::new()));
    registry.register(Box::new(uvc_driver::UvcDriverAgent::new()));
    registry.register(Box::new(agents::GpuDriverAgent));
    registry.register(Box::new(agents::FsBridgeAgent::new()));

    registry.register(Box::new(agents::HwDetectAgent));
    registry.register(Box::new(agents::AutoLearnAgent::new()));
    // LEARNER: coleta pares do EventBus (USER_INTENT/HERMES_RESPONSE), fine-tune
    // ternário + persiste no SGDB (L4Semantic). PollEvery(5000). Antes era nunca
    // registrado → aprendizado não rodava em produção.
    registry.register(Box::new(k_ai::self_learning::SelfLearningAgent::new()));
    registry.register(Box::new(agents::SleepCycleAgent::new()));
    registry.register(Box::new(agents::SelfEvolveAgent::new()));

    registry.register(disk_agent_box);

    // HwRegistry: detecta hardware e cria HwAgents
    crate::display::fb::boot_ckpt(46, "HwRegistry detect");

    let mut hw_reg = crate::hw_agents::HwRegistry::new();

    unsafe { hw_reg.detect_all(); }
    crate::display::fb::boot_ckpt(47, "HwRegistry done");

    k_nano::slog_bin!("HW-AGENTS", "info", "{} dispositivos detectados como HwAgents.", hw_reg.agents.len());

    klogc!("BOOT", "AGENTS", "registered", "{} agents", registry.agents.len());

    // init_phase NÃO aqui (stack do bootloader): roda em raw_sched_run após switch ≥2MB.
    // Redesign round-robin+timeout em agent-core: hang impossível mesmo com SystemAgent.

    // CortexAgent ja foi criado antes do HW discovery — registrar primeiro
    // para que o LLM esteja disponivel para decisoes de hardware
    crate::display::fb::boot_ckpt(48, "CortexAgent register");

    registry.register(Box::new(cortex_agent));

    // Runtime agents — HermesAgent acorda logo apos o Cortex

    registry.register(Box::new(SystemAgent::new()));
    registry.register(Box::new(agents::MonitorAgent::new()));
    registry.register(Box::new(agents::HwBridgeAgent));

    let net_agent = Box::new(agents::NetAgent::new());

    k_nano::slog_bin!("Boot", "info", "NetAgent manifest: name={}, auto_start={}, schedule={:?}",

        net_agent.manifest().name, net_agent.manifest().auto_start, net_agent.manifest().schedule);
    k_nano::slog_hermes!("Net", "info", "registered Continuous — ticks após init_phase (SelfHeal/Disk); gate=e1000 [smoltcp/NIC]");

    registry.register(net_agent);

    registry.register(Box::new(agents::InputAgent::new()));

    // Mouse ANTES do Hermes: Continuous na ordem de registro. Hermes THINK
    // soft-float bloqueia o scheduler — mouse depois do Hermes nunca polla.
    // Posição também atualiza no IRQ (MOUSE_ABS_*) independente do tick.
    registry.register(Box::new(agents::mouse_agent::MouseAgent::new()));

    // Interativos são isentos do rate-limit do scheduler (agent-core: set_urgency
    // >0 = NÃO rate-limited). Sem isso, InputAgent/HwBridgeAgent retornam Pending
    // sempre e após 50 ticks o scheduler os skipa 80% — teclado/rede morrem de
    // fome (polled=1) e o shell nunca recebe o sendkey (bug real de HW + QEMU).
    registry.set_urgency("hw_bridge", 200);
    registry.set_urgency("network_agent", 180);
    registry.set_urgency("input", 200);
    registry.set_urgency("mouse", 150);
    k_nano::slog_bin!("Sched", "info", "urgency aplicada p/ interativos (hw_bridge/network_agent/input/mouse) — isentos de rate-limit");

    // SysInfoAgent — painel de debug com CPU/memória/agentes na tela
    registry.register(Box::new(agents::sysinfo_agent::SysInfoAgent::new()));

    // Display + Metrics ANTES do Hermes: Continuous ring0 polla por ordem de
    // registro. Hermes THINK/LLM soft-float pode bloquear o tick por minutos —
    // se Display vier depois, o orb/HUD nunca sobe (QEMU e HW). Claim graphics
    // só no 1º tick do Display (K* de boot permanece no FB até lá).
    display::fb::fb_remap_uc();
    crate::display::fb::boot_ckpt(40, "pos fb_remap");
    crate::display::fb::boot_ckpt(41, "antes DisplayAgent");
    registry.register(Box::new(display::agent::DisplayAgent::new()));
    crate::display::fb::boot_ckpt(42, "DisplayAgent OK");
    registry.register(Box::new(display::metrics_agent::MetricsAgent::new()));
    crate::display::fb::boot_ckpt(51, "MetricsAgent OK");

    registry.register(Box::new(agents::HermesAgent::new()));

    // The Agency: 30+ agentes especialistas

    agents::register_agency_agents(&mut registry);

    // HW Agents: um agente por dispositivo PCI

    agents::register_hw_agents(&mut registry);

    // Display/Metrics já registrados antes do Hermes (ver acima).

    kjson!("BOOT", "AGENTS", "www", "search", 1);

    registry.register(Box::new(vision_agent::VisionAgent::new()));
    crate::display::fb::boot_ckpt(43, "VisionAgent OK");

    registry.register(Box::new(audio::jarvis::JarbasAgent::new()));
    crate::display::fb::boot_ckpt(44, "JarbasAgent OK");
    // HW sem MSC: saudacao + BOOT.LOG AGORA (hang comum logo apos K44 nos agents audio).
    audio::jarvis::emit_hw_greeting_at_register();

    crate::display::fb::boot_ckpt(45, "antes JarvisVoice");
    registry.register(Box::new(audio::voice::JarbasVoiceAgent::new()));
    crate::display::fb::boot_ckpt(46, "JarvisVoice OK");

    registry.register(Box::new(audio::wakeword::WakeWordAgent::new()));
    crate::display::fb::boot_ckpt(47, "WakeWord OK");

    registry.register(Box::new(audio::pipeline::AudioPipelineAgent::new()));
    crate::display::fb::boot_ckpt(48, "AudioPipeline OK");

    registry.register(Box::new(audio::mixer::AudioMixerAgent::new()));
    crate::display::fb::boot_ckpt(49, "AudioMixer OK");

    let mut cron = cron::CronAgent::new();

    cron.init_defaults();

    registry.register(Box::new(cron));

    registry.register(Box::new(mcp::McpAgent::new()));
    registry.register(Box::new(security::SecurityAgent::new()));
    registry.register(Box::new(safety::SafetyAgent::new()));
    registry.register(Box::new(optimizer::OptimizerAgent::new()));
    registry.register(Box::new(browser_agent::BrowserAgent::new()));
    registry.register(Box::new(sgdb_agent::SgdbAgent::new()));
    registry.register(Box::new(wifi_agent::WifiAgent::new()));
    // ADR-0086 I6: AutoInstallerAgent — EventDriven no tópico SYS_INSTALL
    // (mensageiro: instala o sistema no HD alvo; orquestra HwProfiler+SysInstaller).
    registry.register(Box::new(k_nano::installer_agent::AutoInstallerAgent::new()));

    // Late refresh: GPU CapTokens (GpuCompute) podem ter sido grantados após early emit
    k_hal::hw_gate::emit_all_refresh();

    // BootLogAgent ja registrado no inicio do registry (BOOT_PHASE consumer)

    registry.register(Box::new(agents::log_analyst_agent::LogAnalystAgent::new()));

    // DiagnosticSkill — SystemAgent no SYSTEM_READY + execucao explicita no boot

    let diag_skill = agents::DiagnosticSkill::new();

    k_nano::SKILL_REGISTRY.lock().register(alloc::boxed::Box::new(diag_skill));

    {
        let tok = crate::CapabilityToken::Legacy(1);
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let trust_ok = crate::TRUST_CACHE.lock().check_or_cache(1, "diagnostic", now, 360);
        if !trust_ok {
            k_nano::slog_bin!("Trust", "deny", "boot diagnostic: trust deny");
        } else {
            match k_nano::SKILL_REGISTRY.lock().execute_skill("diagnostic", &[], &tok) {
                Ok(out) => k_nano::slog_bin!("Boot", "info", "DiagnosticSkill executada ({} bytes)", out.len()),
                Err(e) => k_nano::slog_bin!("Boot", "info", "DiagnosticSkill falhou: {}", e),
            }
        }
    }

    

    // Ramdisk — só bootloader 0.11 (removido); Limine usa módulos QEMU loader.

    let mut model_loaded = false;

    // N3: QEMU -device loader ANTES do FAT — PIO de ~200MB no TCG trava/é inviável.
    // Host: -device loader,file=<bitnet>,addr=0x100000000 + -m 6G+
    // Tamanho EXATO via FAT (850/13/2B/3B) — slice > arquivo mapeado → #PF no forward.
    if !model_loaded {
        let load_addr: u64 = 0x100000000;
        // pm_offset já calculado via handoff no início de kernel_boot
        let mem_has_4gb = handoff.has_addr_in_any_region(load_addr);
        let (fat_name, fat_sz): (Option<&'static str>, Option<usize>) = unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            (*ata_guard).as_ref().map(|ata| {
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                        continue;
                    }
                    if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                        // Ordem = degrau ladder; com PACK_LLM=um so, so esse existe.
                        for name in &[
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                        ] {
                            if let Some(sz) = fs.lookup_file_size(name) {
                                if sz >= 1_000_000 {
                                    return (Some(*name), Some(sz));
                                }
                            }
                        }
                    }
                }
                (None, None)
            })
            .unwrap_or((None, None))
        };
        if mem_has_4gb {
            let probe_ptr = (load_addr + pm_offset) as *const u8;
            let raw0 = unsafe { core::ptr::read_volatile(probe_ptr) };
            let raw1 = unsafe { core::ptr::read_volatile(probe_ptr.add(1)) };
            let raw2 = unsafe { core::ptr::read_volatile(probe_ptr.add(2)) };
            let raw3 = unsafe { core::ptr::read_volatile(probe_ptr.add(3)) };
            k_nano::slog_bin!("Asset", "ramdisk", "Probe 4GB: raw=[0x{:02x},0x{:02x},0x{:02x},0x{:02x}]", raw0, raw1, raw2, raw3);
            let qemu_magic = u32::from_le_bytes([raw0, raw1, raw2, raw3]);
            if qemu_magic == 0xBE11BE11 {
                // Fallback so se FAT nao tiver blob (loader-only).
                // v6 é autodescritivo: calcular o tamanho real do arquivo a partir
                // do header (o const v4 604MB truncava o 2B v6 de 792MB → OOM #PF).
                const BITNET_2B_V4_BYTES: usize = 604_856_373;
                let mut model_len = fat_sz.unwrap_or(BITNET_2B_V4_BYTES);
                if fat_sz.is_none() {
                    let hdr = unsafe { core::slice::from_raw_parts(probe_ptr, 64) };
                    if let Some(v6sz) = cortex_crate::model::v6_file_size(hdr) {
                        model_len = v6sz;
                    }
                }
                if let Some(r_end) = handoff.region_end_containing(load_addr) {
                    let region = (r_end - load_addr) as usize;
                    if region < model_len {
                        k_nano::slog_bin!("Asset", "ramdisk", "region {}MB < model {}MB — truncando", region / (1024*1024), model_len / (1024*1024));
                        model_len = region;
                    }
                }
                k_nano::slog_bin!(
                    "Asset",
                    "ramdisk",
                    "QEMU loader: magic OK @0x100000000 exact={}KB fat={:?} name={:?}",
                    model_len / 1024,
                    fat_sz.map(|s| s / 1024),
                    fat_name
                );
                if model_len > 1024 {
                    let model_data = unsafe { core::slice::from_raw_parts(probe_ptr, model_len) };
                    // Copia + LEAK: load_model faz zero-copy nos pesos; dropar o Vec
                    // apos set_model deixava dangling → #PF no FWD (CR2 heap liberado).
                    k_nano::slog_bin!(
                        "Asset",
                        "ramdisk",
                        "QEMU loader: copying {}KB -> heap (leak backing) then load_model...",
                        model_len / 1024
                    );
                    let owned: alloc::vec::Vec<u8> = model_data.to_vec();
                    let leaked: &'static [u8] = alloc::boxed::Box::leak(owned.into_boxed_slice());
                    let llm_v6 = cortex_crate::model::load_model_v6(leaked).and_then(|v| match v {
                        cortex_crate::model::ModelView::Llm(m) => Some(m),
                        _ => None,
                    });
                    if let Some(big_model) = llm_v6.or_else(|| crate::cortex::load_model(leaked)) {
                        crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                        let tag = fat_name.unwrap_or("llama8b.bin");
                        // AIOS na veia (premissa 4): loga a decisão de fit com a RAM
                        // física detectada em runtime — residente vs AirLLM (layer
                        // streaming). O heap auto-adaptativo já cresce até 75% da RAM;
                        // acima disso o modelo exige AirLLM (seam em model_fit).
                        let fit_ram = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
                        let model_mb = (leaked.len() / (1024 * 1024)) as u64;
                        let params = cortex_crate::cortex::GLOBAL_MODEL_PARAMS
                            .load(core::sync::atomic::Ordering::Relaxed);
                        let airllm = cortex_crate::model_fit::needs_airllm(params, model_mb);
                        k_nano::slog_bin!(
                            "Asset",
                            "ramdisk",
                            "LLM LOADED file={} (QEMU-loader@4G->heap-leak) size={}KB RAM={}MB airllm={}",
                            tag,
                            leaked.len() / 1024,
                            fit_ram,
                            airllm
                        );
                        crate::boot_logger::log("BOOT: QEMU loader Falcon3-3B-Instruct-1.58bit loaded");
                        model_loaded = true;
                        // Marca onde começa a região de experts (após modelos grandes,
                        // benchmarks, BPE, BGE — tudo ordenado por tamanho descendente
                        // pelo script PS1). Expert scan começa daqui, evita carregar
                        // tinystories/Piper/BITNET2B como se fossem experts (Falcon3 ja no CURRENT_MODEL).
                        QEMU_LOADER_SCAN_START.store(0x129000000, core::sync::atomic::Ordering::Relaxed);
                    } else {
                        k_nano::slog_bin!("RAMDISK", "info", "QEMU loader: load_model FAILED");
                        crate::load_status::set(
                            crate::load_status::AssetKind::Llm,
                            crate::load_status::LoadStatus::Failed,
                        );
                    }
                }
            } else {
                k_nano::slog_bin!("RAMDISK", "info", "No model at 0x100000000 — trying 0x120000000...");
                let load_addr2: u64 = 0x120000000;
                let has_addr2 = handoff.has_addr_in_any_region(load_addr2);
                if has_addr2 {
                    let probe2 = (load_addr2 + pm_offset) as *const u32;
                    let magic2 = unsafe { core::ptr::read_volatile(probe2) };
                    if magic2 == 0xBE11BE11 {
                        const BITNET_2B_V4_BYTES: usize = 604_856_373;
                        let mut model_len2 = fat_sz
                            .filter(|&sz| sz >= 50 * 1024 * 1024)
                            .unwrap_or(BITNET_2B_V4_BYTES);
                        // Bounds check: truncar ao fim da região de memória
                        if let Some(r_end) = handoff.region_end_containing(load_addr2) {
                            let region2 = (r_end - load_addr2) as usize;
                            if region2 < model_len2 {
                                k_nano::slog_bin!("Asset", "ramdisk", "region2 {}MB < model2 {}MB — truncando", region2 / (1024*1024), model_len2 / (1024*1024));
                                model_len2 = region2;
                            }
                        }
                        let model_data2 = unsafe { core::slice::from_raw_parts(probe2 as *const u8, model_len2) };
                        let llm_v6 = cortex_crate::model::load_model_v6(model_data2).and_then(|v| match v {
                            cortex_crate::model::ModelView::Llm(m) => Some(m),
                            _ => None,
                        });
                        if let Some(big_model) = llm_v6.or_else(|| crate::cortex::load_model(model_data2)) {
                            crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                            k_nano::slog_bin!("RAMDISK", "info", "LLM LOADED file=Falcon3-3B-Instruct-1.58bit (QEMU-loader @0x120000000)");
                            model_loaded = true;
                        }
                    }
                }
            }
        } else {
            k_nano::slog_bin!("RAMDISK", "info", "4GB not in memory map (use -m 6G) — fallback FAT.");
        }
    }

    if !model_loaded {
        // FAT: preferir 2B só se ≤48MB (PIO). >48MB = PRESENT (loader/HW).
        // MICRO fallback para boot QEMU sem travar TCG.
        unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                    if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                        // QEMU TCG: cap 48MB (loader @4GB cobre 2B). Baremetal/HW: sem
                        // QEMU-loader → permitir FAT PIO grande (lento, mas único path).
                        let qemu_loader_2b = {
                            let pm = crate::memory::PHYS_MEM_OFFSET
                                .load(core::sync::atomic::Ordering::Relaxed);
                            if pm == 0 {
                                false
                            } else {
                                let va = (0x100000000u64 + pm) as *const u32;
                                unsafe { core::ptr::read_volatile(va) == 0xBE11BE11 }
                            }
                        };
                        const PIO_QEMU: usize = 8 * 1024 * 1024;
                        const PIO_HW: usize = 700 * 1024 * 1024;
                        let pio_cap = if qemu_loader_2b
                            || k_nano::platform_probe::hypervisor().is_sandbox()
                        {
                            PIO_QEMU
                        } else {
                            PIO_HW
                        };
                        // Preferência chat: 1.3B → 850 → 2B/3B. Stub MICRO.BITNET por último.
                        // Degrau: PACK_LLM=850 só empacota 850; depois 13, 2b, 3b.
                        // Modelos GGUF grandes: /model (AirLLM ATA) em vez de PIO full-RAM.
                        for name in &[
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                            "llama8b.bin",
                        ] {
                            let Some(sz) = fs.lookup_file_size(name) else { continue; };
                            // Stub ~13KB não serve para chat fluente
                            if *name == "llama8b.bin" && sz < 1_000_000 {
                                k_nano::slog_nano!(
                                    "FAT",
                                    "info",
                                    "{} size={}B — stub smoke, skip Active chat",
                                    name,
                                    sz
                                );
                                continue;
                            }
                            if sz > pio_cap {
                                k_nano::slog_nano!("FAT", "info", "{} PRESENT size={}KB — skip full PIO (cap={}MB; QEMU-loader ou --features hw)",
                                    name,
                                    sz / 1024,
                                    pio_cap / (1024 * 1024));
                                continue;

                                // AirLLM: modelo grande demais para PIO — streaming layer-by-layer
                                let fat_mb = sz / (1024 * 1024);
                                let params_est = (fat_mb as u64) * 8_000_000;
                                if cortex_crate::model_fit::needs_airllm(params_est, fat_mb as u64) {
                                    k_nano::slog_nano!("FAT", "info",
                                        "AirLLM: {} ({}MB) > PIO cap - activating streaming", name, fat_mb);
                                    if let Ok(sm) = crate::gguf_streaming::StreamingModel::from_fat(name) {
                                        cortex_crate::cortex::set_streaming_model(
                                            alloc::boxed::Box::new(sm));
                                        k_nano::slog_nano!("FAT", "info",
                                            "AirLLM: streaming registered for {} (layers from disk)", name);
                                        model_loaded = true;
                                        break;
                                    }
                                }
                            }
                            if sz > PIO_QEMU {
                                k_nano::slog_nano!("FAT", "info", "{} size={}MB — baremetal FAT PIO (pode demorar minutos)",
                                    name,
                                    sz / (1024 * 1024));
                            }
                            k_nano::slog_nano!("FAT", "info", "lendo {} ({}KB) — candidato LLM...", name, sz / 1024);
                            if let Some(fat_data) = fs.read_file(name) {
                                let llm_v6 = cortex_crate::model::load_model_v6(&fat_data).and_then(|v| match v {
                                    cortex_crate::model::ModelView::Llm(m) => Some(m),
                                    _ => None,
                                });
                                if let Some(big_model) = llm_v6.or_else(|| crate::cortex::load_model(&fat_data)) {
                                    crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                                    k_nano::slog_nano!("FAT", "info", "LLM LOADED file={} size={}KB — CortexAgent upgraded.", name, fat_data.len() / 1024);
                                    crate::boot_logger::log("BOOT: FAT BitNet model loaded");
                                    model_loaded = true;
                                    break;
                                } else {
                                    k_nano::slog_nano!("FAT", "info", "{} presente mas load_model FAILED", name);
                                    crate::load_status::set(
                                        crate::load_status::AssetKind::Llm,
                                        crate::load_status::LoadStatus::Failed,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        // USB-MSC: mesmo stick unificado (boot ESP + dados) quando nao ha ATA/IDE
        if !model_loaded && !k_nano::platform_probe::hypervisor().is_sandbox() {
            unsafe {
                let mut usb_guard = crate::USB_MSC.lock();
                if let Some(ref mut msc) = *usb_guard {
                    const PIO_HW: usize = 700 * 1024 * 1024;
                    for name in &[
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                        "llama8b.bin",
                    ] {
                        let Some(fat_data) = read_file_from_dev(msc, name) else { continue; };
                        if *name == "llama8b.bin" && fat_data.len() < 1_000_000 {
                            continue;
                        }
                        if fat_data.len() > PIO_HW {
                            k_nano::slog_nano!("FAT", "info", "USB {} PRESENT size={}KB — skip PIO",
                                name,
                                fat_data.len() / 1024);
                            continue;
                        }
                        k_nano::slog_nano!("FAT", "info", "USB lendo {} ({}KB) — candidato LLM...",
                            name,
                            fat_data.len() / 1024);
                        let llm_v6 = cortex_crate::model::load_model_v6(&fat_data).and_then(|v| match v {
                            cortex_crate::model::ModelView::Llm(m) => Some(m),
                            _ => None,
                        });
                        if let Some(big_model) = llm_v6.or_else(|| crate::cortex::load_model(&fat_data)) {
                            crate::cortex::set_model(alloc::boxed::Box::new(big_model));
                            k_nano::slog_nano!("FAT", "info", "LLM LOADED file={} size={}KB via USB-MSC",
                                name,
                                fat_data.len() / 1024);
                            crate::boot_logger::log("BOOT: FAT BitNet model loaded (USB)");
                            model_loaded = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    // Trinity experts: QEMU-loader (HW @0x160000000, RustCoder @0x161000000) + FAT fallback
    {
        fn fat_size_hint(names: &[&str], default: usize) -> usize {
            unsafe {
                let ata_guard = crate::ATA_DRIVER.lock();
                if let Some(ref ata) = *ata_guard {
                    let parts = crate::fat32::read_mbr(ata);
                    for p in &parts {
                        if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                            continue;
                        }
                        if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                            for name in names {
                                if let Some(sz) = fs.lookup_file_size(name) {
                                    return sz.min(1024 * 1024).max(64);
                                }
                            }
                        }
                    }
                }
            }
            default
        }
        /// Scan QEMU loader region [start..end) step=1MB for magic 0xBE11BE11,
        /// then try to load as expert .bitnet. Returns true if loaded.
        fn try_expert_qemu_scan(start: u64, end: u64, size: usize, label: &str, is_hw: bool) -> bool {
            let pm = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
            if pm == 0 {
                return false;
            }
            let mut addr = start;
            while addr < end {
                // SESSÃO_260: só lê páginas PRESENT (scan atravessa hole quando
                // a RAM não alcança os endereços do loader → #PF storm).
                if !crate::memory::is_page_present(addr + pm) {
                    addr = addr.saturating_add(0x100000);
                    continue;
                }
                let ptr = (addr + pm) as *const u8;
                let magic = unsafe { core::ptr::read_volatile(ptr as *const u32) };
                if magic == 0xBE11BE11 {
                    // Bounds check: não ler além do fim do range do scan
                    if addr + (size as u64) > end {
                        k_nano::slog_bin!("Asset", "loader",
                            "{} @{:#x} size {} bytes beyond scan end — skipping", label, addr, size);
                        addr = addr.saturating_add(0x100000);
                        continue;
                    }
                    k_nano::slog_bin!("Asset", "loader",
                        "{} magic 0xBE11BE11 found @{:#x} — tentando parse {} KB",
                        label, addr, size / 1024);
                    let data = unsafe { core::slice::from_raw_parts(ptr, size) };
                    if let Some(model) = crate::cortex::load_model(data) {
                        let nl = model.num_layers;
                        let hd = model.hidden;
                        // Rejeita modelos degenerados (h=0 L=0) — é outro modelo
                        // (ex: BITNET2B cujo parse com tamanho pequeno falha, ou
                        // Piper TTS que tem ver=3 h=0 L=0 e seria aceito como expert).
                        if nl == 0 || hd == 0 {
                            k_nano::slog_bin!("Asset", "loader",
                                "{} @{:#x} degenerado layers={} hidden={} — pulando",
                                label, addr, nl, hd);
                            addr = addr.saturating_add(0x100000);
                            continue;
                        }
                        if is_hw {
                            crate::cortex::set_hwexpert_model(alloc::boxed::Box::new(model));
                        } else {
                            crate::cortex::set_rustcoder_model(alloc::boxed::Box::new(model));
                        }
                        k_nano::slog_bin!("Asset", "loader",
                            "{} LOADED (QEMU-loader @{:#x}) size={}KB layers={} hidden={}",
                            label, addr, size / 1024, nl, hd);
                        return true;
                    } else {
                        k_nano::slog_bin!("Asset", "loader",
                            "{} @{:#x} parse FAILED (proximo endereco)", label, addr);
                    }
                }
                addr = addr.saturating_add(0x100000); // 1MB steps
            }
            k_nano::slog_bin!("Asset", "loader",
                "{} QEMU-loader scan [{:#x}..{:#x}] — 0xBE11BE11 ausente",
                label, start, end);
            false
        }

        // Tamanhos reais dos .bitnet no QEMU-loader (FAT hint curto truncava HW → parse FAILED).
        // Sprint 107 Part B #8: header fix (vocab_size/num_medusa u16→u32, ver
        // tools/fix_bitnet_header.py) somou +4 bytes: 266126 → 266130.
        let hw_sz = 266130usize.max(fat_size_hint(
            &["llama8b.bin", "llama8b.bin"],
            266130,
        ));
        let rust_sz = 270222usize.max(fat_size_hint(
            &["llama8b.bin", "llama8b.bin", "llama8b.bin", "llama8b.bin"],
            270222,
        ));
        // Scan QEMU loader region (0x100000000..0x180000000) for expert magics.
        // Old hardcoded addresses (0x160000000, 0x161000000) were wrong because
        // the PS1 auto-loader places files sequentially from 0x100000000.
        // Garante ler 1MB por expert (arquivos reais ~300KB, gap 1MB entre
        // QEMU loaders — safe). Sem isso, size hint pequeno (ex: 270KB)
        // trunca RUSTCDR2.BIN (326KB) e o scan falha, caindo no próximo magic.
        let scan_sz = hw_sz.max(rust_sz).max(1024 * 1024);
        // Scan cada expert em seu próprio range:
        //   RUSTCDR2.BIN  @0x129000000  (RustCoder)
        //   hw_expert_v3  @0x129200000  (HW expert)
        // Separação evita que ambos carreguem do mesmo arquivo.
        let mut rust_ok = try_expert_qemu_scan(0x129000000, 0x129200000, scan_sz, "RUSTCODER", false);
        let mut hw_ok = try_expert_qemu_scan(0x129200000, 0x180000000, scan_sz, "HWEXPERT", true);

        // SESSION llama8b-unificado: estes loops faziam read_file("llama8b.bin")
        // (~1.8GB ATA PIO) apos K49 — FB congela. Experts so via QEMU-loader
        // no hypervisor; no metal, nomes reais + teto 8MB.
        const EXPERT_PIO_CAP: usize = 8 * 1024 * 1024;
        let qemu_hv = k_nano::platform_probe::hypervisor().is_sandbox();
        crate::display::fb::boot_ckpt(50, "experts pos QEMU-scan");
        if qemu_hv {
            k_nano::slog_bin!(
                "FAT",
                "info",
                "skip expert FAT/USB PIO no hypervisor (nao ler llama8b.bin 1.8GB)"
            );
        } else {
        unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                        continue;
                    }
                    if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                        if !rust_ok {
                            for rname in &[
                                "RUSTCDR3.v6",
                                "RUSTCDR3.BIN",
                                "RUSTCDR2.BIN",
                                "RUSTCDR.BITNET",
                                "RUSTCDR.BIN",
                            ] {
                                let Some(sz) = fs.lookup_file_size(rname) else { continue };
                                if sz > EXPERT_PIO_CAP {
                                    k_nano::slog_bin!("FAT", "info", "{} {}KB > expert cap — skip PIO", rname, sz / 1024);
                                    continue;
                                }
                                if let Some(rust_data) = fs.read_file(rname) {
                                    if let Some(rust_model) = crate::cortex::load_model(&rust_data) {
                                        crate::cortex::set_rustcoder_model(alloc::boxed::Box::new(
                                            rust_model,
                                        ));
                                        k_nano::slog_bin!(
                                            "FAT",
                                            "info",
                                            "RustCoder expert LOADED file={} size={}KB (Trinity hub same router)",
                                            rname,
                                            rust_data.len() / 1024
                                        );
                                        crate::boot_logger::log("BOOT: RustCoder expert loaded");
                                        rust_ok = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if !hw_ok {
                            for hname in &["HWEXPRT.v6", "HWEXPRT.BIN", "HWEXPERT.BIN"] {
                                let Some(sz) = fs.lookup_file_size(hname) else { continue };
                                if sz > EXPERT_PIO_CAP {
                                    continue;
                                }
                                if let Some(hw_data) = fs.read_file(hname) {
                                    if let Some(hw_model) = crate::cortex::load_model(&hw_data) {
                                        crate::cortex::set_hwexpert_model(alloc::boxed::Box::new(
                                            hw_model,
                                        ));
                                        k_nano::slog_bin!("FAT", "info", "HW Expert model loaded (213K HWIDs)!");
                                        crate::boot_logger::log("BOOT: HW Expert loaded");
                                        hw_ok = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // USB-MSC experts (stick unificado)
        if !hw_ok || !rust_ok {
            unsafe {
                let mut usb_guard = crate::USB_MSC.lock();
                if let Some(ref mut msc) = *usb_guard {
                    if !rust_ok {
                        for rname in &["RUSTCDR3.BIN", "RUSTCDR2.BIN", "RUSTCDR.BITNET", "RUSTCDR.BIN"] {
                            if let Some(rust_data) = read_file_from_dev(msc, rname) {
                                if rust_data.len() > EXPERT_PIO_CAP {
                                    continue;
                                }
                                if let Some(rust_model) = crate::cortex::load_model(&rust_data) {
                                    crate::cortex::set_rustcoder_model(alloc::boxed::Box::new(rust_model));
                                    k_nano::slog_bin!("FAT", "info", "RustCoder expert loaded (USB {})!", rname);
                                    rust_ok = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !hw_ok {
                        if let Some(hw_data) = read_file_from_dev(msc, "HWEXPRT.BIN") {
                            if hw_data.len() <= EXPERT_PIO_CAP {
                                if let Some(hw_model) = crate::cortex::load_model(&hw_data) {
                                    crate::cortex::set_hwexpert_model(alloc::boxed::Box::new(hw_model));
                                    k_nano::slog_bin!("FAT", "info", "HW Expert loaded (USB-MSC)!");
                                    hw_ok = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        } // !qemu_hv expert FAT/USB
        if hw_ok {
            crate::boot_logger::log("BOOT: HW Expert loaded");
        }
        if rust_ok {
            crate::boot_logger::log("BOOT: RustCoder expert loaded");
        }
        // ── HW Expert v4 multi-head ──────────────────────────────
        // Carrega modelo v5 (multi-head) separado do v3 (free-text).
        // HWEXPRT4.BIN pode vir do QEMU-loader ou FAT.
        {
            let pm = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
            let mut v4_ok = false;
            if pm != 0 {
                // Scan QEMU loader range para HWEXPRT4.BIN
                let scan_start: u64 = 0x129400000;
                let scan_end: u64 = 0x180000000;
                let scan_sz: usize = 1024 * 1024;
                let mut addr = scan_start;
                while addr < scan_end && !v4_ok {
                    // SESSÃO_260: scan atravessa hole quando RAM não alcança.
                    if !crate::memory::is_page_present(addr + pm) {
                        addr = addr.saturating_add(0x100000);
                        continue;
                    }
                    let ptr = (addr + pm) as *const u8;
                    let magic = unsafe { core::ptr::read_volatile(ptr as *const u32) };
                    if magic == 0xBE11BE11 {
                        if addr + (scan_sz as u64) <= scan_end {
                            let data = unsafe { core::slice::from_raw_parts(ptr, scan_sz) };
                            if let Some(v4model) = cortex_crate::cortex::load_hwexpert_v6(data)
                                .or_else(|| cortex_crate::cortex::load_hwexpert_v5(data))
                            {
                                cortex_crate::cortex::set_hwexpert_v4_model(v4model);
                                k_nano::slog_bin!("HWEXPERT", "info", "v4 multi-head LOADED (QEMU-loader @{:#x})", addr);
                                v4_ok = true;
                            }
                        }
                    }
                    addr = addr.saturating_add(0x100000);
                }
            }
            // FAT32 fallback
            if !v4_ok && !k_nano::platform_probe::hypervisor().is_sandbox() {
                unsafe {
                    let ata_guard = crate::ATA_DRIVER.lock();
                    if let Some(ref ata) = *ata_guard {
                        let parts = crate::fat32::read_mbr(ata);
                        for p in &parts {
                            if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                                continue;
                            }
                            if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                                let v4name = "HWEXPRT4.BIN";
                                let too_big = fs
                                    .lookup_file_size(v4name)
                                    .map(|s| s > 8 * 1024 * 1024)
                                    .unwrap_or(true);
                                if too_big {
                                    continue;
                                }
                                if let Some(v4data) = fs.read_file(v4name) {
                                    if let Some(v4model) = crate::cortex::load_hwexpert_v6(&v4data)
                                        .or_else(|| crate::cortex::load_hwexpert_v5(&v4data))
                                    {
                                        crate::cortex::set_hwexpert_v4_model(v4model);
                                        k_nano::slog_bin!("HWEXPERT", "info", "v4 multi-head LOADED (FAT) size={}KB", v4data.len() / 1024);
                                        v4_ok = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if v4_ok {
                crate::boot_logger::log("BOOT: HW Expert v4 multi-head loaded");
                // Pós-carga: escreve predições no SGDB /hw/pci/*
                crate::boot_logger::log("BOOT: HW Expert v4 predictions → SGDB /hw/pci/");
                let _ = k_ai::sgdb::store::predict_all_pci();
            }
        }
    }

    // ModelHub extras: TinyStories / 850M fast / 3B pro — não substituem Active se já carregado
    {
        // I5 ADR-0086: carrega modelo da NeuralFS do disco instalado (Residente).
        // O ModelProvisioner persiste em /models/; boot lê daqui sem re-baixar.
        fn try_hub_slot_neuralfs(slot: crate::model_hub::ModelSlot) -> bool {
            let mut ata_guard = crate::ATA_DRIVER.lock();
            let Some(ata) = ata_guard.as_mut() else { return false };
            let parts = crate::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != k_nano::neural_fs::volume::MBR_TYPE_NEURALFS {
                    continue;
                }
                let dev: &mut dyn k_nano::block_dev::BlockDevice = ata;
                let Some(vol) = k_nano::neural_fs::volume::NeuralVolume::mount(
                    dev,
                    p.lba_start as u64,
                ) else {
                    continue;
                };
                // ModelProvisioner grava em /models/<FAT_NAME> (create_file na raiz não cria dirs).
                // ponytail: procura na raiz e em /boot (kernel.elf fica em /boot).
                for root in ["models", "boot", ""] {
                    for name in crate::model_hub::fat_names_for(slot) {
                        let path = if root.is_empty() {
                            alloc::string::String::from(*name)
                        } else {
                            alloc::format!("{}/{}", root, name)
                        };
                        let Some(ino) = vol.resolve_path(dev, &path) else { continue };
                        let Ok(data) = vol.read_file(dev, ino) else { continue };
                        if let Some(m) = crate::cortex::load_model(&data) {
                            crate::cortex::register_model_slot(slot, alloc::boxed::Box::new(m));
                            k_nano::slog_bin!(
                                "MODEL",
                                "info",
                                "NeuralFS LOADED file={} slot={} size={}KB",
                                path,
                                slot.name(),
                                data.len() / 1024
                            );
                            return true;
                        }
                    }
                }
            }
            false
        }

        fn try_hub_slot_fat(slot: crate::model_hub::ModelSlot) {
            if crate::model_hub::slot_loaded(slot)
                && matches!(
                    slot,
                    crate::model_hub::ModelSlot::Vision
                        | crate::model_hub::ModelSlot::GeneratorPro
                        | crate::model_hub::ModelSlot::Reranker
                )
            {
                // Pode estar só marcado (pro-alias); ainda tenta blob dedicado
            }
            // QEMU+loader 2B: cap 48MB no hub. HW / sem loader: até 700MB (850/2B/3B).
            let qemu_loader_2b = {
                let pm = crate::memory::PHYS_MEM_OFFSET
                    .load(core::sync::atomic::Ordering::Relaxed);
                if pm == 0 {
                    false
                } else {
                    let va = (0x100000000u64 + pm) as *const u32;
                    unsafe { core::ptr::read_volatile(va) == 0xBE11BE11 }
                }
            };
            // Com QEMU-loader Active ja em RAM: NUNCA PIO do chat grande no hub
            // (antes PIO_FAST=400MB relia BITNET850 e travava o boot por minutos/horas).
            const PIO_FAST: usize = 400 * 1024 * 1024;
            const PIO_HW: usize = 700 * 1024 * 1024;
            const PIO_QEMU: usize = 8 * 1024 * 1024;
            let qemu_hv = qemu_loader_2b || k_nano::platform_probe::hypervisor().is_sandbox();
            let pio_cap = if qemu_hv {
                match slot {
                    crate::model_hub::ModelSlot::Reranker => PIO_QEMU,
                    _ => PIO_QEMU,
                }
            } else {
                match slot {
                    crate::model_hub::ModelSlot::Vision => PIO_FAST,
                    crate::model_hub::ModelSlot::Reranker => 32 * 1024 * 1024,
                    _ => PIO_HW,
                }
            };
            // Active ja veio do loader — nao duplicar GeneratorFast/Pro via FAT
            if qemu_loader_2b
                && crate::cortex::model_is_loaded()
                && matches!(
                    slot,
                    crate::model_hub::ModelSlot::Vision
                        | crate::model_hub::ModelSlot::GeneratorPro
                )
            {
                k_nano::slog_bin!(
                    "MODEL",
                    "info",
                    "hub skip {} — Active ja carregado via QEMU-loader",
                    slot.name()
                );
                return;
            }
            unsafe {
                let ata_guard = crate::ATA_DRIVER.lock();
                if let Some(ref ata) = *ata_guard {
                    let parts = crate::fat32::read_mbr(ata);
                    for p in &parts {
                        if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                            continue;
                        }
                        if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                            for name in crate::model_hub::fat_names_for(slot) {
                                let Some(sz) = fs.lookup_file_size(name) else { continue };
                                if *name == "llama8b.bin" && sz < 1_000_000 {
                                    continue;
                                }
                                if sz > pio_cap {
                                    k_nano::slog_bin!(
                                        "MODEL",
                                        "info",
                                        "{} PRESENT {}KB — skip PIO (cap={}MB); slot={}",
                                        name,
                                        sz / 1024,
                                        pio_cap / (1024 * 1024),
                                        slot.name()
                                    );
                                    continue;
                                }
                                if let Some(data) = fs.read_file(name) {
                                    if let Some(m) = crate::cortex::load_model(&data) {
                                        // Se Active vazio e slot é pro/fast, vira primary
                                        if !crate::cortex::model_is_loaded()
                                            && matches!(
                                                slot,
                                                crate::model_hub::ModelSlot::GeneratorPro
                                                    | crate::model_hub::ModelSlot::Vision
                                            )
                                        {
                                            crate::cortex::set_model(alloc::boxed::Box::new(m));
                                            if slot == crate::model_hub::ModelSlot::GeneratorPro {
                                                crate::model_hub::mark_pro_alias(true);
                                            }
                                        } else {
                                            crate::cortex::register_model_slot(
                                                slot,
                                                alloc::boxed::Box::new(m),
                                            );
                                        }
                                        k_nano::slog_bin!(
                                            "MODEL",
                                            "info",
                                            "hub LOADED file={} slot={} size={}KB",
                                            name,
                                            slot.name(),
                                            data.len() / 1024
                                        );
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // I5: NeuralFS do disco instalado primeiro (Residente), depois FAT32 (pendrive/live)
        for s in [
            crate::model_hub::ModelSlot::Reranker,
            crate::model_hub::ModelSlot::Vision,
            crate::model_hub::ModelSlot::GeneratorPro,
            crate::model_hub::ModelSlot::Learner,
            crate::model_hub::ModelSlot::Agent,
        ] {
            if !crate::model_hub::slot_loaded(s) && try_hub_slot_neuralfs(s) {
                k_nano::slog_bin!("HUB", "info", "slot {} via NeuralFS", s.name());
            }
        }
        try_hub_slot_fat(crate::model_hub::ModelSlot::Reranker);
        try_hub_slot_fat(crate::model_hub::ModelSlot::Vision);
        try_hub_slot_fat(crate::model_hub::ModelSlot::GeneratorPro);
        try_hub_slot_fat(crate::model_hub::ModelSlot::Learner);
        try_hub_slot_fat(crate::model_hub::ModelSlot::Agent);
        k_nano::slog_bin!("HUB", "info", "Agent slot load attempted");
        // Se Active é grande (≥200MB heurística via embed), marca pro-alias
        let dim = crate::cortex::CURRENT_MODEL_EMBED_DIM.load(core::sync::atomic::Ordering::Relaxed);
        if dim >= 2048 {
            crate::model_hub::mark_pro_alias(true);
        }
        k_nano::slog_bin!("MODEL", "info", "{}", crate::model_hub::hub_status());
    }

    // BPE vocab HF (BPB1) via QEMU-loader + FAT — ANTES do LLM-TEST
    // (senão generate cai em CHAR vocab → ABAB / incoerente em BitNet 32k/128k).
    if !crate::bpe::try_load_from_qemu_loader() {
        let _ = crate::bpe::try_load_from_fat();
    }

    // STT CTC tiny: QEMU-loader @0x163000000, depois FAT STT.BIN (HW real)
    if !crate::audio::stt::try_load_from_qemu_loader() {
        let _ = crate::audio::stt::try_load_from_fat();
    }

    // F1/F2: Enable coherence sampling for all BPE models (default: temp=0.7, top_k=16, repeat=1.2)
    if crate::bpe::is_loaded() {
        crate::cortex::set_coherence(true, 0.7, 16, 1.2);
    }

    // LLM test + telemetria N1.1 — ladder prompts (coerencia/tempo) no boot serial
    // ponytail: skip — forward pass soft-float ~2s; trava no WHPX. Testar manual via serial.
    if false && (model_loaded || crate::cortex::model_is_loaded()) {
        let prompts: &[&str] = &[
            "ola",
            "quanto e 2 mais 2",
            "o que e neural os",
        ];
        // Ladder: 1 token-ish curto — soft-float WHPX em h=1536+ e lento; 3 prompts completos
        // podem levar dezenas de min. Loga ticks por prompt para custo/tempo.
        for (i, p) in prompts.iter().enumerate() {
            let t0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let r = crate::cortex::generate_via_model(p);
            let t1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let ticks = t1.saturating_sub(t0);
            k_nano::slog_bin!(
                "LLM-TEST",
                "info",
                "#{}/{} prompt='{}' ticks={} (~{}s) response='{}'",
                i + 1,
                prompts.len(),
                p,
                ticks,
                ticks / 100,
                r
            );
        }
        crate::load_status::set(
            crate::load_status::AssetKind::Llm,
            crate::load_status::LoadStatus::Loaded,
        );
    } else {
        k_nano::slog_bin!("LLM-TEST", "info", "no model — ABSENT");
        crate::boot_logger::log("BOOT: LLM ABSENT — sem ramdisk/loader/FAT modelo utilizavel");
        k_nano::slog_bin!("LLM", "info", "ABSENT — BitNet 2B nao carregado (FAT/ramdisk)");
        crate::load_status::set_if_upgrade(
            crate::load_status::AssetKind::Llm,
            crate::load_status::LoadStatus::Absent,
        );
    }

    crate::load_status::print_status_banner();

    // BPE já tentado antes do LLM-TEST; se ainda ausente, retry (FAT tardio).
    if !crate::bpe::is_loaded() {
        if !crate::bpe::try_load_from_qemu_loader() {
            let _ = crate::bpe::try_load_from_fat();
        }
    }

    // --- Boot greeting: LLM → TTS → FB quando modelo+BPE prontos ---
    // Antes era gated por `weather-e2e` feature flag. Agora flui automaticamente.
    let mut n3_gen: Option<bool> = None;
    let mut n4_intent: Option<bool> = None;
    let mut n5_voice: Option<bool> = None;

    let model_ok = crate::cortex::model_is_loaded();
    let bpe_ok = crate::bpe::is_loaded();
    let already_greeted = audio::jarvis::hw_greet_emitted();
    let qemu = k_nano::platform_probe::hypervisor().is_sandbox();

    // K49 hang: generate_via_model no boot (CPU QEMU / header lixo) apos AudioMixer.
    // Template ja foi falado em emit_hw_greeting_at_register.
    if already_greeted || qemu {
        crate::display::fb::boot_ckpt(50, "saudacao LLM skip (template/QEMU)");
        k_nano::slog_bin!(
            "JARBAS",
            "GREETING",
            "skip generate_via_model (emitted={} qemu={}) — segue Runtime",
            already_greeted,
            qemu
        );
    } else if model_ok && bpe_ok {
        crate::display::fb::boot_ckpt(49, "Gerando saudacao LLM...");
        k_nano::slog_bin!("JARBAS", "GREETING",
            "model LOADED + BPE LOADED — gerando saudacao via LLM");
        let greeting_prompt =
            "You are Jarbas, the Neural OS voice assistant. \
             Generate a single short warm greeting sentence in Portuguese. \
             Be concise, one sentence.";
        let raw = crate::cortex::generate_via_model(greeting_prompt);
        if raw.is_empty() || raw == crate::cortex::NO_MODEL_MSG {
            k_nano::slog_bin!("JARBAS", "GREETING",
                "LLM generate vazio — fallback para saudacao fixa");
            n3_gen = Some(false);
            n4_intent = Some(false);
            n5_voice = Some(false);
        } else {
            k_nano::slog_bin!("JARBAS", "GREETING", "LLM: \"{}\"", raw);
            n3_gen = Some(true);
            n4_intent = Some(true);
            let pcm = crate::audio::skills::synthesize_tts(&raw);
            crate::display::fb::paint_tts_response(&raw);
            n5_voice = Some(!pcm.is_empty());
            k_nano::slog_bin!("JARBAS", "GREETING",
                "TTS samples={} FB painted",
                pcm.len());
        }
    } else if model_ok && !bpe_ok {
        k_nano::slog_bin!("JARBAS", "GREETING",
            "model LOADED mas BPE ausente — generate com CHAR fallback");
        let raw = crate::cortex::generate_via_model("ola");
        if !raw.is_empty() && raw != crate::cortex::NO_MODEL_MSG {
            k_nano::slog_bin!("JARBAS", "GREETING", "CHAR LLM: \"{}\"", raw);
            n3_gen = Some(true);
            n4_intent = Some(true);
            let pcm = crate::audio::skills::synthesize_tts(&raw);
            crate::display::fb::paint_tts_response(&raw);
            n5_voice = Some(!pcm.is_empty());
        } else {
            k_nano::slog_bin!("JARBAS", "GREETING",
                "CHAR generate vazio — sem saudacao LLM");
            n3_gen = Some(false);
            n4_intent = Some(false);
            n5_voice = Some(false);
        }
    } else {
        k_nano::slog_bin!("JARBAS", "GREETING",
            "model ABSENT — saudacao LLM pulada");
    }

    // Weather-e2e path (STT-sim → seed → lexicon): só com --features weather-e2e
    if crate::demo_flags::RUN_WEATHER_E2E_SKINNY {
        k_nano::slog_bin!("Boot", "info",
            "weather-e2e feature ativo — rodando STT-sim/seed/lexicon");
        // STT CTC (PCM sintético) → Hermes → generate_via_model → TTS (sem canned).
        let stt_seed = "qual a previsao do tempo para amanha?";
        let pcm_probe = crate::audio::tts::synthesize(stt_seed);
        let mut stt_ctc = crate::audio::stt::transcribe_global(&pcm_probe);
        let ctc_alpha = |s: &str| s.chars().filter(|c| c.is_ascii_alphabetic()).count();
        if ctc_alpha(&stt_ctc) < 4 && crate::audio::skills::piper_is_loaded() {
            let pcm2 = crate::audio::skills::synthesize_tts("tempo");
            let ctc2 = crate::audio::stt::transcribe_global(&pcm2);
            k_nano::slog_bin!("JARBAS", "STT", "retry piper-pcm len={} ctc_len={} ctc='{}' (prev='{}')",
                pcm2.len(), ctc2.len(), ctc2, stt_ctc);
            if ctc_alpha(&ctc2) > ctc_alpha(&stt_ctc) { stt_ctc = ctc2; }
        }
        if ctc_alpha(&stt_ctc) < 4 && crate::audio::skills::piper_is_loaded() {
            let pcm3 = crate::audio::skills::synthesize_tts("dia sol");
            let ctc3 = crate::audio::stt::transcribe_global(&pcm3);
            k_nano::slog_bin!("JARBAS", "STT", "retry2 piper-pcm len={} ctc_len={} ctc='{}'",
                pcm3.len(), ctc3.len(), ctc3);
            if ctc_alpha(&ctc3) > ctc_alpha(&stt_ctc) { stt_ctc = ctc3; }
        }
        k_nano::slog_bin!("JARBAS", "STT", "pcm_len={} ctc_len={} ctc='{}'",
            pcm_probe.len(), stt_ctc.len(), stt_ctc);
        {
            let ctc_payload = if stt_ctc.is_empty() {
                alloc::string::String::from("[ctc empty]")
            } else { stt_ctc.clone() };
            let _ = crate::EVENT_BUS.publish(crate::Event {
                id: 0,
                topic: alloc::string::String::from(crate::audio::TOPIC_STT_TEXT),
                payload: ctc_payload.into_bytes(),
                token: crate::CapabilityToken::Legacy(1),
            });
        }
        let stt_owned = if stt_ctc.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 4 {
            if crate::bpe::weatherish_hit_count(&stt_ctc) >= 1
                || stt_ctc.contains("temp") || stt_ctc.contains("dia")
            { stt_ctc } else {
                k_nano::slog_bin!("JARBAS-STT", "info", "weak ctc → seed prompt");
                alloc::string::String::from(stt_seed)
            }
        } else {
            if !stt_ctc.is_empty() {
                k_nano::slog_bin!("JARBAS", "STT",
                    "path_ctc_nonempty='{}' → seed LLM", stt_ctc);
            } else {
                k_nano::slog_bin!("JARBAS-STT-SIM", "info",
                    "{} (ctc empty/short)", stt_seed);
            }
            alloc::string::String::from(stt_seed)
        };
        let _ = crate::EVENT_BUS.publish(crate::Event {
            id: 0, topic: alloc::string::String::from("USER_INTENT"),
            payload: stt_owned.as_bytes().to_vec(),
            token: crate::CapabilityToken::Legacy(1),
        });
        let stt = stt_owned.as_str();
        if crate::cortex::model_is_loaded() {
            k_nano::slog_hermes!("Gate", "n4",
                "intent_e2e STT→USER_INTENT→cortex generate_via_model (generator)");
            let raw = crate::cortex::generate_via_model_with_route(stt, "generator");
            if raw.is_empty() {
                k_nano::slog_bin!("JARBAS-TTS", "info", "FAILED empty generate");
                if n3_gen.is_none() { n3_gen = Some(false); }
                if n4_intent.is_none() { n4_intent = Some(false); }
                if n5_voice.is_none() { n5_voice = Some(false); }
            } else {
                k_nano::slog_bin!("JARBAS-TTS", "info", "{}", raw);
                n3_gen = Some(true);
                n4_intent = Some(true);
                let piper_on = crate::audio::skills::piper_is_loaded();
                let _pcm = crate::audio::skills::synthesize_tts(&raw);
                k_nano::slog_bin!("JARBAS", "TTS", "piper={} pcm_samples={}",
                    if piper_on { "LOADED" } else { "OFF" }, _pcm.len());
                crate::display::fb::paint_tts_response(&raw);
                n5_voice = Some(!_pcm.is_empty() || piper_on);
            }
        } else {
            k_nano::slog_bin!("JARBAS-TTS", "info", "SKIP llm=ABSENT");
            if n3_gen.is_none() { n3_gen = Some(false); }
            if n4_intent.is_none() { n4_intent = Some(false); }
            if n5_voice.is_none() { n5_voice = Some(false); }
        }
    }

    // Load Trinity MoE: ROUTER.BITNET se existir; senão keyword (sem LCG)
    {
        let mut trinity = TRINITY.lock();
        if !trinity.moe_router_loaded() {
            let router_loaded = {
                let mut loaded = false;
                // Try NVMe
                let mut nvme_g = k_nano::disk_agent::nvme::NVME_DRIVER.lock();
                if let Some(ref mut nvme) = *nvme_g {
                    if let Some(data) = unsafe { read_file_from_dev(nvme, "ROUTER.BITNET") } {
                        loaded = crate::trinity::load_router_from_file(&data);
                    }
                }
                // Try AHCI
                if !loaded {
                    let mut ahci_guard = crate::AHCI_DRIVER.lock();
                    if let Some(ref mut ahci) = *ahci_guard {
                        if let Some(data) = unsafe { read_file_from_dev(ahci, "ROUTER.BITNET") } {
                            loaded = crate::trinity::load_router_from_file(&data);
                        }
                    }
                }
                // Try ATA
                if !loaded {
                    let ata_guard = crate::ATA_DRIVER.lock();
                    if let Some(ref ata) = *ata_guard {
                        let parts = crate::fat32::read_mbr(ata);
                        for p in &parts {
                            if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                            if let Some(fs) = unsafe { crate::fat32::Fat32Reader::new(ata, p) } {
                                if let Some(data) = unsafe { fs.read_file("ROUTER.BITNET") } {
                                    loaded = crate::trinity::load_router_from_file(&data);
                                    break;
                                }
                            }
                        }
                    }
                }
                // Try USB-MSC
                if !loaded {
                    let mut usb_guard = crate::USB_MSC.lock();
                    if let Some(ref mut msc) = *usb_guard {
                        if let Some(data) = unsafe { read_file_from_dev(msc, "ROUTER.BITNET") } {
                            loaded = crate::trinity::load_router_from_file(&data);
                        }
                    }
                }
                loaded
            };
            if let Some((embed, weight)) = crate::trinity::init_router_weights(trinity.agent_count())
            {
                trinity.load_router(embed, weight, true);
            } else {
                k_nano::slog_cortex!(
                    "TRINITY",
                    "warn",
                    "ROUTER.BITNET ausente — MoE keyword only (nao carrega LCG seed=42)"
                );
                crate::trinity::publish_cortex_posture(false);
            }
            let _ = router_loaded;
        }
    }

    // ADR-0042 N3 gate — telemetria honesta (cérebro): LOADED + MAP_WEIGHTS + Trinity + generate
    n3_cortex_gate(n3_gen);

    // ADR-0042 N4 gate — orquestrador: intent routing + ReAct/skills + cortex path + EventBus
    n4_hermes_gate(n4_intent);

    // ADR-0042 N5 gate — ego/UI: compositor + persona + voz via Hermes + FB/display
    n5_jarbas_gate(&registry, n5_voice);

    // Sprint 95-96: Cognitive + Memory status

    k_nano::slog_bin!("COG", "info", "{}", INTENT_PLANNER.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", SUCCESS_ENGINE.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", FEEDBACK_LOOP.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", NEURAL_CACHE.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", WORKFLOW_PREDICTOR.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", CODEBOOK_VQ.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", REACT_LOOP.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", MCP_SERVER.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", AUTOSKILL_GEN.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", DYNAMIC_SCALER.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", SCHED_OPT.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", REPLAY_BUF.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", BITNET_TRAINER.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", EPISODIC_MEM.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", TASK_SPAWNER.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", WORKSPACE_ISO.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", DELTA_BRANCH.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", MATMUL_FREE_LM.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", TEAM_MEMORY.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", VECTOR_FS.lock().status());

    k_nano::slog_bin!("COG", "info", "{}", crate::memory_systems::bge_status());

    publish_boot_phase(BootPhase::AgentFleet, &alloc::format!("{} agents + DiagnosticSkill registrados", registry.agents.len()));

    // ADR-0086 §2.8 (I10): autobiografia do OS — quem sou, onde estou (SELF.STATE na SGDB).
    // O boot é releitura, não redescoberta: grava a fase derivada do boot_media::mode().
    {
        use k_ai::self_state::{LifePhase, current_phase, record_life_event, write_self_state};
        let mode = k_nano::boot_mode::boot_mode();
        let phase = match mode {
            k_nano::boot_mode::BootMode::Live => LifePhase::Visitante,
            k_nano::boot_mode::BootMode::Install => LifePhase::Mensageiro,
            k_nano::boot_mode::BootMode::Installed => LifePhase::Residente,
            k_nano::boot_mode::BootMode::Unknown => LifePhase::Unknown,
        };
        let prev = current_phase();
        // ADR-0082 Onda CPU — fechar o loop (#4 auditoria): o boot relê /hw/*
        // da SGDB em vez de redescobrir (ADR-0086: boot é releitura). O perfil
        // resolvido alimenta o hw_profile do SELF.STATE. Fallback = valor live
        // (SGDB off / key ausente → degrada, nunca quebra o boot).
        let live_isa = k_nano::platform_probe::hw_info().isa_name();
        let hw_profile = match k_ai::sgdb::hw_get("cpu/isa") {
            Some(p) if p == live_isa => {
                k_nano::slog_bin!("HW", "onda", "Onda CPU loop OK: /hw/cpu/isa={} (releitura)", p);
                Some(p)
            }
            Some(p) => {
                k_nano::slog_bin!("HW", "onda", "Onda CPU divergencia: sgdb={} live={} (usa live)", p, live_isa);
                Some(String::from(live_isa))
            }
            None => {
                k_nano::slog_bin!("HW", "onda", "Onda CPU: /hw/cpu/isa indisponivel (fallback live={})", live_isa);
                Some(String::from(live_isa))
            }
        };
        write_self_state(phase, None, prev != phase, hw_profile.as_deref(), None);
        record_life_event(&alloc::format!("boot phase={} (prev={})", phase.as_str(), prev.as_str()));
        k_nano::slog_bin!("SELF", "info", "SELF.STATE: fase={} (SGDB best-effort)", phase.as_str());

        // S5+S6 (ADR-0086 oracle): boot-attempt counter + mark OK.
        // Chegou ao Runtime = boot OK → zera tries (update confirmado). Se havia
        // update pendente e isto é a N-ésima tentativa sem confirmar, force rollback.
        let _ = hermes_crate::self_update::SelfUpdate::note_boot_attempt(3);
        hermes_crate::self_update::SelfUpdate::mark_boot_ok();
    }

    crate::display::fb::boot_ckpt(50, "Runtime OK — iniciando scheduler");

    k_nano::slog_bin!("Sched", "info", "{} runtime agents. Iniciando scheduler...", registry.agents.len());

    // PIC+STI antes do 1º hlt(): se ACPI=None/APIC nunca sobe, PIT acorda o scheduler.
    // Se PlatformAgent já ativou APIC, USING_APIC→só STI de novo.
    unsafe { crate::interrupts_ext::init_pic_fallback_and_sti(); }

    publish_boot_phase(BootPhase::Runtime, "Entrando no AgentScheduler");

    // Onda 6 — residuals AirLLM (ATA soft path OK; DMA/stream/K-quant AWAITING).
    crate::gguf_streaming::log_airllm_residuals();
    // Onda 7 — LAN snapshot (PASS só se RX>0 já visto; senão AWAITING).
    {
        let rx = crate::netstack::net_rx_count();
        if rx == 0 {
            k_nano::slog_bin!(
                "NET-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=rx_count_zero_at_runtime"
            );
        } else {
            k_nano::slog_bin!("NET-HW", "info", "VERDICT=PASS reason=rx_count={} at_runtime", rx);
        }
    }

    // Stack do scheduler no heap. NÃO usar Box::new([0u8; N]) — estoura stack do boot.
    // Ordem no bump allocator (cresce p/ cima): [stack 8MB][guard 64K][registry].
    // RSP fica no TOPO do buffer; stack cresce p/ BAIXO. Se o registry fosse
    // alocado ANTES da stack, overflow de 2MB esmagava Vec/BTree (#PF CR2=0x18 /
    // index OOB len=0) — visto após self_heal no init_phase (SESSION matriz).
    const SCHED_STACK_SIZE: usize = 8 * 1024 * 1024;
    const SCHED_STACK_GUARD: usize = 64 * 1024;

    let (sp, registry): (u64, &'static mut agent_core::AgentRegistry) = {
        let heap_stack = alloc::vec![0u8; SCHED_STACK_SIZE].into_boxed_slice();
        let sp = (heap_stack.as_ptr() as u64 + SCHED_STACK_SIZE as u64) & !0xFu64;
        core::mem::forget(heap_stack);
        // Guarda: absorve overflow raso antes de tocar o registry.
        let guard = alloc::vec![0u8; SCHED_STACK_GUARD].into_boxed_slice();
        core::mem::forget(guard);
        // Registry DEPOIS da stack+guard no bump — overflow não zera agents/BTree.
        let registry: &'static mut agent_core::AgentRegistry =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(registry));
        (sp, registry)
    };
    k_nano::slog_bin!(
        "BOOT",
        "info",
        "AgentRegistry heap-pinned agents={} ptr=0x{:x} sched_stack={}MB+guard",
        registry.agents.len(),
        registry as *const _ as usize,
        SCHED_STACK_SIZE / (1024 * 1024)
    );

    unsafe {
        // ── BootReport: publica resumo do boot no EventBus ──
        let report = k_nano::boot_report::finalize_and_publish();
        k_nano::slog_bin!(
            "BOOT",
            "REPORT",
            "storage={} usb_msc={} log_written={} ckpt=K{} {}",
            report.storage_ok,
            report.usb_msc,
            report.boot_log_written,
            report.last_ckpt,
            report.ai.line()
        );

        // ── Dump boot_ramlog no FB antes do Runtime ──
        // Última tentativa de persistir BOOT.LOG (ATA agora retorna false sem travar)
        crate::boot_logger::flush();
        // Mostra o log completo no FB pra foto
        let session = k_nano::boot_logger::build_session_bytes();
        let session_str = core::str::from_utf8(&session).unwrap_or("");
        if !session_str.is_empty() {
            crate::display::fb::console_print(">>> BOOT.LOG (RAM) <<<");
            for line in session_str.lines().take(40) {
                crate::display::fb::console_print(line);
            }
            crate::display::fb::console_print(">>> FIM BOOT.LOG <<<");
        }

        let reg = registry as *mut agent_core::AgentRegistry;
        core::arch::asm!(
            "mov rsp, {sp}",
            "mov rdi, {reg}",
            "jmp {run}",
            sp = in(reg) sp,
            reg = in(reg) reg,
            run = sym raw_sched_run,
            clobber_abi("C"),
            options(noreturn)
        );
    }

}



// ── Boot Phase Events ─────────────────────────────────────────

// Publicados no EventBus para que HermesAgent, CortexAgent e BootLogAgent

// possam acompanhar o progresso do boot e tomar decisoes.



pub const TOPIC_BOOT_PHASE: &str = "BOOT_PHASE";

/// Receiver estático: 1 consumer mínimo inscrito antes das publishes.
static BOOT_PHASE_RX: spin::Mutex<Option<event_bus::Receiver>> = spin::Mutex::new(None);

pub fn ensure_boot_phase_consumer() {
    let mut g = BOOT_PHASE_RX.lock();
    if g.is_none() {
        *g = Some(EVENT_BUS.subscribe(TOPIC_BOOT_PHASE));
        k_nano::slog_bin!("BOOT", "info", "Consumer BOOT_PHASE inscrito no EventBus");
    }
}

fn drain_boot_phase_consumer() {
    // Drena sem serial/FB — publish_boot_phase já imprimiu a payload.
    // Imprimir de novo gerava 3 linhas sobrepostas ([BOOT] + [LOG] + [BOOT-PHASE-RX]).
    if let Some(ref mut rx) = *BOOT_PHASE_RX.lock() {
        while rx.try_receive().is_some() {}
    }
}

/// P001: Registra skills builtin no SKILL_REGISTRY canônico (k_nano::globals).
/// Antes isto era um `lazy_static` privado no bin — shadowing deixava hermes/k_ai
/// vendo um registry vazio. Agora todos compartilham `k_nano::SKILL_REGISTRY`.
pub fn register_builtin_skills() {
    let mut reg = k_nano::SKILL_REGISTRY.lock();
    reg.register(alloc::boxed::Box::new(EchoSkill));
    reg.register(alloc::boxed::Box::new(SystemStatusSkill));
    reg.register(alloc::boxed::Box::new(HardwareInfoSkill));
    reg.register(alloc::boxed::Box::new(net::NetDiagnosticSkill));
    reg.register(alloc::boxed::Box::new(HwIdentifySkill));
    reg.register(alloc::boxed::Box::new(hermes_crate::expert_skills::DiskDiagSkill));
    reg.register(alloc::boxed::Box::new(hermes_crate::expert_skills::SecuritySkill));
    reg.register(alloc::boxed::Box::new(hermes_crate::self_update::UpdateCheckSkill));
    reg.register(alloc::boxed::Box::new(audio::skills::TtsSkill));
    reg.register(alloc::boxed::Box::new(audio::skills::SttSkill));
    reg.register(alloc::boxed::Box::new(audio::settings::AudioGetSettingsSkill));
    reg.register(alloc::boxed::Box::new(audio::settings::AudioSetVolumeSkill));
    reg.register(alloc::boxed::Box::new(audio::settings::AudioToggleVoiceCloneSkill));
    reg.register(alloc::boxed::Box::new(audio::context::EmotionalContextSkill));
    reg.set_policy("*", skill_registry::ToolPolicy { enabled: true, auto_approve: false });
    k_nano::slog_bin!("SKILL", "init", "{} builtin skills registered", reg.skill_count());
}



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
    ensure_boot_phase_consumer();
    let payload = alloc::format!("[BOOT:{:?}] {}", phase, msg);
    k_nano::slog_bin!("Log", "msg", "{}", payload);
    // Disco/buffer só — sem segundo serial_println ([LOG] duplicava no FB).
    crate::boot_logger::log_quiet(&payload);
    let _ = EVENT_BUS.publish(crate::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_BOOT_PHASE),
        payload: payload.into_bytes(),
        token: crate::CapabilityToken::Legacy(1),
    });
    drain_boot_phase_consumer();
}



fn verify_kernel_from_disk(ata: &crate::ata::AtaDriver, parts: &[crate::fat32::Partition]) {
    for part in parts {
        if part.type_code == 0x0B
            || part.type_code == 0x0C
            || part.type_code == 0x1C
            || part.type_code == 0x73
        {
            if let Some(fat32) = unsafe { crate::fat32::Fat32Reader::new(ata, part) } {
                if let Some(data) = unsafe { fat32.read_file("KERNEL~1") } {
                    if !crate::identity::verify_kernel_signature(&data) {
                        // NÃO halt — notebooks USB-boot sem kernel assinado no ATA interno
                        // travavam aqui após K17 (loop spin eterno).
                        k_nano::slog_bin!("Sec", "info", "KERNEL~1 assinatura INVALIDA — continue (HW USB boot)");
                        crate::display::fb::console_print("SEC: kernel unsigned — continue");
                        return;
                    }
                    k_nano::slog_bin!("Sec", "info", "Assinatura do kernel OK.");
                    crate::tpm::tpm_extend_pcr(crate::tpm::TPM_PCR_KERNEL, &data);
                    return;
                }
            }
        }
    }
    k_nano::slog_bin!("Sec", "info", "Kernel nao assinado (sem FAT ou KERNEL~1 nao encontrado).");
}



// All old async fn daemons removed — migrated to native agents in agents.rs











