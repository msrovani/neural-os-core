//! Ponte neural-kernel ↔ jarbas-crate (ADR-0042 N5.7).
//!
//! Audio truth permanece em `neural-kernel/src/audio/*` (ADR-0045 + Sprint107 wakeword).
//! `jarbas-crate::audio` existe no crate mas não é re-exportado no bin — só cross-check TOPIC_*.

pub fn topics_in_sync() -> bool {
    crate::audio::TOPIC_AUDIO_IN == jarbas_crate::audio::TOPIC_AUDIO_IN
        && crate::audio::TOPIC_AUDIO_OUT == jarbas_crate::audio::TOPIC_AUDIO_OUT
        && crate::audio::TOPIC_WAKEWORD == jarbas_crate::audio::TOPIC_WAKEWORD
        && crate::audio::TOPIC_STT_TEXT == jarbas_crate::audio::TOPIC_STT_TEXT
        && crate::audio::TOPIC_TTS_CMD == jarbas_crate::audio::TOPIC_TTS_CMD
}

/// Log de boot non-fatal — compara TOPIC_* monólito vs jarbas-crate.
pub fn log_bridge_status() {
    let ok = topics_in_sync();
    crate::serial_println!(
        "[JARBAS-BRIDGE] jarbas-crate=linked topics_mirror_in_sync={} audio_truth=neural-kernel full_wire=OK(jarbas-crate)",
        ok
    );
}
