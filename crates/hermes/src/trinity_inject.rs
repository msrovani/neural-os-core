//! trinity_inject — Injeção de capacidades (Efeito Matrix).
//!
//! Quando um agente enfrenta tarefa inédita:
//! 1. get_or_mmap_expert (cortex) — carrega pesos do expert na arena
//! 2. register_wasm_skill (hermes) — registra a skill WASM
//! 3. grant_fe (k_hal) — concede capacidade no CapGate
//!
//! Fluxo: hardware_unknown → Trinity classify → inject_capability → AGENT_READY

use alloc::string::String;
use alloc::format;
use cortex::trinity::{TrinityRouter, ExpertKind};
use k_hal::cap_gate::{fe_for_class, grant_fe};
use k_hal::device_cap::DeviceClass;

/// Resultado da injeção de capacidade.
#[derive(Debug, Clone)]
pub struct InjectResult {
    pub expert: &'static str,
    pub skill_registered: bool,
    pub cap_granted: bool,
    pub msg: String,
}

/// Mapeamento: DeviceClass → ExpertKind.
/// Usado pelo hw_pnp para auto-inject quando dispositivo desconhecido é enumerado.
pub fn class_to_expert_kind(class: DeviceClass) -> Option<ExpertKind> {
    match class {
        DeviceClass::Net | DeviceClass::Wifi => Some(ExpertKind::HwIdentify),
        DeviceClass::Display | DeviceClass::Gpu | DeviceClass::Video => Some(ExpertKind::Generator),
        DeviceClass::Snd => Some(ExpertKind::SpeechSynth),
        DeviceClass::Block => Some(ExpertKind::DiskDiag),
        DeviceClass::Input | DeviceClass::UsbHost | DeviceClass::Bluetooth => Some(ExpertKind::HwControl),
        _ => None,
    }
}

/// Reverse mapping: wire family string → ExpertKind.
/// Usado pelo hw_pnp para auto-inject quando dispositivo desconhecido é enumerado.
pub fn family_to_expert_kind(family: &str) -> Option<ExpertKind> {
    match family {
        "net" | "nic" | "ethernet" | "wifi" | "wlan" => Some(ExpertKind::HwIdentify),
        "gpu" | "display" | "vga" | "framebuffer" => Some(ExpertKind::Generator),
        "audio" | "hda" | "codec" | "tts" => Some(ExpertKind::SpeechSynth),
        "disk" | "ata" | "nvme" | "ahci" | "scsi" => Some(ExpertKind::DiskDiag),
        "input" | "keyboard" | "mouse" | "usb" => Some(ExpertKind::HwControl),
        "npu" | "accelerator" | "tensor" => Some(ExpertKind::RustCoder),
        _ => None,
    }
}

/// Injeta capacidade: expert weights + skill WASM + CapGate grant.
///
/// Chamado quando:
/// - hw_pnp detecta dispositivo desconhecido
/// - Hermes identifica tarefa sem skill correspondente
/// - AutoLearn detecta necessidade de novo expert
pub fn inject_capability(
    router: &mut TrinityRouter,
    kind: ExpertKind,
    class: Option<DeviceClass>,
) -> InjectResult {
    let expert_name = match kind {
        ExpertKind::HwIdentify => "hw_identify",
        ExpertKind::HwControl => "hw_control",
        ExpertKind::RustCoder => "rust_coder",
        ExpertKind::DiskDiag => "disk_diag",
        ExpertKind::Security => "security",
        ExpertKind::Generator => "generator",
        ExpertKind::SpeechSynth => "speech_synth",
        ExpertKind::Unknown => "unknown",
    };

    // 1. Tenta carregar pesos do expert na arena (get_or_mmap_expert)
    let has_weights = router.get_or_mmap_expert(kind).is_some();

    // 2. Registra skill WASM se existir bytecode
    //    (placeholder — integração real com wasmi_rt quando LLM #412 emitir wasm)
    let skill_registered = false; // TODO: wasmi_rt::register_wasm_skill(...)

    // 3. Concede CapGate se classe disponivel
    let mut cap_granted = false;
    if let Some(c) = class {
        if let Some(cap) = fe_for_class(c) {
            grant_fe(cap);
            cap_granted = true;
            k_nano::slog_bin!("HERMES", "info",
                "inject_capability: {} cap_granted={:?}",
                expert_name, cap);
        }
    }

    let msg = format!(
        "expert={} weights={} skill={} cap={}",
        expert_name, has_weights, skill_registered, cap_granted
    );

    k_nano::slog_bin!("HERMES", "info",
        "inject_capability: {} (Efeito Matrix)",
        msg);

    InjectResult {
        expert: expert_name,
        skill_registered,
        cap_granted,
        msg,
    }
}

/// Auto-inject: chamado pelo hw_pnp quando dispositivo desconhecido é enumerado.
/// Mapeia DeviceClass → ExpertKind → inject_capability.
pub fn auto_inject_for_device(
    router: &mut TrinityRouter,
    family: &str,
    class: Option<DeviceClass>,
) -> InjectResult {
    if let Some(kind) = family_to_expert_kind(family) {
        inject_capability(router, kind, class)
    } else {
        InjectResult {
            expert: "unknown",
            skill_registered: false,
            cap_granted: false,
            msg: String::from("no expert mapping for family"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_to_expert_mapping() {
        assert!(matches!(class_to_expert_kind(DeviceClass::Net), Some(ExpertKind::HwIdentify)));
        assert!(matches!(class_to_expert_kind(DeviceClass::Snd), Some(ExpertKind::SpeechSynth)));
        assert!(matches!(class_to_expert_kind(DeviceClass::Block), Some(ExpertKind::DiskDiag)));
    }

    #[test]
    fn family_to_expert_mapping() {
        assert!(matches!(family_to_expert_kind("ethernet"), Some(ExpertKind::HwIdentify)));
        assert!(matches!(family_to_expert_kind("hda"), Some(ExpertKind::SpeechSynth)));
        assert!(matches!(family_to_expert_kind("unknown_device"), None));
    }
}
