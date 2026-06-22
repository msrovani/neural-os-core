#![no_std]
#![no_main]

use bootloader::bootinfo::BootInfo;

mod serial;
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

    println!("[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.");
    serial_println!("[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.");

    loop {}
}
