//! ADR-0079 — Install Progress Card para o AutoInstaller Neural.
//! Escuta INSTALL_PROGRESS no EventBus e exibe gauge + step atual.

use crate::display::card::{UiDeclaration, Widget};
use alloc::string::String;
use alloc::format;
use k_nano::EVENT_BUS;

pub const TOPIC_INSTALL_PROGRESS: &str = "INSTALL_PROGRESS";

/// Cria card de progresso de instalação.
/// `progress` — percentual 0-100.
/// `step` — descrição do passo atual (ex: "Copiando kernel...").
pub fn install_progress_card(progress: u8, step: &str) -> UiDeclaration {
    let mut card = UiDeclaration::new(7901, "Installing Neural OS", 20, 40, 350, 160);
    card.closable = false;

    // Gauge de progresso
    card.body.push(Widget::Gauge {
        label: String::from("Progress"),
        value: progress as i32,
        max: 100,
        unit: String::from("%"),
    });

    // Step atual
    card.body.push(Widget::KeyValue(
        String::from("Step"),
        String::from(step),
    ));

    // Previsão baseada em progresso
    if progress < 100 {
        card.body.push(Widget::Text(String::from("Running...")));
    } else {
        card.body.push(Widget::Text(String::from("Complete! Reboot to enjoy Neural OS.")));
        card.body.push(Widget::Button(String::from("Reboot")));
    }

    card
}

/// Lê o evento de progresso mais recente do EventBus.
/// Retorna (progress, step) ou (0, "waiting") se não houver evento.
pub fn poll_progress() -> (u8, String) {
    let rx = EVENT_BUS.subscribe(TOPIC_INSTALL_PROGRESS);
    if let Some(ev) = rx.try_receive() {
        let payload = core::str::from_utf8(&ev.payload).unwrap_or("{}");
        // Parse simples: {step:..., progress:N}
        let progress = if let Some(pos) = payload.find("progress:") {
            let val_str = &payload[pos + 9..];
            let val_end = val_str.find(|c: char| !c.is_ascii_digit()).unwrap_or(val_str.len());
            val_str[..val_end].parse::<u8>().unwrap_or(0)
        } else { 0 };
        let step = if let Some(pos) = payload.find("step:") {
            let val_str = &payload[pos + 5..];
            let val_end = val_str.find(',').unwrap_or(val_str.len());
            let raw = &val_str[..val_end];
            String::from(raw.trim_matches(|c: char| c == '"' || c == ' ' || c == '}'))
        } else {
            String::from("installing...")
        };
        (progress, step)
    } else {
        (0, String::from("waiting"))
    }
}
