use crate::acpi::AcpiInfo;
use crate::pci::PciDevice;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct HardwareInventory {
    pub cpu_count: u8,
    pub total_ram_bytes: u64,
    pub pci_devices: Vec<PciDevice>,
    pub lapic_count: u8,
    pub has_virtio_net: bool,
    pub has_virtio_gpu: bool,
    pub has_nvme: bool,
    pub has_xhci: bool,
}

impl HardwareInventory {
    pub fn collect(pci_devices: Vec<PciDevice>, acpi_info: Option<&AcpiInfo>) -> Self {
        let lapic_count = acpi_info.map_or(1, |a| a.lapic_count);
        let has_virtio_net = pci_devices.iter().any(|d| d.vendor_id == 0x1AF4 && d.device_id == 0x1041);
        let has_virtio_gpu = pci_devices.iter().any(|d| d.vendor_id == 0x1AF4 && d.device_id == 0x1050);
        let has_nvme = pci_devices.iter().any(|d| d.class == 0x01 && d.subclass == 0x08);
        let has_xhci = pci_devices.iter().any(|d| d.class == 0x0C && d.subclass == 0x03);
        let total_ram_bytes = {
            let guard = crate::memory::GLOBAL_ALLOCATOR.lock();
            guard.as_ref().map_or(0, |a| a.usable_memory_bytes())
        };

        HardwareInventory {
            cpu_count: core::cmp::max(lapic_count, 1),
            total_ram_bytes,
            pci_devices,
            lapic_count,
            has_virtio_net,
            has_virtio_gpu,
            has_nvme,
            has_xhci,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemArchitecture {
    pub ring0_mode: u8,
    pub ring1_mode: u8,
    pub heap_size_mb: u32,
    pub trust_level: u8,
    pub power_mode: u8,
    pub tensor_tier: u8,
}

impl SystemArchitecture {
    pub fn infer(inv: &HardwareInventory) -> Self {
        let has_gpu = inv.pci_devices.iter().any(|d| d.class == 0x03);
        let ram_gb = inv.total_ram_bytes as f64 / 1_073_741_824.0;
        let is_many_cores = inv.cpu_count > 4;

        SystemArchitecture {
            ring0_mode: 0,
            ring1_mode: if has_gpu { 1 } else { 0 },
            heap_size_mb: if ram_gb > 2.0 { 2048 } else if ram_gb > 0.5 { 512 } else { 64 },
            trust_level: 1,
            power_mode: if is_many_cores { 1 } else { 0 },
            tensor_tier: 0,
        }
    }
}
