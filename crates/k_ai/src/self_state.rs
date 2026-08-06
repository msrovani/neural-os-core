//! SELF.STATE — autobiografia do OS na SGDB (ADR-0086 §2.8, gap I10).
//! O AIOS lembra quem é, de onde veio e o que já fez: `sys/self_state` (KV) +
//! memória episódica L3. Cada transição de vida grava aqui (instalar, adaptar,
//! atualizar, trocar HW) — o boot é releitura, não redescoberta.

use alloc::string::String;
use alloc::format;
use alloc::vec::Vec;

pub const SELF_STATE_KEY: &str = "sys/self_state";

/// Fase de vida do OS (ADR-0086 §2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifePhase {
    Visitante,    // pendrive live — provar o OS
    Mensageiro,   // pendrive instalador — entregar ao silício
    Residente,    // instalado — dominar
    Unknown,
}

impl LifePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Visitante => "visitante",
            Self::Mensageiro => "mensageiro",
            Self::Residente => "residente",
            Self::Unknown => "unknown",
        }
    }
}

/// Escreve/atualiza o SELF.STATE. Best-effort: falha silenciosa se SGDB/tickv
/// ainda não pronto (boot cedo) — o AIOS segue degradado, não quebra.
pub fn write_self_state(
    phase: LifePhase,
    installed_at: Option<&str>,
    first_boot: bool,
    hw_profile: Option<&str>,
    last_update: Option<&str>,
) {
    let mut s = String::from("SELF.STATE v1\n");
    s.push_str(&format!("phase={}\n", phase.as_str()));
    s.push_str(&format!("first_boot={}\n", first_boot));
    if let Some(i) = installed_at { s.push_str(&format!("installed_at={}\n", i)); }
    if let Some(h) = hw_profile { s.push_str(&format!("hw_profile={}\n", h)); }
    if let Some(u) = last_update { s.push_str(&format!("last_update={}\n", u)); }
    match crate::sgdb::put_kv(SELF_STATE_KEY, s.as_bytes()) {
        Ok(()) => k_nano::slog_kai!("SELF", "state", "SELF.STATE atualizado phase={}", phase.as_str()),
        Err(e) => k_nano::slog_kai!("SELF", "state", "write skip (SGDB indisponivel: {})", e),
    }
}

/// Lê o SELF.STATE atual. None = sem autobiografia ainda (1º boot ou SGDB off).
pub fn read_self_state() -> Option<String> {
    match crate::sgdb::get_kv(SELF_STATE_KEY) {
        Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
        _ => None,
    }
}

/// Fase gravada no SELF.STATE (default Residente se legível).
pub fn current_phase() -> LifePhase {
    match read_self_state() {
        Some(s) if s.contains("phase=residente") => LifePhase::Residente,
        Some(s) if s.contains("phase=mensageiro") => LifePhase::Mensageiro,
        Some(s) if s.contains("phase=visitante") => LifePhase::Visitante,
        _ => LifePhase::Unknown,
    }
}

/// Registra evento de vida na memória episódica (L3) — a narrativa do OS.
pub fn record_life_event(event: &str) {
    let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let key = format!("sys/life/t{}", tick);
    let _ = crate::sgdb::put_kv(&key, event.as_bytes());
    k_nano::slog_kai!("SELF", "life", "{}", event);
}

#[cfg(test)]
mod tests {
    use super::LifePhase;

    #[test]
    fn phase_names() {
        assert_eq!(LifePhase::Visitante.as_str(), "visitante");
        assert_eq!(LifePhase::Mensageiro.as_str(), "mensageiro");
        assert_eq!(LifePhase::Residente.as_str(), "residente");
    }
}
