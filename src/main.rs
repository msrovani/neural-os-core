#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

fn serial_init() {
    let port = 0x3F8u16;
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port + 1, in("al") 0x00i8);
        core::arch::asm!("out dx, al", in("dx") port + 3, in("al") 0x80u8 as i8);
        core::arch::asm!("out dx, al", in("dx") port + 0, in("al") 0x03i8);
        core::arch::asm!("out dx, al", in("dx") port + 1, in("al") 0x00i8);
        core::arch::asm!("out dx, al", in("dx") port + 3, in("al") 0x03i8);
        core::arch::asm!("out dx, al", in("dx") port + 2, in("al") 0xC7u8 as i8);
        core::arch::asm!("out dx, al", in("dx") port + 4, in("al") 0x0Bi8);
    }
}

fn serial_write_byte(byte: u8) {
    let port = 0x3F8u16;
    unsafe {
        loop {
            let status: i8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") port + 5);
            if status & 0x20i8 != 0 {
                break;
            }
        }
        core::arch::asm!("out dx, al", in("dx") port, in("al") byte as i8);
    }
}

fn serial_write(msg: &[u8]) {
    for &byte in msg {
        serial_write_byte(byte);
    }
}

#[no_mangle]
pub extern "C" fn _start(_boot_info: &()) -> ! {
    serial_init();
    let message = b"[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.\n";
    serial_write(message);
    loop {}
}
