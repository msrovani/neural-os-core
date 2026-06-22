#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use bootloader::bootinfo::BootInfo;

mod allocator;
mod interrupts;
mod memory;
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
    let mut frame_allocator = unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

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

    loop {}
}
