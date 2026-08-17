//! Observe DeviceTree (k_hal) no boot — plano de bind antes dos drivers.
//! Heurística de tabela (SESSION_248); Cortex ainda sem pesos neste ponto.
//! Não é bypass: o plano é evidência PCI + rank, registrado no EventBus.

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::boot_bind::{classify_nic, install_plan, NicKind};

/// Observa o silício publicado pelo HAL e instala o plano R0.
/// Retorna (devices no tree, nics no plano).
pub fn observe_and_plan() -> (usize, usize) {
    let tree = crate::inventory::khal_device_tree();
    let n = tree.len();
    let mut present: Vec<NicKind> = Vec::new();
    let mut blocks = 0u32;
    let mut nets = 0u32;
    let mut gpus = 0u32;
    let mut usb = 0u32;
    for cap in &tree {
        match cap.id.class {
            k_hal::device_cap::DeviceClass::Block => blocks = blocks.saturating_add(1),
            k_hal::device_cap::DeviceClass::Net | k_hal::device_cap::DeviceClass::Wifi => {
                nets = nets.saturating_add(1);
            }
            k_hal::device_cap::DeviceClass::Gpu => gpus = gpus.saturating_add(1),
            k_hal::device_cap::DeviceClass::UsbHost => usb = usb.saturating_add(1),
            _ => {}
        }
        let kind = classify_nic(cap.id.vendor_id, cap.id.device_id);
        if kind != NicKind::None && !present.iter().any(|k| *k == kind) {
            present.push(kind);
        }
    }
    install_plan(&present, n);
    let (_order, nic_n) = k_nano::boot_bind::nic_probe_order();
    k_nano::slog_kai!(
        "Boot",
        "observe",
        "H1 devices={} nic_plan={} block={} net={} gpu={} usbhost={} (heuristica tabela; Cortex sem pesos ainda)",
        n,
        nic_n,
        blocks,
        nets,
        gpus,
        usb
    );
    let payload = alloc::format!(
        "BOOT_OBSERVE:devices={}:nics={}:block={}:gpu={}",
        n,
        nic_n,
        blocks,
        gpus
    );
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("BOOT_OBSERVE"),
        payload: payload.into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    (n, nic_n)
}

/// Triplas VID-gated a partir do DeviceTree — sem re-scan PCI (SESSION_262 hang).
pub fn heal_triples_from_tree() -> Vec<(u16, u16, u8, u8)> {
    crate::inventory::khal_device_tree()
        .iter()
        .map(|c| {
            (
                c.id.vendor_id,
                c.id.device_id,
                c.id.pci_class,
                c.id.pci_subclass,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_nano::boot_bind::{rank_present, NicKind};

    #[test]
    fn qemu_e1000_only_plan() {
        let (o, n) = rank_present(&[NicKind::E1000]);
        assert_eq!(n, 1);
        assert_eq!(o[0], NicKind::E1000);
    }

    #[test]
    fn triples_empty_without_h1() {
        // Host: DeviceTree vazio — honest, sem panic.
        let t = heal_triples_from_tree();
        assert!(t.is_empty());
    }
}
