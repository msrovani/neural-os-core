#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use bootloader::bootinfo::BootInfo;
use memory::BitmapFrameAllocator;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};

mod allocator;
mod interrupts;
mod memory;
mod nn;
mod serial;
mod simd;
mod tensor;
mod vga_buffer;

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

    interrupts::init_pics();
    interrupts::enable_interrupts();

    loop {
        x86_64::instructions::hlt();
        let ticks = interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if ticks > 0 && ticks % 100 == 0 {
            serial_println!("[WATCHDOG] Ticks do temporizador: {}", ticks);
        }
    }
}
