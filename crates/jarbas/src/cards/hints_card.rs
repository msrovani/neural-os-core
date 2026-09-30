//! Card "Hints Neurais (H3)" — telemetria AO VIVO do renderer de hints.
//!
//! Validação do H3-revisit em HW sem extrair BOOT.LOG: mostra `HINT_FORWARD_US`
//! e `HINTS_GENERATED` direto dos statics do `hint_render` (fonte única), mais
//! o stage do renderer e o estado de residência na VRAM. Critério de aceite do
//! H3: `fwd < 100µs` e `n` crescendo (~2/s).
//!
//! Refresh: o DisplayAgent re-spawna (`spawn_or_update_card` = mesmo id, a
//! janela não duplica) no tick de 2 Hz — leitura de statics + format, zero HW.
//!
//! Honestidade: em QEMU (VirtIO-GPU sem aperture) o stage é Off/Ready e os
//! contadores ficam 0 — o card mostra isso explicitamente (não finge atividade).

use crate::display::card::{UiDeclaration, Widget};
use alloc::format;
use alloc::string::String;

pub const HINTS_CARD_ID: u32 = 8003;

/// Stage do hint_render em texto honesto (espelha `hint_stage()`).
fn stage_name(s: u8) -> (&'static str, u8) {
    // (label, state-code do Widget::Status: 0=absent 1=unknown 2=ok)
    match s {
        0 => ("off (sem BAR)", 0),
        1 => ("ready (sem pesos)", 1),
        2 => ("resident (na VRAM)", 2),
        _ => ("device", 2),
    }
}

/// Constrói o card a partir dos statics correntes do hint_render.
pub fn hints_card() -> UiDeclaration {
    let stage = k_hal::gpu::hint_render::hint_stage();
    let fwd_us = k_hal::gpu::hint_render::HINT_FORWARD_US.load(core::sync::atomic::Ordering::Relaxed);
    let n = k_hal::gpu::hint_render::HINTS_GENERATED
        .load(core::sync::atomic::Ordering::Relaxed);
    let resident_bytes =
        k_hal::gpu::hint_render::HINT_RESIDENT_BYTES.load(core::sync::atomic::Ordering::Relaxed);

    let (sname, sstate) = stage_name(stage);

    let mut card = UiDeclaration::new(HINTS_CARD_ID, "Hints Neurais (H3)", 430, 96, 340, 150);
    card.closable = true;

    card.body.push(Widget::Status {
        label: String::from("stage"),
        value: String::from(sname),
        state: sstate,
    });
    card.body.push(Widget::Divider);

    // Critério de aceite do H3 ao vivo: fwd < 100µs E n crescendo.
    card.body.push(Widget::Status {
        label: String::from("fwd"),
        value: format!("{}us (alvo <100)", fwd_us),
        state: if stage >= 2 && fwd_us > 0 && fwd_us < 100 { 2 } else { 1 },
    });
    card.body.push(Widget::Status {
        label: String::from("hints n"),
        value: format!("{} (~2/s em HW)", n),
        state: if n > 0 { 2 } else { 1 },
    });
    if resident_bytes > 0 {
        card.body.push(Widget::Status {
            label: String::from("vram"),
            value: format!("{}B resid.", resident_bytes),
            state: 2,
        });
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

    #[test]
    fn stage_off_e_absent_quando_sem_bar() {
        // Host: sem BAR mapeada os statics nascem 0 → stage 0 (Off).
        // O card NÃO deve fingir atividade (estado absent, fwd=0us ≠ ok).
        let card = hints_card();
        assert_eq!(card.id, HINTS_CARD_ID);
        assert_eq!(card.title, "Hints Neurais (H3)");
        // 1ª widget = stage
        match &card.body[0] {
            Widget::Status { label, value, state } => {
                assert_eq!(label.as_str(), "stage");
                assert_eq!(value.as_str(), "off (sem BAR)");
                assert_eq!(*state, 0); // absent — honesto
            }
            other => panic!("widget 0 deveria ser Status, got {:?}", discrim(other)),
        }
    }

    #[test]
    fn fwd_e_n_presentes_mesmo_zerados() {
        let card = hints_card();
        let mut seen_fwd = false;
        let mut seen_n = false;
        for wg in &card.body {
            if let Widget::Status { label, value, state } = wg {
                if label.as_str() == "fwd" {
                    seen_fwd = true;
                    assert!(value.contains("0us"), "host sem BAR: fwd=0, got {}", value);
                    assert_eq!(*state, 1); // unknown: sem dado não é ok
                }
                if label.as_str() == "hints n" {
                    seen_n = true;
                    assert!(value.starts_with("0 "), "host: n=0, got {}", value);
                    assert_eq!(*state, 1);
                }
            }
        }
        assert!(seen_fwd && seen_n);
    }

    #[test]
    fn altura_cobre_todas_as_linhas() {
        let card = hints_card();
        let lines = card.body.len() as i32;
        let dividers = card
            .body
            .iter()
            .filter(|w| matches!(w, Widget::Divider))
            .count() as i32;
        // Mesma conta do builder: linha 13px, divisor 8px, título 34, folga 8.
        let expected = 34 + (lines - dividers) * 13 + dividers * 8 + 8;
        assert_eq!(card.h, expected);
    }

    // Helper de debug (Widget não deriva Debug em todos os campos relevantes).
    fn discrim(w: &Widget) -> &'static str {
        match w {
            Widget::Text(_) => "Text",
            Widget::KeyValue(_, _) => "KeyValue",
            Widget::Gauge { .. } => "Gauge",
            Widget::Bars { .. } => "Bars",
            Widget::List(_) => "List",
            Widget::Divider => "Divider",
            Widget::Button(_) => "Button",
            Widget::Status { .. } => "Status",
            Widget::Panel { .. } => "Panel",
        }
    }
}
