use alloc::string::String;
use alloc::vec::Vec;
use crate::apps::App;

pub struct PowerApp {
    confirmed: bool,
}

impl PowerApp {
    pub fn new() -> Self { PowerApp { confirmed: false } }
}

impl App for PowerApp {
    fn name(&self) -> &str { "power" }
    fn icon_hint(&self) -> &str { "power button circle" }
    fn window_size(&self) -> (u32, u32) { (300, 160) }

    fn on_click(&mut self, x: i32, y: i32) -> Option<String> {
        if y >= 40 && y < 80 {
            if x >= 20 && x < 140 {
                if self.confirmed {
                    crate::serial_println!("[POWER] Shutdown...");
                    unsafe { core::arch::asm!("out dx, al", in("dx") 0x604u16, in("al") 0x10u8, options(nostack)); }
                    return Some(String::from("Shutting down..."));
                }
                self.confirmed = true;
                return Some(String::from("Click again to confirm"));
            }
            if x >= 160 && x < 280 {
                crate::serial_println!("[POWER] Reboot...");
                unsafe { x86_64::instructions::port::Port::new(0x64u16).write(0xFEu8); }
                return Some(String::from("Rebooting..."));
            }
        }
        self.confirmed = false;
        None
    }

    fn render(&self) -> &[u8] { &[] }
}
