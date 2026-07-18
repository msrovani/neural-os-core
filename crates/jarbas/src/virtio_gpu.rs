//! VirtIO-GPU FE — Display via HalOffer; BE/notify em k-hal (ADR-0041 H4+).
//! Sem map BAR / MMIO no R3. Framebuffer = UEFI GOP (já em display::fb).

use k_hal::device_cap::DeviceClass;
use k_hal::offer;
use k_hal::virtio::{self, VirtioStage};

pub const VIRTIO_GPU_TRANS: u16 = 0x1045;
pub const VIRTIO_GPU_MODERN: u16 = 0x1050;

/// Init FE: HalOffer Display + kick QUEUE_NOTIFY no BE k-hal (sem PCI scan MMIO aqui).
pub unsafe fn init_driver_virtio_gpu() -> bool {
    let st = offer::query(DeviceClass::Gpu);
    k_nano::slog_jarbas!("VGPU", "offer", "gpu status={:?}", st);

    // Bind FE display/gpu via HalOffer (Hermes path preferível; aqui bind direto R1 API)
    match offer::request(DeviceClass::Gpu, "display") {
        Ok(h) => {
            k_nano::slog_jarbas!("VGPU", "bind", "HalOffer OK topic={}", h.topic);
        }
        Err(e) => {
            k_nano::slog_jarbas!("VGPU", "bind", "HalOffer {:?} — GOP FE intacto", e);
        }
    }

    // QUEUE_NOTIFY: BE k-hal já tentou no H4; reforça kick se DeviceTree tem VirtIO-GPU
    let mut kicked = false;
    for c in k_hal::discovery::device_tree() {
        if c.id.vendor_id == 0x1AF4
            && (c.id.device_id == VIRTIO_GPU_MODERN || c.id.device_id == VIRTIO_GPU_TRANS)
        {
            let stage = virtio::try_pci_queue_notify(
                c.id.pci_bus,
                c.id.pci_dev,
                c.id.pci_fn,
                0,
            );
            k_nano::slog_hal!("VirtIO", "vgpu_fe", "kick {:?}", stage);
            if stage == VirtioStage::NotifySent {
                kicked = true;
            }
        }
    }

    // Sucesso FE = GOP / GPU lock já populado no boot UEFI
    let gop_ok = crate::display::fb::GPU
        .lock()
        .as_ref()
        .map(|g| g.fb_width > 0)
        .unwrap_or(false);
    k_nano::slog_jarbas!(
        "VGPU",
        "fe",
        "GOP={} notify_kick={} (sem MMIO R3)",
        gop_ok,
        kicked
    );
    gop_ok || kicked || matches!(st, offer::OfferStatus::Available | offer::OfferStatus::Bound)
}
