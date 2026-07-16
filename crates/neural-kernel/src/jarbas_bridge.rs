//! Ponte incremental neural-kernel <-> jarbas/audio (Sprint 107 Part B #9, ADR-0045).
//!
//! Truth do audio pipeline continua em `neural-kernel/src/audio/*` (boot real).
//! `crates/jarbas/src/audio/*` e um espelho K2CHJ ainda nao wired ao binario de boot.
//!
//! ## Blocker CONFIRMADO (2026-07-16)
//! `jarbas` foi adicionado como dependencia OPCIONAL em `Cargo.toml` (feature
//! `jarbas-bridge`, desligada por padrao). Isso sozinho compila e linka OK
//! (`cargo build --release --target x86_64-unknown-none --features jarbas-bridge`
//! termina 0 erros ENQUANTO nenhum item de `jarbas::*` for referenciado).
//!
//! Mas ao referenciar qualquer item de `jarbas::audio::*` (ex.: `jarbas::audio::TOPIC_AUDIO_IN`),
//! o link FALHA com:
//!   error: the `#[global_allocator]` in this crate conflicts with global allocator in: k_nano
//!   error: the `#[alloc_error_handler]` in this crate conflicts with allocation error handler in: k_nano
//! Motivo: `jarbas` depende de `hermes -> k_ai -> cortex -> k_nano`, e `k_nano::allocator`
//! declara seu proprio `#[global_allocator]`/`#[alloc_error_handler]` (TALC), que colide
//! com o de `neural-kernel::allocator` no MESMO binario final (`crate-type=bin`).
//! Isso NAO e um ciclo de dependencias Cargo — e um conflito de lang items no link.
//!
//! ## Wiring pleno = fora de escopo desta sprint
//! Resolver exigiria uma de: (a) remover o `#[global_allocator]` de `k_nano::allocator`
//! e fazer `neural-kernel` fornecer o unico alocador global tambem para os crates K2CHJ
//! (risco alto — `k_nano` e usado standalone por outros bins/testes), ou (b) `neural-kernel`
//! parar de ter seu proprio allocator e usar o de `k_nano` (grande refactor, fora do
//! escopo "nao quebrar boot" desta sprint). Ver `docs/memory/STATE.md` "Próximo".
//!
//! ## Passo incremental seguro (o que este modulo faz)
//! Mantem os `TOPIC_*` sincronizados MANUALMENTE (comentario, nao referencia cross-crate)
//! e loga um diagnostico non-fatal em boot quando `jarbas-bridge` esta habilitada,
//! comparando apenas contra os literais locais (`crate::audio::TOPIC_*`) — sem
//! `use jarbas::...`, para nao disparar o conflito de allocator acima.
//! Espelho fonte: `crates/jarbas/src/audio/mod.rs` linhas 35-39 (TOPIC_AUDIO_IN,
//! TOPIC_AUDIO_OUT, TOPIC_WAKEWORD, TOPIC_STT_TEXT, TOPIC_TTS_CMD) — idênticos hoje.

/// Copia manual (NAO importada) dos literais TOPIC_* de `jarbas/src/audio/mod.rs`,
/// para detectar drift sem referenciar o crate `jarbas` (evita o conflito de allocator).
/// Atualizar manualmente se `jarbas/src/audio/mod.rs` mudar.
mod jarbas_mirror_literals {
    pub const TOPIC_AUDIO_IN: &str = "AUDIO_IN";
    pub const TOPIC_AUDIO_OUT: &str = "AUDIO_OUT";
    pub const TOPIC_WAKEWORD: &str = "WAKEWORD";
    pub const TOPIC_STT_TEXT: &str = "STT_TEXT";
    pub const TOPIC_TTS_CMD: &str = "TTS_CMD";
}

pub fn topics_in_sync() -> bool {
    use jarbas_mirror_literals as m;
    crate::audio::TOPIC_AUDIO_IN == m::TOPIC_AUDIO_IN
        && crate::audio::TOPIC_AUDIO_OUT == m::TOPIC_AUDIO_OUT
        && crate::audio::TOPIC_WAKEWORD == m::TOPIC_WAKEWORD
        && crate::audio::TOPIC_STT_TEXT == m::TOPIC_STT_TEXT
        && crate::audio::TOPIC_TTS_CMD == m::TOPIC_TTS_CMD
}

/// Log de boot non-fatal — roda uma vez, so sob a feature `jarbas-bridge`.
pub fn log_bridge_status() {
    let ok = topics_in_sync();
    crate::serial_println!(
        "[JARBAS-BRIDGE] optional_dep=linked (unreferenced) topics_mirror_in_sync={} full_wire=BLOCKED(global_allocator k_nano vs neural-kernel)",
        ok
    );
}
