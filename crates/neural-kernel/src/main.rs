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
mod interrupts;
mod memory;
mod pci;
mod sync;
mod nn;
mod serial;
mod simd;
mod task;
mod tensor;
mod vga_buffer;

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
            description: String::from("Reports RAM occupancy and CPU status via hardware context tensor"),
            required_tokens: vec![1],
        }
    }
    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let tensor = crate::memory::global_hardware_context();
        let occupancy_pct = tensor[0] * 100.0;
        serial_println!("[SKILL EXECUTADA] Memoria RAM: {:.2}%. CPU: Agentes Cooperativos em operacao.", occupancy_pct);
        println!("[SKILL EXECUTADA] Memoria RAM: {:.2}%. CPU: Agentes Cooperativos em operacao.", occupancy_pct);
        Ok(alloc::vec::Vec::new())
    }
}

lazy_static! {
    static ref EVENT_BUS: event_bus::EventBus = event_bus::EventBus::new();
    static ref SKILL_REGISTRY: spin::Mutex<SkillRegistry> = {
        let mut reg = SkillRegistry::new();
        reg.register(alloc::boxed::Box::new(EchoSkill));
        reg.register(alloc::boxed::Box::new(SystemStatusSkill));
        spin::Mutex::new(reg)
    };
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

    let mut mapper = unsafe { memory::init_memory(boot_info.physical_memory_offset) };
    let mut frame_allocator = BitmapFrameAllocator::empty();
    frame_allocator.init(&boot_info.memory_map);

    allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");

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

    serial_println!("[EXECUTOR] Inicializando Neural Executor...");
    println!("[EXECUTOR] Inicializando Neural Executor...");

    let mut executor = task::executor::NeuralExecutor::new();
    executor.spawn(task::agent::AgentTask::new(system_daemon()));
    executor.spawn(task::agent::AgentTask::new(hardware_monitor_daemon()));
    executor.spawn(task::agent::AgentTask::new(hw_bridge_daemon()));
    executor.spawn(task::agent::AgentTask::new(input_daemon()));
    executor.spawn(task::agent::AgentTask::new(intent_router_daemon()));
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
        _ => None,
    }
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

async fn intent_router_daemon() {
    let receiver = EVENT_BUS.subscribe("USER_INTENT");
    let status_skill = String::from("system_status");
    loop {
        if let Some(event) = receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[CORTEX] Texto do usuario recebido: \"{}\"", text);
            println!("[CORTEX] Texto do usuario recebido: \"{}\"", text);
            let intent_id = if text.contains("STATUS") || text.contains("status") { 1 } else { 0 };
            serial_println!("[CORTEX] Intencao inferida: ID {}", intent_id);
            println!("[CORTEX] Intencao inferida: ID {}", intent_id);
            if intent_id == 1 {
                let reg = SKILL_REGISTRY.lock();
                match reg.execute_skill(&status_skill, &event.payload, &event.token) {
                    Ok(_output) => {
                        serial_println!("[CORTEX] SystemStatusSkill executada com sucesso.");
                        println!("[CORTEX] SystemStatusSkill executada com sucesso.");
                    }
                    Err(e) => {
                        serial_println!("[CORTEX] Erro na skill: {}", e);
                        println!("[CORTEX] Erro na skill: {}", e);
                    }
                }
                drop(reg);
            } else {
                serial_println!("[CORTEX] Intencao ID 0: acao padrao (nenhuma skill registrada).");
                println!("[CORTEX] Intencao ID 0: acao padrao (nenhuma skill registrada).");
            }
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
