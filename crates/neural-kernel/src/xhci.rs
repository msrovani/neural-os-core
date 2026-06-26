use alloc::vec::Vec;
use crate::pci::PciDevice;
use crate::memory::PHYS_MEM_OFFSET;
use crate::serial_println;
use core::sync::atomic::Ordering;

pub struct UsbDevice {
    pub port: u8,
    pub speed: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}

pub struct XhciDriver {
    op: u64,
    pub ports: u8,
    pub slots: u8,
}

impl XhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.class != 0x0C || dev.subclass != 0x03 { return None; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let mmio = ((dev.bar0 & !0xF) as u64) + pmoff;
        let op = mmio + core::ptr::read_volatile(mmio as *const u8) as u64;
        let hcs1 = core::ptr::read_volatile((mmio + 4) as *const u32);
        serial_println!("[XHCI] Controller: {} portas, {} slots", hcs1 & 0xFF, (hcs1 >> 8) & 0xFF);
        Some(XhciDriver { op, ports: (hcs1 & 0xFF) as u8, slots: ((hcs1 >> 8) & 0xFF) as u8 })
    }

    pub unsafe fn init(&mut self) -> bool {
        serial_println!("[XHCI] Init OK: {} portas disponiveis", self.ports);
        true
    }

    pub unsafe fn port_scan(&self) -> Vec<UsbDevice> {
        let mut devices = Vec::new();
        for port in 0..self.ports.min(8) {
            let portsc = core::ptr::read_volatile((self.op + 0x400 + port as u64 * 0x10) as *const u32);
            if portsc & 0x01 == 0 { continue; }
            let speed = ((portsc >> 20) & 0x0F) as u8;
            let speed_name = match speed {
                1 => "Low Speed (1.5 Mbps) — teclado/mouse",
                2 => "Full Speed (12 Mbps) — USB 1.x",
                3 => "High Speed (480 Mbps) — USB 2.0 pendrive",
                4 => "Super Speed (5 Gbps) — USB 3.0",
                5 => "Super Speed+ (10 Gbps) — USB 3.1",
                _ => "Unknown",
            };
            serial_println!("[USB] Porta {}: {} conectado — {}", port, speed_name, self.speed_desc(speed));
            // TODO: xHCI TRB-based Get Descriptor for VID/PID — Sprint 31
            devices.push(UsbDevice { port, speed, vendor_id: 0, product_id: 0 });
        }
        devices
    }

    fn speed_desc(&self, speed: u8) -> &'static str {
        match speed {
            1 => "dispositivo USB 1.x (teclado, mouse, baixa velocidade)",
            2 => "dispositivo USB 2.0 full speed",
            3 => "dispositivo USB 2.0 high speed (pendrive, camera)",
            4 => "dispositivo USB 3.0 super speed (SSD externo, hub SS)",
            5 => "dispositivo USB 3.1 super speed+ (10 Gbps)",
            _ => "dispositivo USB de velocidade desconhecida",
        }
    }
}

