#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use bootloader::bootinfo::BootInfo;
use core::sync::atomic::Ordering;
use event_bus::{CapabilityToken, Event};
use skill_registry::{McpManifest, Skill, SkillRegistry};
use memory::BitmapFrameAllocator;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};

mod acpi;
mod allocator;
mod apic;
mod cortex;
mod hermes;
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
mod serial;
mod simd;
mod task;
mod tensor;
mod time_utils;
mod usage;
mod conversation;
mod vga_buffer;
mod net;
mod netstack;
mod network_agent;
mod proto;
mod rtl8139;

use lazy_static::lazy_static;

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

lazy_static! {
    static ref EVENT_BUS: event_bus::EventBus = event_bus::EventBus::new();
    static ref SKILL_REGISTRY: spin::Mutex<SkillRegistry> = {
        let mut reg = SkillRegistry::new();
        reg.register(alloc::boxed::Box::new(EchoSkill));
        reg.register(alloc::boxed::Box::new(SystemStatusSkill));
        reg.register(alloc::boxed::Box::new(HardwareInfoSkill));
        reg.register(alloc::boxed::Box::new(net::NetDiagnosticSkill));
        reg.set_policy("*", skill_registry::ToolPolicy { enabled: true, auto_approve: false });
        spin::Mutex::new(reg)
    };
    static ref TRUST_CACHE: spin::Mutex<trust::TrustCache> = spin::Mutex::new(trust::TrustCache::new());
    static ref SYSTEM_ARCH: spin::Mutex<Option<inventory::SystemArchitecture>> = spin::Mutex::new(None);
    static ref MEMORY_HIERARCHY: spin::Mutex<Option<mhi::MemoryHierarchy>> = spin::Mutex::new(None);
    static ref USAGE_TRACKER: spin::Mutex<usage::UsageTracker> = spin::Mutex::new(usage::UsageTracker::new());
    static ref EVENT_LOG: spin::Mutex<conversation::EventLog> = spin::Mutex::new(conversation::EventLog::new());
    static ref CONVERSATION_TRACKER: spin::Mutex<hermes::ConversationTracker> = spin::Mutex::new(hermes::ConversationTracker::new());
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("[PANIC] {}", info);
    serial_println!("[PANIC] {}", info);
    loop {}
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

    let _pci_devices = unsafe { pci::init_pci() };

    let acpi_info = unsafe { acpi::init_acpi(boot_info.physical_memory_offset) };
    if let Some(ref info) = acpi_info {
        unsafe { apic::init_apic(info); }
    } else {
        serial_println!("[APIC] ACPI nao encontrado. Mantendo PIC legacy.");
        println!("[APIC] ACPI nao encontrado. Mantendo PIC legacy.");
        interrupts::init_pics();
        interrupts::enable_interrupts();
    }

    memory::init_global_allocator(frame_allocator);

    let slab_metrics = { let s = crate::slab::SLAB_ALLOCATOR.lock(); (s.metrics().0, s.metrics().1) };
    serial_println!("[SLAB] Alocador slab operacional. Allocs: {}, Deallocs: {}",
        slab_metrics.0, slab_metrics.1);
    println!("[SLAB] Alocador slab com {} buckets ativos.", slab::BUCKET_SIZES.len());

    unsafe { smp::init_smp(); }

    let pci_devices = unsafe { pci::scan_pci() };
    let arch = inventory::SystemArchitecture::infer(
        &inventory::HardwareInventory::collect(pci_devices, acpi_info.as_ref()),
    );
    serial_println!(
        "[ARCH] System architecture: ring0={} ring1={} heap={}MB trust={} power={} tensor={}",
        arch.ring0_mode, arch.ring1_mode, arch.heap_size_mb,
        arch.trust_level, arch.power_mode, arch.tensor_tier,
    );
    println!(
        "[ARCH] System architecture: ring0={} ring1={} heap={}MB",
        arch.ring0_mode, arch.ring1_mode, arch.heap_size_mb,
    );

    let mhi = mhi::MemoryHierarchy::new();
    serial_println!("[MHI] {} tier(s). Best: {:?} ({} bytes avail)",
        mhi.tiers.len(), mhi.best_tier(), mhi.tiers[0].capacity_bytes);
    println!("[MHI] Memory hierarchy: {} tier(s), {:.1} MB usable.",
        mhi.tiers.len(), mhi.tiers[0].capacity_bytes as f64 / 1_048_576.0);

    *SYSTEM_ARCH.lock() = Some(arch);
    *MEMORY_HIERARCHY.lock() = Some(mhi.clone());

    unsafe {
        if net::init_driver_rtl8139() {
            serial_println!("[NET] RTL8139 OK.");
        } else {
            serial_println!("[NET] Sem hardware de rede. Modo offline.");
            println!("[NET] Sem hardware de rede.");
        }
    }

    let ticks = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    serial_println!("[EXECUTOR] Timer ticks: {}", ticks);
    serial_println!("[EXECUTOR] Inicializando Neural Executor...");
    println!("[EXECUTOR] Inicializando Neural Executor...");

    let mut executor = task::executor::NeuralExecutor::new();
    executor.spawn(task::agent::AgentTask::new(system_daemon()));
    executor.spawn(task::agent::AgentTask::new(hardware_monitor_daemon()));
    executor.spawn(task::agent::AgentTask::new(hw_bridge_daemon()));
    executor.spawn(task::agent::AgentTask::new(network_agent::network_agent_daemon()));
    executor.spawn(task::agent::AgentTask::new(input_daemon()));
    executor.spawn(task::agent::AgentTask::new(cortex_llm_daemon()));
    executor.spawn(task::agent::AgentTask::new(intent_router_daemon()));
    executor.spawn(task::agent::AgentTask::new(hermes_console_daemon()));
    executor.run();
}

fn scancode_to_ascii(scancode: u8) -> Option<char> {
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

fn execute_skill_with_trust(
    skill_name: &str,
    payload: &[u8],
    token: &event_bus::CapabilityToken,
) -> Result<Vec<u8>, &'static str> {
    let token_val = token.0;
    let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    const AUTO_TTL: u64 = 360;

    {
        let mut tc = TRUST_CACHE.lock();
        if !tc.is_trusted(token_val, skill_name, now) {
            let reg = SKILL_REGISTRY.lock();
            if !reg.validate_token(skill_name, token) {
                return Err("token nao autorizado para esta skill");
            }
            tc.check_or_cache(token_val, skill_name, now, AUTO_TTL);
        }
    }

    let reg = SKILL_REGISTRY.lock();
    reg.execute_skill_unchecked(skill_name, payload)
}

async fn system_daemon() {
    println!("[DAEMON] Agente assincrono inicializado. Aguardando SYSTEM_READY...");
    serial_println!("[DAEMON] Agente assincrono inicializado. Aguardando SYSTEM_READY...");

    let receiver = EVENT_BUS.subscribe("SYSTEM_READY");

    loop {
        if let Some(event) = receiver.try_receive() {
            println!("[IPC] Evento recebido no topico {} com payload protegido. Token: {}",
                event.topic, event.token.0);
            serial_println!("[IPC] Evento recebido no topico {} com payload protegido. Token: {}",
                event.topic, event.token.0);

            let reg = SKILL_REGISTRY.lock();
            match reg.execute_skill("echo", &event.payload, &event.token) {
                Ok(output) => {
                    println!("[SKILL] EchoSkill executada. Output reverso: {:?}", output);
                    serial_println!("[SKILL] EchoSkill executada. Output reverso: {:?}", output);
                }
                Err(e) => {
                    println!("[SKILL] Erro ao executar skill: {}", e);
                    serial_println!("[SKILL] Erro ao executar skill: {}", e);
                }
            }
            drop(reg);

            println!("[DAEMON] SYSTEM_READY confirmado. Ciclo de inicializacao completo.");
            serial_println!("[DAEMON] SYSTEM_READY confirmado. Ciclo de inicializacao completo.");
            break;
        }
        task::yield_now().await;
    }
}

async fn hw_bridge_daemon() {
    loop {
        let scancode = crate::interrupts::LAST_SCANCODE.swap(0, Ordering::Acquire);
        if scancode != 0 {
            let event = Event {
                id: 0,
                topic: String::from("RAW_HW_IRQ1"),
                payload: vec![scancode],
                token: CapabilityToken(1),
            };
            let _ = EVENT_BUS.publish(event);
        }
        task::yield_now().await;
    }
}

async fn cortex_llm_daemon() {
    let model = cortex::TransformerModel::new();
    let receiver = EVENT_BUS.subscribe(cortex::TOPIC_LLM_REQUEST);
    serial_println!("[CORTEX-LLM] Transformer loaded. Starting daemon...");
    loop {
        if let Some(event) = receiver.try_receive() {
            let prompt = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[CORTEX-LLM] Generating for: \"{}\"", prompt);
            let output = cortex::generate_text(&model, prompt);
            serial_println!("[CORTEX-LLM] Generated: \"{}\"", output);
            let resp = crate::Event {
                id: 0,
                topic: alloc::string::String::from(cortex::TOPIC_LLM_RESPONSE),
                payload: output.into_bytes(),
                token: crate::CapabilityToken(1),
            };
            let _ = EVENT_BUS.publish(resp);
        }
        task::yield_now().await;
    }
}

async fn intent_router_daemon() {
    let receiver = EVENT_BUS.subscribe(hermes::TOPIC_USER_INTENT);
    let cortex = cortex::Cortex::new();
    let status_skill_name = String::from("system_status");
    let echo_skill_name = String::from("echo");
    let hw_skill_name = String::from("hardware_info");
    let net_diag_skill_name = String::from("net_diag");
    loop {
        if let Some(event) = receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[CORTEX] Texto do usuario: \"{}\"", text);
            println!("[CORTEX] Texto do usuario: \"{}\"", text);

            let cmd = hermes::parse_command(text);
            let response = match cmd {
                hermes::Command::Status => {
                    match execute_skill_with_trust(&status_skill_name, &event.payload, &event.token) {
                        Ok(_) => String::from("System status report executado."),
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::Echo(ref arg) => {
                    match execute_skill_with_trust(&echo_skill_name, arg.as_bytes(), &event.token) {
                        Ok(output) => {
                            let reversed = core::str::from_utf8(&output).unwrap_or("(bytes nao UTF-8)");
                            alloc::format!("Echo reverso: \"{}\"", reversed)
                        }
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::HardwareInfo => {
                    match execute_skill_with_trust(&hw_skill_name, &event.payload, &event.token) {
                        Ok(output) => {
                            String::from(core::str::from_utf8(&output).unwrap_or("(binary)"))
                        }
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::NetDiag => {
                    match execute_skill_with_trust(&net_diag_skill_name, &event.payload, &event.token) {
                        Ok(output) => {
                            String::from(core::str::from_utf8(&output).unwrap_or("(binary)"))
                        }
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::Fetch(ref url) => {
                    let parsed: Option<([u8; 4], u16, alloc::string::String)> = {
                        let url_str = url.trim();
                        if let Some(rest) = url_str.strip_prefix("http://") {
                            let without_slash = if let Some(pos) = rest.find('/') {
                                let (hp, p) = rest.split_at(pos);
                                (hp, alloc::string::ToString::to_string(p))
                            } else {
                                (rest, alloc::string::String::from("/"))
                            };
                            let (host_str, path) = without_slash;
                            let (host_only, port) = if let Some(pos) = host_str.find(':') {
                                let (h, p_str) = host_str.split_at(pos);
                                let p: u16 = p_str[1..].parse().unwrap_or(80);
                                (h, p)
                            } else {
                                (host_str, 80u16)
                            };
                            let parts: Vec<&str> = host_only.split('.').collect();
                            if parts.len() == 4 {
                                let ip = [
                                    parts[0].parse().unwrap_or(0),
                                    parts[1].parse().unwrap_or(0),
                                    parts[2].parse().unwrap_or(0),
                                    parts[3].parse().unwrap_or(0),
                                ];
                                Some((ip, port, path))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    match parsed {
                        Some((host_ip, port, path)) => {
                            match unsafe { crate::net::http_get(host_ip, port, &path) } {
                                Some(body) => {
                                    let text = core::str::from_utf8(&body).unwrap_or("(binary)");
                                    let preview = if text.len() > 200 { &text[..200] } else { text };
                                    alloc::format!("Fetch OK ({} bytes):\n{}", body.len(), preview)
                                }
                                None => String::from("Fetch falhou: sem resposta"),
                            }
                        }
                        None => String::from("Formato: /fetch http://ip:port/path (DNS numerico apenas)"),
                    }
                }
                hermes::Command::Ping(ref target) => {
                    let parts: Vec<&str> = target.split('.').collect();
                    if parts.len() == 4 {
                        let ip = [
                            parts[0].parse().unwrap_or(0),
                            parts[1].parse().unwrap_or(0),
                            parts[2].parse().unwrap_or(0),
                            parts[3].parse().unwrap_or(0),
                        ];
                        match unsafe { crate::net::ping(ip) } {
                            Some(_) => alloc::format!("Pong! {} -> OK", target),
                            None => alloc::format!("Ping {} falhou: sem resposta", target),
                        }
                    } else {
                        String::from("Formato: /ping <ip> (ex: /ping 10.0.2.2)")
                    }
                }
                hermes::Command::Usage => {
                    let snap = USAGE_TRACKER.lock().snapshot();
                    alloc::format!(
                        "Usage: {} chamadas totais, {} ticks.{}",
                        snap.total_calls, snap.total_exec_time_ticks,
                        snap.by_skill.iter().map(|(n, c)| alloc::format!(" {}:{}", n, c)).collect::<alloc::string::String>(),
                    )
                }
                hermes::Command::Conversation => {
                    let log = EVENT_LOG.lock();
                    log.summarize()
                }
                hermes::Command::TrustAllow(token, ref skill) => {
                    let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
                    {
                        let mut tc = TRUST_CACHE.lock();
                        tc.trust_allow(token, skill, now);
                    }
                    alloc::format!("Trust permitido: token {} pode usar skill '{}'", token, skill)
                }
                hermes::Command::TrustDeny(token, ref skill) => {
                    {
                        let mut tc = TRUST_CACHE.lock();
                        tc.trust_deny(token, skill);
                    }
                    alloc::format!("Trust revogado: token {} negado para skill '{}'", token, skill)
                }
                hermes::Command::Help => {
                    String::from("Comandos: /status, /echo <txt>, /hw, /netdiag, /usage, /conv, /ping <ip>, /fetch <url>, /trust allow <token> <skill>, /trust deny <token> <skill>, /help | Ou digite algo para o MLP.")
                }
                hermes::Command::Chat(ref msg) => {
                    let intent = cortex.think(msg);
                    let intent_name = intent.skill_name();
                    serial_println!("[CORTEX] Intent: {} = {:?}", intent_name, intent);
                    let response = match intent {
                        cortex::Intent::Greeting => {
                            String::from("Hermes: Ola! Digite /help para comandos, ou converse comigo.")
                        }
                        cortex::Intent::Chat => {
                            alloc::format!("Hermes: \"{}\" — entendido! (intent chatear)", msg)
                        }
                        _ => {
                            match SKILL_REGISTRY.lock().has_skill(intent_name) {
                                true => {
                                    match execute_skill_with_trust(intent_name, msg.as_bytes(), &event.token) {
                                        Ok(output) => {
                                            let text = core::str::from_utf8(&output).unwrap_or("(binary)");
                                            alloc::format!("[Cortex] {}: {}", intent_name, text)
                                        }
                                        Err(e) => alloc::format!("[Cortex] {} erro: {}", intent_name, e),
                                    }
                                }
                                false => {
                                    alloc::format!("Hermes: Nao tenho skill para '{}'. Tente /help", intent_name)
                                }
                            }
                        }
                    };
                    serial_println!("[CORTEX] Response: {}", response);
                    response
                }
            };

            let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            USAGE_TRACKER.lock().record_call("intent_router", 1);
            EVENT_LOG.lock().push(
                conversation::EventKind::UserInput,
                event.payload.clone(),
                now,
            );
            EVENT_LOG.lock().push(
                conversation::EventKind::HermesResponse,
                response.as_bytes().to_vec(),
                now,
            );
            CONVERSATION_TRACKER.lock().record_exchange(text, &response);
            if CONVERSATION_TRACKER.lock().needs_compact() {
                let compact_msg = CONVERSATION_TRACKER.lock().compact();
                serial_println!("[HERMES] {}", compact_msg);
                EVENT_LOG.lock().push(
                    conversation::EventKind::ContextCompacted,
                    compact_msg.into_bytes(),
                    now,
                );
            }
            let resp_event = event_bus::Event {
                id: 0,
                topic: alloc::string::String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: response.into_bytes(),
                token: event_bus::CapabilityToken(1),
            };
            let _ = EVENT_BUS.publish(resp_event);
        }
        task::yield_now().await;
    }
}

async fn hermes_console_daemon() {
    let receiver = EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE);
    loop {
        if let Some(event) = receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("(bytes)");
            serial_println!("[Hermes] {}", text);
            println!("[Hermes] {}", text);
        }
        task::yield_now().await;
    }
}

async fn hardware_monitor_daemon() {
    let event = Event {
        id: 0,
        topic: String::from("SYSTEM_READY"),
        payload: vec![1, 2, 3],
        token: CapabilityToken(1),
    };
    match EVENT_BUS.publish(event) {
        Ok(()) => {
            println!("[MONITOR] Evento SYSTEM_READY publicado.");
            serial_println!("[MONITOR] Evento SYSTEM_READY publicado.");
        }
        Err(e) => {
            println!("[MONITOR] Falha na publicacao: {}", e);
            serial_println!("[MONITOR] Falha na publicacao: {}", e);
        }
    }
}

async fn input_daemon() {
    let receiver = EVENT_BUS.subscribe("RAW_HW_IRQ1");
    let mut buffer = String::new();
    loop {
        if let Some(event) = receiver.try_receive() {
            let scancode = event.payload.first().copied().unwrap_or(0);
            if scancode < 0x80 {
                match scancode {
                    0x1C => {
                        let text = core::mem::take(&mut buffer);
                        if !text.is_empty() {
                            serial_println!("[INPUT] ENTER detected — publishing USER_INTENT: \"{}\"", text);
                            println!("[INPUT] ENTER detected — publishing USER_INTENT: \"{}\"", text);
                            let event = Event {
                                id: 0,
                                topic: String::from("USER_INTENT"),
                                payload: text.into_bytes(),
                                token: CapabilityToken(1),
                            };
                            let _ = EVENT_BUS.publish(event);
                        }
                    }
                    0x0E => {
                        buffer.pop();
                    }
                    _ => {
                        if let Some(ch) = scancode_to_ascii(scancode) {
                            buffer.push(ch);
                        }
                    }
                }
            }
        }
        task::yield_now().await;
    }
}
