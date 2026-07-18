//! Ponte neural-kernel ↔ jarbas-crate (ADR-0042 N5.7 / Sprint Sound).
//!
//! Audio truth permanece em `neural-kernel/src/audio/*` (ADR-0045).
//! `jarbas-crate::audio` é espelho — TOPIC_* + settings contract; sem cutover.

/// Verifica contrato de tópicos EventBus entre monólito e espelho.
pub fn topics_in_sync() -> bool {
    crate::audio::TOPIC_AUDIO_IN == jarbas_crate::audio::TOPIC_AUDIO_IN
        && crate::audio::TOPIC_AUDIO_OUT == jarbas_crate::audio::TOPIC_AUDIO_OUT
        && crate::audio::TOPIC_WAKEWORD == jarbas_crate::audio::TOPIC_WAKEWORD
        && crate::audio::TOPIC_STT_TEXT == jarbas_crate::audio::TOPIC_STT_TEXT
        && crate::audio::TOPIC_TTS_CMD == jarbas_crate::audio::TOPIC_TTS_CMD
}

/// Defaults de settings alinhados (espelho ↔ truth).
pub fn settings_contract_ok() -> bool {
    use core::sync::atomic::Ordering;
    let t = crate::audio::settings::WAKE_LISTEN_TICKS.load(Ordering::Relaxed);
    let j = jarbas_crate::audio::settings::WAKE_LISTEN_TICKS.load(Ordering::Relaxed);
    let vt = crate::audio::settings::VAD_THRESHOLD.load(Ordering::Relaxed);
    let vj = jarbas_crate::audio::settings::VAD_THRESHOLD.load(Ordering::Relaxed);
    t == j && vt == vj
}

/// Log de boot non-fatal — compara TOPIC_* + settings contract.
pub fn log_bridge_status() {
    let topics = topics_in_sync();
    let settings = settings_contract_ok();
    k_nano::slog_bin!("JARBAS", "BRIDGE", "jarbas-crate=linked topics_ok={} settings_ok={} audio_truth=neural-kernel cutover=deferred(ADR-0045)",
        topics,
        settings);
}
