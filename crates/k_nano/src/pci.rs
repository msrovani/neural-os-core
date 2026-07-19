use crate::{kjson};
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

pub unsafe fn read_config_dword(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
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

pub unsafe fn read_config_word(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let dword = read_config_dword(bus, device, function, offset & 0xFC);
    ((dword >> ((offset as u32 & 2) * 8)) & 0xFFFF) as u16
}

pub unsafe fn read_config_byte(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let dword = read_config_dword(bus, device, function, offset & 0xFC);
    ((dword >> ((offset as u32 & 3) * 8)) & 0xFF) as u8
}

unsafe fn read_bar(bus: u8, device: u8, function: u8, bar_index: u8) -> u32 {
    let offset = 0x10 + bar_index * 4;
    read_config_dword(bus, device, function, offset)
}

/// Escaneia recursivamente um barramento e retorna seus dispositivos.
unsafe fn scan_bus(bus: u8, visited: &mut alloc::vec::Vec<u8>) -> alloc::vec::Vec<PciDevice> {
    let mut devices = alloc::vec::Vec::new();
    if visited.contains(&bus) { return devices; }
    visited.push(bus);

    for device in 0..=31u8 {
        let vendor_id = read_config_word(bus, device, 0, 0x00);
        if vendor_id == 0xFFFF || vendor_id == 0x0000 { continue; }

        let header_type = read_config_byte(bus, device, 0, 0x0E);
        let is_multi = (header_type & 0x80) != 0;
        let device_id = read_config_word(bus, device, 0, 0x02);
        let class_raw = read_config_word(bus, device, 0, 0x0A);
        let class = (class_raw >> 8) as u8;
        let subclass = (class_raw & 0xFF) as u8;
        let is_bridge = class == 0x06 && subclass == 0x04;
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

        if is_multi {
            for function in 1..=7u8 {
                let vf = read_config_word(bus, device, function, 0x00);
                if vf != 0xFFFF && vf != 0x0000 {
                    let df = read_config_word(bus, device, function, 0x02);
                    let cr = read_config_word(bus, device, function, 0x0A);
                    let cl = (cr >> 8) as u8;
                    let sc = (cr & 0xFF) as u8;
                    let pi = (read_config_word(bus, device, function, 0x08) >> 8) as u8;
                    // Le todas as 6 BARs, exatamente como a function 0 (funcoes 1-7
                    // podem ter recursos MMIO/IO proprios, ex: multi-porta NIC/SATA).
                    let b0 = read_bar(bus, device, function, 0);
                    let b1 = read_bar(bus, device, function, 1);
                    let b2 = read_bar(bus, device, function, 2);
                    let b3 = read_bar(bus, device, function, 3);
                    let b4 = read_bar(bus, device, function, 4);
                    let b5 = read_bar(bus, device, function, 5);
                    devices.push(PciDevice {
                        bus, device, function,
                        vendor_id: vf, device_id: df, class: cl, subclass: sc, prog_if: pi,
                        bar0: b0, bar1: b1, bar2: b2, bar3: b3, bar4: b4, bar5: b5,
                    });
                }
            }
        }

        if is_bridge {
            let sec_bus = read_config_byte(bus, device, 0, 0x19);
            if sec_bus != 0 && sec_bus != bus {
                devices.extend(scan_bus(sec_bus, visited));
            }
        }
    }
    devices
}

/// Varre barramento PCI com zero alocação, executando callback para cada device.
/// Retorna true se o callback abortou a varredura (early exit).
pub unsafe fn scan_pci_cb<F: FnMut(u8, u8, u8, u16, u16) -> bool>(mut cb: F) -> bool {
    for bus in 0..=255u8 {
        for slot in 0..=31u8 {
            let vid = read_config_word(bus, slot, 0, 0x00);
            if vid == 0xFFFF || vid == 0x0000 { continue; }
            let did = read_config_word(bus, slot, 0, 0x02);
            if cb(bus, slot, 0, vid, did) { return true; }
            let htype = read_config_byte(bus, slot, 0, 0x0E);
            if (htype & 0x80) == 0 { continue; }
            for func in 1..=7u8 {
                let vf = read_config_word(bus, slot, func, 0x00);
                if vf == 0xFFFF || vf == 0x0000 { continue; }
                let df = read_config_word(bus, slot, func, 0x02);
                if cb(bus, slot, func, vf, df) { return true; }
            }
        }
    }
    false
}

/// Busca o primeiro dispositivo PCI por class/subclass (zero alocação).
/// Retorna (bus, device, function, vendor_id, device_id) se encontrado.
pub unsafe fn find_device_by_class(class: u8, subclass: u8) -> Option<(u8, u8, u8, u16, u16)> {
    let mut result = None;
    scan_pci_cb(|bus, slot, func, vid, did| {
        let c = read_config_word(bus, slot, func, 0x0A);
        if (c >> 8) as u8 == class && (c & 0xFF) as u8 == subclass {
            result = Some((bus, slot, func, vid, did));
            true
        } else {
            false
        }
    });
    result
}

pub unsafe fn scan_pci() -> Vec<PciDevice> {
    let mut visited = alloc::vec::Vec::new();
    let mut all = alloc::vec::Vec::new();
    // Escaneia todos os barramentos da root hierarchy
    for bus in 0..=255u8 {
        all.extend(scan_bus(bus, &mut visited));
    }
    all
}

pub unsafe fn init_pci() -> Vec<PciDevice> {
    let devices = scan_pci();
    kjson!("BOOT", "PCI", "scan", "n", devices.len());
    for (i, dev) in devices.iter().enumerate() {
        kjson!("PCI", "DEV", "found", "idx", i,
            "bus", dev.bus, "dev", dev.device, "fn", dev.function,
            "vid", dev.vendor_id, "did", dev.device_id, "class", dev.class);
    }
    devices
}

/// Estrutura de uma capability PCI vendor-specific VirtIO (tipo 0x09)
#[derive(Debug, Clone, Copy)]
pub struct VirtioPciCap {
    pub bar: u8,
    pub offset: u32,
    pub length: u32,
    pub cfg_type: u8,  // 0=common, 1=notify, 2=isr, 3=device, 4=pci_cfg
}

/// Lê o ponteiro de capabilities (offset 0x34) e retorna um array
pub unsafe fn read_pci_capabilities(bus: u8, device: u8, function: u8) -> alloc::vec::Vec<(u8, u8)> {
    let mut caps = alloc::vec::Vec::new();
    let mut ptr = read_config_byte(bus, device, function, 0x34) as u8;
    while ptr != 0 {
        let cap_id = read_config_byte(bus, device, function, ptr);
        let next = read_config_byte(bus, device, function, ptr + 1);
        caps.push((cap_id, ptr));
        ptr = next;
    }
    caps
}

/// Varre capabilities PCI por tipo 0x09 (VirtIO vendor-specific)
/// e extrai cfg_type, bar, offset, length para encontrar o MMIO base.
pub unsafe fn read_virtio_cap(bus: u8, device: u8, function: u8, target_cfg_type: u8) -> Option<VirtioPciCap> {
    let caps = read_pci_capabilities(bus, device, function);
    for (cap_id, ptr) in &caps {
        if *cap_id == 0x09 {
            let cfg_type = read_config_byte(bus, device, function, ptr + 3);
            if cfg_type == target_cfg_type {
                let bar = read_config_byte(bus, device, function, ptr + 4);
                let offset = read_config_dword(bus, device, function, ptr + 8);
                let length = read_config_dword(bus, device, function, ptr + 12);
                let len = if length > 0 { length } else { 0x1000 };
                return Some(VirtioPciCap { bar, offset, length: len, cfg_type });
            }
        }
    }
    None
}

/// Lê o valor da BAR (32-bit) de um device, considerando BARs 64-bit
pub unsafe fn read_bar_value(bus: u8, device: u8, function: u8, bar_index: u8) -> u64 {
    let offset = 0x10 + bar_index * 4;
    let low = read_config_dword(bus, device, function, offset);
    if low & 1 == 1 { return (low & !0xFF) as u64; } // I/O
    let btype = (low >> 1) & 3;
    if btype == 2 {
        // 64-bit MMIO: BAR+1 contém high bits
        let high = read_config_dword(bus, device, function, offset + 4) as u64;
        (low & !0xF) as u64 | (high << 32)
    } else {
        (low & !0xF) as u64
    }
}

pub(crate) unsafe fn write_config_dword(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
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
    core::arch::asm!(
        "out dx, eax",
        in("dx") CONFIG_DATA,
        in("eax") value,
        options(nostack, preserves_flags)
    );
}

/// Habilita Bus Master + Memory Space + I/O Space no PCI command register.
pub unsafe fn enable_pci_bus_master(dev: &PciDevice) {
    enable_pci_bus_master_unsafe(dev.bus, dev.device, dev.function);
}

pub unsafe fn enable_pci_bus_master_unsafe(bus: u8, device: u8, function: u8) {
    let cmd = read_config_word(bus, device, function, 0x04);
    let new_cmd = cmd | 0x07;
    if new_cmd != cmd {
        write_config_dword(bus, device, function, 0x04, new_cmd as u32);
        let verify = read_config_word(bus, device, function, 0x04);
        crate::slog_nano!("PCI", "info", "BusMaster enabled for {:02x}:{:02x}.{:02x}: cmd={:#06x}->{:#06x}", bus, device, function, cmd, verify);
    }
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
