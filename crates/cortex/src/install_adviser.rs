//! Cortex InstallAdviser — LLM recomenda onde e como instalar (ADR-0079 M3).
//! Roteia via ModelHub slots (compatível com multi-LLM hub em outra sessão).
//! Fallback para regras hardcoded se o slot apropriado não estiver carregado.

use alloc::string::String;
use alloc::format;
use crate::model_hub::{self, ModelSlot};

/// Recomendação de instalação gerada pelo LLM ou fallback.
#[derive(Debug, Clone)]
pub struct InstallRecommendation {
    pub target_disk: String,
    pub filesystem: String,
    pub model_tier: String,
    pub ram_estimate_mb: u64,
    pub explanation: String,
}

/// Gera recomendação usando o ModelHub (slot Active ou GeneratorPro).
/// Se nenhum modelo de geração estiver disponível, usa fallback hardcoded.
pub fn advise(
    profile: &k_nano::hw_profiler::HwProfile,
    disk_summary: &[(&str, u64, &str)],
) -> InstallRecommendation {
    let prompt = build_prompt(profile, disk_summary);

    // Tenta via hub: primeiro GeneratorPro (melhor para análise), depois Active
    let llm_output = model_hub::generate_from_slot(ModelSlot::GeneratorPro, &prompt)
        .or_else(|| model_hub::generate_from_slot(ModelSlot::Active, &prompt));

    if let Some(text) = llm_output {
        if !text.starts_with("[CORTEX]") && !text.is_empty() {
            return parse_recommendation(&text, profile, disk_summary);
        }
    }

    // Fallback: modelo não carregado ou falha
    fallback_recommendation(profile, disk_summary)
}

fn build_prompt(profile: &k_nano::hw_profiler::HwProfile, disk_summary: &[(&str, u64, &str)]) -> String {
    let mut p = String::from("You are the Neural OS installer assistant. Recommend the best target disk and filesystem.\n\nHardware profile:\n");
    p.push_str(&profile.summary());
    p.push_str("\n\nAvailable disks:\n");
    for (name, size_lba, kind) in disk_summary {
        let gb = (size_lba * 512) / (1024 * 1024 * 1024);
        p.push_str(&format!("  {} type={} size={}GB\n", name, kind, gb));
    }
    p.push_str("\nRespond with:\nTARGET=<disk>\nFS=<filesystem>\nMODEL=<model_tier>\nREASON=<short reason>\n");
    p
}

fn parse_recommendation(
    text: &str,
    profile: &k_nano::hw_profiler::HwProfile,
    disk_summary: &[(&str, u64, &str)],
) -> InstallRecommendation {
    let mut target = String::new();
    let mut fs = String::from("NeuralFS");
    let mut model = String::from("2B");
    let mut reason = String::new();

    for line in text.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("TARGET=") {
            target = val.trim().into();
        } else if let Some(val) = line.strip_prefix("FS=") {
            fs = val.trim().into();
        } else if let Some(val) = line.strip_prefix("MODEL=") {
            model = val.trim().into();
        } else if let Some(val) = line.strip_prefix("REASON=") {
            reason = val.trim().into();
        }
    }

    if target.is_empty() {
        return fallback_recommendation(profile, disk_summary);
    }

    let ram_mb = match model.as_str() {
        "7B" | "Large" => 16_000,
        "2B" | "Standard" => 8_000,
        "tiny" | "Tiny" => 4_000,
        _ => 8_000,
    };

    InstallRecommendation {
        target_disk: target,
        filesystem: fs,
        model_tier: model,
        ram_estimate_mb: ram_mb,
        explanation: if reason.is_empty() {
            format!("Install on target with NeuralFS")
        } else {
            reason
        },
    }
}

/// Fallback hardcoded quando o LLM não está disponível.
fn fallback_recommendation(
    profile: &k_nano::hw_profiler::HwProfile,
    disk_summary: &[(&str, u64, &str)],
) -> InstallRecommendation {
    let mut best_disk = String::from("unknown");
    let mut best_size = 0u64;
    for (name, size_lba, _kind) in disk_summary {
        if *size_lba > best_size {
            best_size = *size_lba;
            best_disk = String::from(*name);
        }
    }

    let (model_tier, ram_mb) = match profile.total_ram_mb {
        r if r >= 32_000 => ("2B+BGE+RERANK", 32_000),
        r if r >= 16_000 => ("7B", 16_000),
        r if r >= 8_000 => ("2B", 8_000),
        _ => ("TINY", 4_000),
    };

    let explanation = format!(
        "Recommended: install on {} ({}GB) with NeuralFS. Model: {}. RAM: {}MB detected.",
        best_disk,
        best_size * 512 / (1024 * 1024 * 1024),
        model_tier,
        profile.total_ram_mb,
    );

    InstallRecommendation {
        target_disk: best_disk,
        filesystem: String::from("NeuralFS"),
        model_tier: String::from(model_tier),
        ram_estimate_mb: ram_mb,
        explanation,
    }
}
