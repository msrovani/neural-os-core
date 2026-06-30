//! DevFsAgent — dispositivos de hardware como arquivos.
//! Mount: /dev/
//! Arquivos: /dev/pci/NNNN:NNNN, /dev/rtl8139/, /dev/xhci/

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use crate::fs::FilesystemAgent;
use crate::pci::PciDevice;

pub struct DevFsAgent {
    pci_devices: Vec<PciDevice>,
}

impl DevFsAgent {
    pub fn new() -> Self {
        let pci_devices = unsafe { crate::pci::scan_pci() };
        DevFsAgent { pci_devices }
    }
}

impl FilesystemAgent for DevFsAgent {
    fn name(&self) -> &str { "devfs" }
    fn mount_point(&self) -> &str { "/dev" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let path = path.trim_matches('/');

        // /dev/pci/<vendor>:<device>
        if let Some(pci_str) = path.strip_prefix("pci/") {
            for dev in &self.pci_devices {
                let id = alloc::format!("{:04x}:{:04x}", dev.vendor_id, dev.device_id);
                if pci_str == id {
                    let info = alloc::format!(
                        "Vendor: {:#06x}\nDevice: {:#06x}\nClass: {:#04x}\nSubclass: {:#04x}\nBAR0: {:#010x}\nBus: {} Dev: {} Func: {}\n",
                        dev.vendor_id, dev.device_id, dev.class, dev.subclass, dev.bar0, dev.bus, dev.device, dev.function
                    );
                    return Ok(info.into_bytes());
                }
            }
            return Err("PCI device not found");
        }

        match path {
            "pci" | "pci/list" => {
                let mut s = String::from("PCI Devices:\n");
                for dev in &self.pci_devices {
                    s.push_str(&alloc::format!("  {:02x}:{:02x}.{:02x} {:04x}:{:04x} class={:#04x}/{:02x}\n",
                        dev.bus, dev.device, dev.function, dev.vendor_id, dev.device_id, dev.class, dev.subclass));
                }
                Ok(s.into_bytes())
            }
            "rtl8139" | "net" => {
                let cfg = crate::net::NET_CONFIG.lock();
                let mac = cfg.mac;
                let ip = cfg.ip;
                let online = cfg.online;
                drop(cfg);
                let s = alloc::format!(
                    "MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\nIP: {}.{}.{}.{}\nOnline: {}\n",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
                    ip[0], ip[1], ip[2], ip[3], online
                );
                Ok(s.into_bytes())
            }
            "xhci" | "usb" => {
                let state = crate::xhci::XHCI_STATE.lock();
                Ok(if state.is_some() {
                    Vec::from("xHCI controller present\n")
                } else {
                    Vec::from("xHCI not available\n")
                })
            }
            "mem" => {
                let ctx = crate::memory::global_hardware_context();
                let s = alloc::format!("Memory: {:.1}% used\n", ctx[0] * 100.0);
                Ok(s.into_bytes())
            }
            _ => Err("Device not found"),
        }
    }

    fn write(&mut self, _path: &str, _data: &[u8]) -> Result<(), &str> {
        Err("/dev/ is read-only")
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        match path.trim_matches('/') {
            "" => Ok(vec![
                String::from("pci"), String::from("rtl8139"),
                String::from("xhci"), String::from("mem"),
            ]),
            "pci" => Ok(self.pci_devices.iter().map(|d| {
                alloc::format!("{:04x}:{:04x}", d.vendor_id, d.device_id)
            }).collect()),
            _ => Err("Directory not found"),
        }
    }
}
