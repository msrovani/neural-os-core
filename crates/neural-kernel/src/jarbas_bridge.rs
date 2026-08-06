//! Ponte neural-kernel ↔ jarbas-crate (ADR-0042 N5.7 / Sprint Sound).
//!
//! Cutover de áudio concluído (e51a48b): `jarbas-crate::audio` é a truth
//! (ADR-0045), o bin re-exporta via `crate::audio` e os antigos truth de
//! `neural-kernel/src/audio/*` foram deletados.

/// Verifica contrato de tópicos EventBus entre bin e crate (legado).
///
/// Desde o re-export (`pub use jarbas_crate::audio::*`), os dois lados
/// resolvem para o mesmo crate — o check é tautológico. Mantido como
/// contrato documental/legacy; não muda comportamento.
pub fn topics_in_sync() -> bool {
    crate::audio::TOPIC_AUDIO_IN == jarbas_crate::audio::TOPIC_AUDIO_IN
        && crate::audio::TOPIC_AUDIO_OUT == jarbas_crate::audio::TOPIC_AUDIO_OUT
        && crate::audio::TOPIC_WAKEWORD == jarbas_crate::audio::TOPIC_WAKEWORD
        && crate::audio::TOPIC_STT_TEXT == jarbas_crate::audio::TOPIC_STT_TEXT
        && crate::audio::TOPIC_TTS_CMD == jarbas_crate::audio::TOPIC_TTS_CMD
}

/// Defaults de settings alinhados (bin ↔ crate).
///
/// Tautológico após o re-export (ambos resolvem `jarbas_crate::audio::settings`);
/// mantido como contrato documental/legacy — não muda comportamento.
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
    k_nano::slog_bin!("JARBAS", "BRIDGE", "jarbas-crate=linked topics_ok={} settings_ok={} audio_truth=jarbas-crate cutover=done(e51a48b)",
        topics,
        settings);
}
