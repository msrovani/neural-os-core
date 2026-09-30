//! Card "Hardware Detectado" — inventário honesto do HW visto pela AIOS.
//!
//! Só formatação: lê o snapshot do produtor (`hermes::hw_inventory`, alimentado
//! pelo HwDetectAgent) e monta a `UiDeclaration` (ADR-0058). Nenhuma decisão de
//! política aqui — quem decide o que é `ok`/`unknown`/`absent` é o agente.

use crate::display::card::{UiDeclaration, Widget};
use alloc::format;
use alloc::string::String;
use hermes::hw_inventory::HwInventory;

pub const HW_INVENTORY_CARD_ID: u32 = 8002;

/// Constrói o card a partir do snapshot corrente do produtor.
pub fn hw_inventory_card() -> UiDeclaration {
    build_card(&hermes::hw_inventory::snapshot())
}

/// Puro: snapshot → declaração (testável sem hardware).
pub fn build_card(inv: &HwInventory) -> UiDeclaration {
    let mut card = UiDeclaration::new(HW_INVENTORY_CARD_ID, "Hardware Detectado", 16, 96, 400, 200);
    card.closable = true;

    card.body
        .push(Widget::Text(format!("{} dispositivos PCI", inv.found)));
    card.body.push(Widget::Divider);

    // Uma linha por subsistema com affordance de estado (ok/unknown/absent).
    for r in inv.rows.iter() {
        card.body.push(Widget::Status {
            label: String::from(r.row.label()),
            value: String::from(r.detail()),
            state: r.state.code(),
        });
    }

    // Devices que a AIOS não sabe nomear/suportar — nunca somem.
    if inv.unknown_len > 0 {
        card.body.push(Widget::Divider);
        card.body
            .push(Widget::Text(format!("Sem driver ({}):", inv.unknown_total)));
        for u in inv.unknown.iter().take(inv.unknown_len as usize) {
            card.body.push(Widget::Text(String::from(u.label())));
        }
        if (inv.unknown_total as usize) > inv.unknown_len as usize {
            card.body.push(Widget::Text(format!(
                "+{} mais",
                inv.unknown_total as usize - inv.unknown_len as usize
            )));
        }
    }

    // Altura: título (26) + linhas (13) + divisores (8) + folga.
    let mut h = 34i32;
    for wg in &card.body {
        h += match wg {
            Widget::Divider => 8,
            _ => 13,
        };
    }
    card.h = h + 8;
    card
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes::hw_inventory::{HwRow, HwRowEntry, HwState, HwUnknown};

    #[test]
    fn card_lists_five_rows_and_unknown_devices() {
        let mut inv = HwInventory::empty();
        inv.found = 7;
        inv.rows[HwRow::Nic as usize] =
            HwRowEntry::new(HwRow::Nic, HwState::Ok, "e1000 8086:100e bound");
        inv.rows[HwRow::Wifi as usize] = HwRowEntry::new(
            HwRow::Wifi,
            HwState::Unknown,
            "unknown 14c3:7925 Network no driver",
        );
        inv.unknown_len = 1;
        inv.unknown_total = 1;
        inv.unknown[0] = HwUnknown::new("PCI 14c3:7925 Network");

        let card = build_card(&inv);

        let status = card
            .body
            .iter()
            .filter(|w| matches!(w, Widget::Status { .. }))
            .count();
        assert_eq!(status, 5, "uma linha de status por subsistema");
        assert!(card
            .body
            .iter()
            .any(|w| matches!(w, Widget::Text(t) if t.contains("14c3:7925"))));
        assert!(card.h > 26);
    }

    #[test]
    fn unknown_cap_shows_remainder_count() {
        let mut inv = HwInventory::empty();
        inv.unknown_len = 8;
        inv.unknown_total = 11; // 8 listados + 3 restantes
        for i in 0..8usize {
            inv.unknown[i] = HwUnknown::new("PCI 9999:1000 Network");
        }
        let card = build_card(&inv);
        assert!(card
            .body
            .iter()
            .any(|w| matches!(w, Widget::Text(t) if t == "+3 mais")));
    }
}
