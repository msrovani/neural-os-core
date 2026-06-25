use crate::{println, serial_println};
use alloc::vec::Vec;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub bar3: u32,
    pub bar4: u32,
    pub bar5: u32,
}

unsafe fn read_config_dword(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | (offset as u32 & 0xFC);
    core::arch::asm!(
        "out dx, eax",
        in("dx") CONFIG_ADDRESS,
        in("eax") address,
        options(nostack, preserves_flags)
    );
    let mut value: u32;
    core::arch::asm!(
        "in eax, dx",
        in("dx") CONFIG_DATA,
        out("eax") value,
        options(nostack, preserves_flags)
    );
    value
}

unsafe fn read_config_word(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let dword = read_config_dword(bus, device, function, offset & 0xFC);
    ((dword >> ((offset as u32 & 2) * 8)) & 0xFFFF) as u16
}

unsafe fn read_config_byte(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let dword = read_config_dword(bus, device, function, offset & 0xFC);
    ((dword >> ((offset as u32 & 3) * 8)) & 0xFF) as u8
}

unsafe fn read_bar(bus: u8, device: u8, function: u8, bar_index: u8) -> u32 {
    let offset = 0x10 + bar_index * 4;
    read_config_dword(bus, device, function, offset)
}

pub unsafe fn scan_pci() -> Vec<PciDevice> {
    let mut devices = Vec::new();
    for bus in 0..=255u8 {
        for device in 0..=31u8 {
            let vendor_id = read_config_word(bus, device, 0, 0x00);
            if vendor_id != 0xFFFF && vendor_id != 0x0000 {
                let device_id = read_config_word(bus, device, 0, 0x02);
                let class_raw = read_config_word(bus, device, 0, 0x0A);
                let class = (class_raw >> 8) as u8;
                let subclass = (class_raw & 0xFF) as u8;
                let prog_if = (read_config_word(bus, device, 0, 0x08) >> 8) as u8;
                let bar0 = read_bar(bus, device, 0, 0);
                let bar1 = read_bar(bus, device, 0, 1);
                let bar2 = read_bar(bus, device, 0, 2);
                let bar3 = read_bar(bus, device, 0, 3);
                let bar4 = read_bar(bus, device, 0, 4);
                let bar5 = read_bar(bus, device, 0, 5);
                devices.push(PciDevice {
                    bus, device, function: 0,
                    vendor_id, device_id, class, subclass, prog_if,
                    bar0, bar1, bar2, bar3, bar4, bar5,
                });
                if class == 0x06 && subclass == 0x04 {
                    // Read secondary bus number from bridge config space offset 0x19
                    let sec_bus = read_config_byte(bus, device, 0, 0x19);
                    if sec_bus != 0 {
                        for function in 0..=7u8 {
                            let vf = read_config_word(sec_bus, device, function, 0x00);
                            if vf != 0xFFFF && vf != 0x0000 {
                                let df = read_config_word(sec_bus, device, function, 0x02);
                                let bar0 = read_bar(sec_bus, device, function, 0);
                                devices.push(PciDevice {
                                    bus: sec_bus, device, function,
                                    vendor_id: vf, device_id: df,
                                    class, subclass, prog_if: 0,
                                    bar0, bar1: 0, bar2: 0,
                                    bar3: 0, bar4: 0, bar5: 0,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    devices
}

pub unsafe fn init_pci() -> Vec<PciDevice> {
    let devices = scan_pci();
    serial_println!("[PCI] Scan concluido: {} dispositivos encontrados.", devices.len());
    println!("[PCI] Scan concluido: {} dispositivos encontrados.", devices.len());
    for dev in &devices {
        serial_println!(
            "[PCI] {:02x}:{:02x}.{:02x} {:04x}:{:04x} class={:02x} subclass={:02x}",
            dev.bus, dev.device, dev.function,
            dev.vendor_id, dev.device_id, dev.class, dev.subclass
        );
    }
    devices
}

pub fn class_name(class: u8, subclass: u8) -> &'static str {
    match (class, subclass) {
        (0x00, _) => "Unclassified",
        (0x01, 0x08) => "NVMe",
        (0x01, _) => "Mass Storage",
        (0x02, _) => "Network",
        (0x03, 0x00) => "VGA Controller",
        (0x03, _) => "Display",
        (0x04, _) => "Audio",
        (0x06, 0x00) => "Host Bridge",
        (0x06, 0x04) => "PCI Bridge",
        (0x06, _) => "Bridge",
        (0x0C, 0x03) => "USB 3.0 (xHCI)",
        (0x0C, _) => "Serial Bus",
        (0x08, 0x00) => "PIC",
        (0x08, _) => "Generic System",
        _ => "Other",
    }
}
