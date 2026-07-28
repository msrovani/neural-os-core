//! HW Change Detection — compara HW atual com perfil salvo (ADR-0079 M4).
//! No boot, detecta se GPU/NIC/WiFi mudaram desde a última instalação.
//! Dispara eventos para SelfHeal ou NetFs fallback.

use alloc::string::String;
use alloc::format;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::hw_profiler::{self, HwProfile};

/// Resultado da comparação entre HW atual e perfil salvo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HwChangeVerdict {
    Unchanged,
    GpuChanged { old: String, new: String },
    NicChanged { old: String, new: String },
    WifiChanged { old: String, new: String },
    DiskCountChanged { old: usize, new: usize },
    Unknown,
}

pub static HW_CHANGED: AtomicBool = AtomicBool::new(false);

/// Compara HW atual com perfil salvo no NeuralFS.
/// `saved_profile_json` — conteúdo de /config/hw_profile.txt do target.
pub fn detect_hw_changes(saved_profile_json: &str) -> HwChangeVerdict {
    let current = hw_profiler::profile_hardware();

    // Parse simples do perfil salvo (formato: "RAM: XXXMB CPU:YYY GPU:ZZZ ...")
    let old_has_nvidia = saved_profile_json.contains("GPU:NVIDIA");
    let old_has_intel = saved_profile_json.contains("GPU:Intel");
    let old_has_wifi = saved_profile_json.contains("WiFi:AX");
    let old_has_nic = saved_profile_json.contains("NIC:Realtek");

    // Compara GPU
    if current.has_nvidia_gpu && !old_has_nvidia && old_has_intel {
        HW_CHANGED.store(true, Ordering::Relaxed);
        return HwChangeVerdict::GpuChanged {
            old: String::from("Intel"),
            new: String::from("NVIDIA"),
        };
    }
    if current.has_intel_gpu && !old_has_intel && old_has_nvidia {
        HW_CHANGED.store(true, Ordering::Relaxed);
        return HwChangeVerdict::GpuChanged {
            old: String::from("NVIDIA"),
            new: String::from("Intel"),
        };
    }

    // Compara WiFi
    if current.has_intel_wifi && !old_has_wifi {
        HW_CHANGED.store(true, Ordering::Relaxed);
        return HwChangeVerdict::WifiChanged {
            old: String::from("none"),
            new: String::from("Intel AX"),
        };
    }
    if !current.has_intel_wifi && old_has_wifi {
        HW_CHANGED.store(true, Ordering::Relaxed);
        return HwChangeVerdict::WifiChanged {
            old: String::from("Intel AX"),
            new: String::from("none"),
        };
    }

    // Compara NIC
    if current.has_realtek_nic && !old_has_nic {
        HW_CHANGED.store(true, Ordering::Relaxed);
        return HwChangeVerdict::NicChanged {
            old: String::from("other"),
            new: String::from("Realtek"),
        };
    }

    HwChangeVerdict::Unchanged
}

/// Lê perfil salvo do NeuralFS do target.
/// ponytail: parse simples de string — versão futura vai ler de /config/hw_profile.txt
pub fn load_saved_profile(target: &mut dyn crate::block_dev::BlockDevice) -> Option<String> {
    // Tenta montar NeuralFS e ler /config/hw_profile.txt
    let total_lba = target.total_sectors();
    let esp_size = 1_048_576u64.min(total_lba.saturating_sub(2048 + 64) / 2);
    let neural_start = 2048u64 + esp_size;

    let mut vol = crate::neural_fs::volume::NeuralVolume::mount(target, neural_start)?;
    // ponytail: lookup de /config/hw_profile.txt via inode conhecido
    let _ = vol; // placeholder — walk de diretório virá na Fase 1
    None
}
