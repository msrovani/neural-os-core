//! SGDB HW registry — Onda 1 T-013..016 (ADR-0100 §1.2)
//! `/hw/storage/<id>/…`, `/hw/gpu/<id>/…`, `/hw/net/<nic>/…`, wifi só se presente.
//! Reuse `tickv` (k_nano::storage) quando pronto; senão só slog (honesto, sem panic).
//! Não inventa novo path — reusa `sgdb.rs` existente (este arquivo).

use alloc::format;

/// Escreve `hw/<path>` no TickvLite se pronto, sempre slog visível (ok).
fn hw_put(path: &str, val: &str) {
    let full = format!("hw/{}", path);
    crate::slog_nano!("SGDB", "ok", "/hw/{}={}", path, val);
    if crate::storage::tickv::is_ready() {
        let _ = crate::storage::tickv::put_blob(&full, val.as_bytes());
    }
}

/// StorageBus → /hw/storage/<id>/… pós-probe (T-013)
pub fn publish_storage() {
    let bus = crate::storage_bus::STORAGE_BUS.lock();
    hw_put("storage/count", &format!("{}", bus.device_count()));
    for (i, e) in bus.entries().iter().enumerate().take(8) {
        let kind = match e.kind {
            crate::storage_bus::BusKind::Nvme => "nvme",
            crate::storage_bus::BusKind::Ahci => "ahci",
            crate::storage_bus::BusKind::Ata => "ata",
            crate::storage_bus::BusKind::Usb => "usb",
            crate::storage_bus::BusKind::VirtioBlk => "virtio-blk",
        };
        hw_put(&format!("storage/{}/kind", i), kind);
        hw_put(&format!("storage/{}/name", i), e.name);
        hw_put(&format!("storage/{}/sectors", i), &format!("{}", e.total_sectors_512));
        hw_put(&format!("storage/{}/mbr_ok", i), if e.mbr_ok { "true" } else { "false" });
        for m in &e.mounts {
            hw_put(&format!("storage/{}/mount", i), m.mount_point);
            hw_put(&format!("storage/{}/fs", i), m.fs_type);
        }
        // T-007 banda opcional: tenta medir se device ainda vivo (best-effort, não hang)
        // não mede aqui (probe já fez I/O); deixa medida explícita para boot_observe.
    }
    if bus.device_count() == 0 {
        crate::slog_nano!("SGDB", "trace", "/hw/storage empty — no BlockDevice");
    }
}

/// GPU → /hw/gpu/<id>/… (T-014) — BAR roles já medidos via `read_bar_size` em s252
pub fn publish_gpu() {
    // # ponytail: host gate — PCI port I/O 0xCF8/0xCFC só em bare-metal
    #[cfg(not(target_os = "none"))]
    {
        crate::slog_nano!("SGDB", "trace", "/hw/gpu skip (host)");
        return;
    }
    #[cfg(target_os = "none")]
    {
        let devs = unsafe { crate::pci::scan_pci() };
        let mut idx = 0usize;
        for d in devs.iter().filter(|d| d.class == 0x03) {
            hw_put(&format!("gpu/{}/did", idx), &format!("{:04x}:{:04x}", d.vendor_id, d.device_id));
            hw_put(&format!("gpu/{}/class", idx), &format!("{:02x}:{:02x}", d.class, d.subclass));
            hw_put(&format!("gpu/{}/bar0", idx), &format!("{:#x}", d.bar0));
            // BAR size real (VRAM aperture) quando possível
            let sz = unsafe { crate::pci::read_bar_size(d.bus, d.device, d.function, 0) };
            if sz > 0 {
                hw_put(&format!("gpu/{}/bar0_size", idx), &format!("{}", sz));
            }
            idx += 1;
            if idx >= 4 { break; }
        }
        if idx == 0 {
            crate::slog_nano!("SGDB", "trace", "/hw/gpu none — no VGA device");
        } else {
            crate::slog_nano!("SGDB", "ok", "/hw/gpu count={}", idx);
        }
    }
}

/// Net → /hw/net/<nic>/… (T-015) — sem WiFi se rádio ausente
pub fn publish_net() {
    let (order, n) = crate::boot_bind::nic_probe_order();
    hw_put("net/plan_n", &format!("{}", n));
    for i in 0..n.min(4) {
        hw_put(&format!("net/{}/kind", i), order[i].as_str());
    }
    // PCI net devices (class 02) como suplemento HW real — só em metal
    #[cfg(target_os = "none")]
    {
        let devs = unsafe { crate::pci::scan_pci() };
        let mut cnt = 0usize;
        for d in devs.iter().filter(|d| d.class == 0x02 && d.subclass != 0x80) {
            hw_put(&format!("net/pci{}/did", cnt), &format!("{:04x}:{:04x}", d.vendor_id, d.device_id));
            cnt += 1;
            if cnt >= 4 { break; }
        }
    }
    #[cfg(not(target_os = "none"))]
    if n == 0 {
        crate::slog_nano!("SGDB", "trace", "/hw/net none (host)");
    }
}

/// WifiAgent só se device presente (T-016) — não fingir
pub fn publish_wifi() {
    #[cfg(not(target_os = "none"))]
    {
        crate::slog_nano!("SGDB", "trace", "/hw/wifi none — skip WifiAgent (host)");
        return;
    }
    #[cfg(target_os = "none")]
    {
        let devs = unsafe { crate::pci::scan_pci() };
        let has_wifi = devs.iter().any(|d| d.class == 0x02 && d.subclass == 0x80);
        if !has_wifi {
            crate::slog_nano!("SGDB", "trace", "/hw/wifi none — skip WifiAgent");
            return;
        }
        hw_put("wifi/present", "true");
        let mut idx = 0usize;
        for d in devs.iter().filter(|d| d.class == 0x02 && d.subclass == 0x80) {
            hw_put(&format!("wifi/{}/did", idx), &format!("{:04x}:{:04x}", d.vendor_id, d.device_id));
            idx += 1;
            if idx >= 4 { break; }
        }
        crate::slog_nano!("SGDB", "ok", "/hw/wifi present devices={}", idx);
    }
}

/// Publica todo /hw/* de uma vez (chame após H1 + StorageBus probe).
pub fn publish_all() {
    publish_storage();
    publish_gpu();
    publish_net();
    publish_wifi();
    crate::slog_nano!("SGDB", "ok", "/hw/* publish_all done");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hw_put_does_not_panic() {
        hw_put("test/key", "val");
    }
    #[test]
    fn publish_all_does_not_panic_without_hw() {
        // Host sem PCI real → scan_pci retorna vazio, deve só logar trace
        publish_all();
    }
}
