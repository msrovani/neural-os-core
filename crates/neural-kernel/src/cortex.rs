// E1a: re-export crate items + bin-specific Trinity routing functions
pub use cortex_crate::cortex::*;
use alloc::string::String;

// ─── Bin-specific MoE routing director ─────────────────────────
// E1a: moved from crate's cortex.rs because they depend on bin globals
// (TRINITY, EVENT_BUS, audio::settings, etc.)

fn extract_volume_percent(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if let Some(n) = core::str::from_utf8(&b[start..i])
                .ok()
                .and_then(|t| t.parse::<u32>().ok())
            {
                if n <= 100 {
                    return Some(n as u8);
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

fn dispatch_hw_control(utterance: &str) -> String {
    let lower = utterance.to_ascii_lowercase();
    k_nano::slog_cortex!("TRINITY", "info", "MoE routing: HwControl (skill/HW, no LLM)");
    if lower.contains("unmute") {
        crate::audio::settings::AUDIO_VOLUME
            .store(80, core::sync::atomic::Ordering::Relaxed);
        return String::from("Volume restaurado para 80%");
    }
    if lower.contains("mute") {
        crate::audio::settings::AUDIO_VOLUME
            .store(0, core::sync::atomic::Ordering::Relaxed);
        return String::from("Volume mutado (0%)");
    }
    if lower.contains("brilho") || lower.contains("brightness") {
        let pct = extract_volume_percent(&lower).unwrap_or(80);
        return alloc::format!("[HW] brilho {}% (backlight stub — CapGate/HAL pendente)", pct);
    }
    if lower.contains("volume") || lower.contains("vol") {
        let pct = extract_volume_percent(&lower).unwrap_or(80);
        crate::audio::settings::AUDIO_VOLUME
            .store(pct, core::sync::atomic::Ordering::Relaxed);
        return alloc::format!("Volume definido para {}%", pct);
    }
    String::from("[HW] controle nao reconhecido — diga ex: ajuste o volume para 80%")
}

/// Fallback: try CURRENT_MODEL, then ModelHub slots (Vision → GeneratorPro → Reranker).
fn fallback_generate(prompt: &str) -> String {
    let guard = CURRENT_MODEL.lock();
    match guard.as_ref() {
        Some(m) => m.generate(prompt),
        None => {
            if let Some(out) =
                crate::model_hub::generate_from_slot(crate::model_hub::ModelSlot::Vision, prompt)
            {
                return out;
            }
            if let Some(out) =
                crate::model_hub::generate_from_slot(crate::model_hub::ModelSlot::GeneratorPro, prompt)
            {
                return out;
            }
            if let Some(out) =
                crate::model_hub::generate_from_slot(crate::model_hub::ModelSlot::Reranker, prompt)
            {
                return out;
            }
            String::from("[CORTEX] Falcon3-3B-Instruct-1.58bit not loaded")
        }
    }
}

fn dispatch_expert(prompt: &str, expert_name: &str) -> String {
    if expert_name == "hw_control" {
        let utterance = crate::trinity::extract_user_utterance(prompt);
        return dispatch_hw_control(utterance);
    }
    if expert_name == "rust_coder" {
        let guard = RUSTCODER_MODEL.lock();
        if let Some(m) = guard.as_ref() {
            k_nano::slog_cortex!("TRINITY", "info", "MoE routing: RustCoder expert");
            return m.generate(&alloc::format!(
                "{{\"role\":\"system\",\"content\":\"Gere apenas codigo Rust valido.\"}}\n{}\n",
                prompt
            ));
        }
        k_nano::slog_cortex!("TRINITY", "info", "MoE routing: RustCoder expert unloaded, fallback CURRENT_MODEL");
        return fallback_generate(prompt);
    }
    if expert_name == "hw_identify" {
        let guard = HWEXPERT_MODEL.lock();
        if let Some(m) = guard.as_ref() {
            k_nano::slog_cortex!("TRINITY", "info", "MoE routing: HWIdentify expert");
            return m.generate(&alloc::format!("identifique hardware {}", prompt));
        }
        k_nano::slog_cortex!("TRINITY", "info", "MoE routing: HWIdentify expert unloaded, fallback CURRENT_MODEL");
        return fallback_generate(prompt);
    }
    if expert_name == "agent" || expert_name == "orchestrator" || expert_name == "agentic" {
        k_nano::slog_cortex!("TRINITY", "info", "MoE routing: Agent expert");
        if let Some(out) =
            crate::model_hub::generate_from_slot(crate::model_hub::ModelSlot::Agent, prompt)
        {
            return out;
        }
        k_nano::slog_cortex!("TRINITY", "info", "MoE routing: Agent expert unloaded, fallback CURRENT_MODEL");
        return fallback_generate(prompt);
    }
    if expert_name == "generator"
        || expert_name == "generator_pro"
        || expert_name == "generator_fast"
        || expert_name == "tinystories"
    {
        let _ = crate::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from("TRINITY_UNMATCHED"),
            payload: prompt.as_bytes().to_vec(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
        let slot = match expert_name {
            "generator_pro" => crate::model_hub::ModelSlot::GeneratorPro,
            "generator_fast" => crate::model_hub::ModelSlot::Vision,
            "tinystories" => crate::model_hub::ModelSlot::Reranker,
            _ => crate::model_hub::select_generator_slot(prompt),
        };
        k_nano::slog_cortex!(
            "TRINITY",
            "info",
            "MoE generator slot={}",
            slot.name()
        );
        if slot != crate::model_hub::ModelSlot::Active {
            if let Some(out) = crate::model_hub::generate_from_slot(slot, prompt) {
                return out;
            }
            // Pro miss → Fast → Active
            if slot == crate::model_hub::ModelSlot::GeneratorPro {
                if let Some(out) =
                    crate::model_hub::generate_from_slot(crate::model_hub::ModelSlot::Vision, prompt)
                {
                    k_nano::slog_cortex!("TRINITY", "info", "pro miss → generator_fast");
                    return out;
                }
            }
        }
    }
    fallback_generate(prompt)
}

/// Gera resposta usando rota já decidida pelo caller (Hermes R3) — sem re-classificar.
pub fn generate_via_model_with_route(prompt: &str, expert_name: &str) -> String {
    dispatch_expert(prompt, expert_name)
}

pub fn generate_via_model(prompt: &str) -> String {
    // Sempre classifica o utterance do usuário (não o envelope de skills).
    let utterance = crate::trinity::extract_user_utterance(prompt);

    // Chat / saudacao → BitNet principal (nunca expert 128h).
    if cortex_crate::bpe::prompt_is_greeting(utterance) || cortex_crate::bpe::prompt_is_greeting(prompt) {
        k_nano::slog_cortex!("TRINITY", "info", "saudacao → CURRENT_MODEL (skip MoE expert)");
        let _ = cortex_crate::global_arena::take_pending_route();
        let guard = CURRENT_MODEL.lock();
        return match guard.as_ref() {
            Some(m) => m.generate(prompt),
            None => String::from("[CORTEX] Falcon3-3B-Instruct-1.58bit not loaded"),
        };
    }

    // 1) Rota pendente do Hermes (já classificada no utterance)
    if let Some((name, _trace)) = cortex_crate::global_arena::take_pending_route() {
        k_nano::slog_cortex!("TRINITY", "info", "usando rota pendente R3: {}", name);
        return dispatch_expert(prompt, name);
    }
    // 2) Classifica utterance na arena
    let expert_name = cortex_crate::global_arena::with_arena(|arena| {
        let trinity = crate::TRINITY.lock();
        let (expert, trace) = trinity.classify_intent_with_trace(utterance, arena);
        let name = expert.name;
        drop(trinity);
        cortex_crate::global_arena::push_route_trace(trace);
        name
    })
    .unwrap_or_else(|| {
        let trinity = crate::TRINITY.lock();
        trinity.classify_intent(utterance).name
    });
    dispatch_expert(prompt, expert_name)
}
