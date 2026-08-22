//! Observe DeviceTree — plano NIC+storage, cards PnP, Trust, memória.
//! Premissas: ADR-0088, emagrecer, Agent/Skill (cards+EventBus), Trust HITL,
//! DeviceRecipe (Escalate ≠ Auto), HANR quando SGDB acordar.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use k_hal::device_cap::DeviceClass;
use k_hal::device_recipe::RecipePromote;
use k_nano::boot_bind::{
    classify_nic, classify_storage, install_plan, install_storage_plan, set_has_snd, NicKind,
    StorageKind,
};
use spin::Mutex;

static PENDING_HANR: Mutex<Option<String>> = Mutex::new(None);

/// `trust_ok`: (token,agent,skill)=(1,boot_observe,plan). Deny não apaga evidência —
/// só recusa Auto em recipe Escalate (HITL).
pub fn observe_and_plan(trust_ok: bool) -> (usize, usize) {
    let tree = crate::inventory::khal_device_tree();
    let n = tree.len();
    let mut nics: Vec<NicKind> = Vec::new();
    let mut stores: Vec<StorageKind> = Vec::new();
    let mut blocks = 0u32;
    let mut nets = 0u32;
    let mut gpus = 0u32;
    let mut usb = 0u32;
    let mut snd = 0u32;
    let mut escalate_n = 0u32;
    let mut cards = 0u32;

    for cap in &tree {
        match cap.id.class {
            DeviceClass::Block => blocks = blocks.saturating_add(1),
            DeviceClass::Net | DeviceClass::Wifi => nets = nets.saturating_add(1),
            DeviceClass::Gpu => gpus = gpus.saturating_add(1),
            DeviceClass::UsbHost => usb = usb.saturating_add(1),
            DeviceClass::Snd => snd = snd.saturating_add(1),
            _ => {}
        }
        let promote = cap.recipe_promote;
        if promote == RecipePromote::Escalate as u8 {
            escalate_n = escalate_n.saturating_add(1);
            publish_health_escalate(cap.id.vendor_id, cap.id.device_id);
        }
        let nic = classify_nic(cap.id.vendor_id, cap.id.device_id);
        let allow_auto = promote != RecipePromote::Escalate as u8;
        if nic != NicKind::None && allow_auto && !nics.iter().any(|k| *k == nic) {
            nics.push(nic);
        }
        let st = if cap.id.class == DeviceClass::UsbHost {
            StorageKind::UsbHost
        } else {
            classify_storage(cap.id.pci_class, cap.id.pci_subclass)
        };
        if st != StorageKind::None && allow_auto && !stores.iter().any(|k| *k == st) {
            let tcg = k_nano::platform_probe::hypervisor()
                == k_nano::platform_probe::HypervisorKind::Tcg;
            if tcg && st == StorageKind::Ata {
                k_nano::slog_kai!("Boot", "observe", "TCG skip ATA no plano");
            } else {
                stores.push(st);
            }
        }
        if cards < 16 {
            emit_card(cap.id.vendor_id, cap.id.device_id, cap.id.pci_class, cap.id.pci_subclass, cap.name);
            cards = cards.saturating_add(1);
        }
    }

    if !trust_ok {
        k_nano::slog_kai!(
            "Boot",
            "observe",
            "Trust DENY (1,boot_observe,plan) — plano por evidencia; Escalate nao Auto"
        );
    }

    install_plan(&nics, n);
    install_storage_plan(&stores, n);
    set_has_snd(n == 0 || snd > 0);
    let (_no, nic_n) = k_nano::boot_bind::nic_probe_order();
    let (_so, sto_n) = k_nano::boot_bind::storage_probe_order();

    let summary = format!(
        "BOOT_OBSERVE:devices={}:nics={}:storage={}:block={}:gpu={}:usb={}:snd={}:escalate={}:trust={}",
        n, nic_n, sto_n, blocks, gpus, usb, snd, escalate_n, trust_ok
    );
    let ai = k_nano::boot_report::BootAiCounts {
        observe: n as u32,
        plan: (nic_n.saturating_add(sto_n)) as u32,
        act: (nics.len().saturating_add(stores.len())) as u32,
        escalate: escalate_n,
        verify: 0,
    };
    k_nano::boot_report::note_ai(ai);
    k_nano::slog_kai!("Boot", "aios", "{}", ai.line());
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("BOOT_AI"),
        payload: ai.line().into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    k_nano::slog_kai!("Boot", "observe", "{} (tabela+recipe; Cortex sem pesos)", summary);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("BOOT_OBSERVE"),
        payload: summary.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    *PENDING_HANR.lock() = Some(summary);
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

fn emit_card(vid: u16, did: u16, class: u8, subclass: u8, name: &str) {
    let card = crate::hw_capability::build_card(vid, did, class, subclass, name);
    let wire = card.to_wire();
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(crate::hw_capability::TOPIC_HW_CAPABILITY),
        payload: wire.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

fn publish_health_escalate(vid: u16, did: u16) {
    let msg = format!("HEALTH_ISSUE:HITL:recipe_escalate:{:04X}:{:04X}", vid, did);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("HEALTH_ISSUE"),
        payload: msg.into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

/// Depois de `sgdb::boot_init` — memoriza o plano (ADR-0088 mandamento 3).
pub fn hydrate_memory() {
    let Some(s) = PENDING_HANR.lock().take() else {
        return;
    };
    if crate::sgdb::ready() {
        let _ = crate::sgdb::put_hanr("boot_bind", &s);
        crate::self_state::record_life_event(&s);
        k_nano::boot_report::note_ai_verify();
    } else {
        k_nano::slog_kai!("Boot", "observe", "SGDB nao ready — plano so no EventBus (honesto)");
        *PENDING_HANR.lock() = Some(s);
    }
}

#[cfg(test)]
mod tests {
    use k_nano::boot_bind::{rank_storage, StorageKind};

    #[test]
    fn qemu_e1000_only_plan() {
        let (o, n) = k_nano::boot_bind::rank_present(&[k_nano::boot_bind::NicKind::E1000]);
        assert_eq!(n, 1);
        assert_eq!(o[0], k_nano::boot_bind::NicKind::E1000);
    }

    #[test]
    fn triples_empty_without_h1() {
        let t = super::heal_triples_from_tree();
        assert!(t.is_empty());
    }

    #[test]
    fn usb_live_skips_ata_in_rank() {
        let (o, n) = rank_storage(&[StorageKind::UsbHost, StorageKind::Nvme]);
        assert_eq!(n, 2);
        assert_eq!(o[0], StorageKind::Nvme);
        assert_eq!(o[1], StorageKind::UsbHost);
    }

    #[test]
    fn boot_ai_line_roundtrip() {
        let c = k_nano::boot_report::BootAiCounts {
            observe: 3,
            plan: 2,
            act: 2,
            escalate: 1,
            verify: 0,
        };
        let p = k_nano::boot_report::parse_boot_ai_line(&c.line()).unwrap();
        assert_eq!(p.act, 2);
        assert_eq!(p.escalate, 1);
    }
}
