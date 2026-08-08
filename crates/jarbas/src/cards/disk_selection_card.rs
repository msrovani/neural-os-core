//! ADR-0086 A5 — Disk Selection Card para o AutoInstaller Neural.
//! Lista discos detectados e deixa o usuário escolher o alvo (target ≠ source).
//! Ao clicar em um disco, define DISK_SELECTION e dispara a instalação.

use crate::display::card::{UiDeclaration, Widget};
use alloc::string::String;
use alloc::format;
use alloc::vec::Vec;
use k_nano::sys_installer::SysInstaller;

/// Cria card de seleção de disco.
/// Cada disco não-boot vira um botão; o boot é marcado como "source".
pub fn disk_selection_card() -> UiDeclaration {
    let mut inst = SysInstaller::new();
    inst.scan_disks();

    let mut card = UiDeclaration::new(7902, "Selecionar Disco de Instalacao", 20, 40, 360, 120);
    card.closable = true;

    if inst.disks.is_empty() {
        card.body.push(Widget::Text(String::from("Nenhum disco detectado.")));
        return card;
    }

    // Lista discos: boot = KeyValue (source), não-boot = botão
    let mut disk_buttons = Vec::new();
    for disk in &inst.disks {
        if disk.is_boot {
            card.body.push(Widget::KeyValue(
                format!("Disco {}", disk.index),
                format!("{} (boot/source) - {} MB", disk.name, disk.total_sectors / 2048),
            ));
        } else {
            disk_buttons.push(format!(
                "Disco {} - {} ({:.0} MB)",
                disk.index,
                disk.name,
                disk.total_sectors as f64 / 2048.0
            ));
        }
    }

    if !disk_buttons.is_empty() {
        card.body.push(Widget::Text(String::from("Escolha o disco de destino:")));
        card.body.push(Widget::List(disk_buttons.clone()));
        for label in &disk_buttons {
            card.body.push(Widget::Button(format!("Instalar em {}", label)));
        }
    } else {
        card.body.push(Widget::Text(String::from("Apenas disco de boot detectado.")));
        card.body.push(Widget::Text(String::from("Conecte um disco de destino (NVMe/USB).")));
    }

    card
}

/// Mapeia o índice do botão (entre os botões de disco no card) → índice do disco real.
/// Os botões são adicionados na ordem dos discos não-boot, então btn_index 0 = 1º não-boot.
pub fn button_index_to_disk_index(btn_index: usize) -> Option<u8> {
    let mut inst = SysInstaller::new();
    inst.scan_disks();
    let non_boot: Vec<u8> = inst.disks.iter().filter(|d| !d.is_boot).map(|d| d.index).collect();
    non_boot.get(btn_index).copied()
}
