//! Ponte neural-kernel ↔ jarbas-crate (ADR-0042 N5.7 / Sprint Sound).
//!
//! Cutover de áudio concluído (e51a48b): `jarbas-crate::audio` é a truth
//! (ADR-0045), o bin re-exporta via `crate::audio` (`pub use jarbas_crate::audio::*`).
//!
//! Como os dois lados resolvem para o MESMO crate, qualquer check de contrato
//! runtime (`topics_in_sync`/`settings_contract_ok`) era tautológico — removido
//! na auditoria 10 itens #3. O contrato é garantido ESTRUTURALMENTE pelo
//! re-export: `crate::audio::TOPIC_*` e `jarbas_crate::audio::TOPIC_*` são o
//! mesmo item (um drift exigiria recompilar o crate com outra definição).
//! Não há código runtime nesta ponte.
