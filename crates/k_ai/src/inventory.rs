//! Inventário de hardware (k-ai L2) — ADR-0042 N2 + ADR-0041 DeviceTree via k-hal.
use k_nano::acpi::AcpiInfo;
use k_nano::pci::PciDevice;
use alloc::vec::Vec;

/// Snapshot DeviceTree do k-hal (sem scan PCI em k_ai).
pub fn khal_device_tree() -> alloc::vec::Vec<k_hal::device_cap::DeviceCap> {
    k_hal::device_tree()
}

/// Contagem de devices publicados pelo HAL (HEALTH / SelfHeal).
pub fn khal_device_count() -> usize {
    k_hal::discovery::device_count()
}

#[derive(Debug, Clone)]
pub struct HardwareInventory {
    pub cpu_count: u16,
    pub total_ram_bytes: u64,
    pub pci_devices: Vec<PciDevice>,
    pub lapic_count: u16,
    pub has_virtio_net: bool,
    pub has_virtio_gpu: bool,
    pub has_nvme: bool,
    pub has_xhci: bool,
    pub has_gpu: bool,
}

impl HardwareInventory {
    pub fn collect(pci_devices: Vec<PciDevice>, acpi_info: Option<&AcpiInfo>) -> Self {
        let lapic_count = acpi_info.map_or(1, |a| a.lapic_count);
        let has_virtio_net = pci_devices.iter().any(|d| d.vendor_id == 0x1AF4 && d.device_id == 0x1041);
        let has_virtio_gpu = pci_devices.iter().any(|d| d.vendor_id == 0x1AF4 && d.device_id == 0x1050);
        let has_nvme = pci_devices.iter().any(|d| d.class == 0x01 && d.subclass == 0x08);
        let has_xhci = pci_devices.iter().any(|d| d.class == 0x0C && d.subclass == 0x03);
        let has_gpu = pci_devices.iter().any(|d| d.class == 0x03);
        let total_ram_bytes = {
            let guard = k_nano::memory::GLOBAL_ALLOCATOR.lock();
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
            has_gpu,
        }
    }

    /// Inventário a partir do DeviceTree (sem `scan_pci` — boot USB / SESSION_262).
    pub fn from_khal() -> Self {
        let tree = khal_device_tree();
        let has_virtio_net = tree.iter().any(|d| {
            d.id.vendor_id == 0x1AF4 && (d.id.device_id == 0x1041 || d.id.device_id == 0x1000)
        });
        let has_virtio_gpu = tree
            .iter()
            .any(|d| d.id.vendor_id == 0x1AF4 && d.id.class == k_hal::device_cap::DeviceClass::Gpu);
        let has_nvme = tree
            .iter()
            .any(|d| d.id.pci_class == 0x01 && d.id.pci_subclass == 0x08);
        let has_xhci = tree
            .iter()
            .any(|d| d.id.class == k_hal::device_cap::DeviceClass::UsbHost);
        let has_gpu = tree
            .iter()
            .any(|d| d.id.class == k_hal::device_cap::DeviceClass::Gpu);
        let total_ram_bytes = {
            let guard = k_nano::memory::GLOBAL_ALLOCATOR.lock();
            guard.as_ref().map_or(0, |a| a.usable_memory_bytes())
        };
        HardwareInventory {
            cpu_count: 1,
            total_ram_bytes,
            pci_devices: Vec::new(),
            lapic_count: 1,
            has_virtio_net,
            has_virtio_gpu,
            has_nvme,
            has_xhci,
            has_gpu,
        }
    }

    /// Tuplas (VID, DID, class, subclass) para SelfHeal VID-gated (ADR-0042 N2).
    pub fn vid_class_triples(&self) -> Vec<(u16, u16, u8, u8)> {
        if !self.pci_devices.is_empty() {
            return self
                .pci_devices
                .iter()
                .map(|d| (d.vendor_id, d.device_id, d.class, d.subclass))
                .collect();
        }
        crate::boot_observe::heal_triples_from_tree()
    }

    /// Subconjunto que precisa de check de firmware (política NVIDIA-coerente).
    pub fn fw_gated_devices(&self) -> Vec<(u16, u16, u8, u8)> {
        self.vid_class_triples()
            .into_iter()
            .filter(|&(vid, did, class, subclass)| {
                crate::self_heal::SelfHeal::device_needs_fw(vid, did, class, subclass)
            })
            .collect()
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
        let has_gpu = inv.has_gpu || inv.pci_devices.iter().any(|d| d.class == 0x03);
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
