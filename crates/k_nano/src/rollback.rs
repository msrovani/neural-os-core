//! Rollback — garante que o pendrive continua bootável se target falhar (ADR-0079 M3).
//! Estratégia: o boot pelo target é tentado N vezes. Se falhar, o firmware UEFI
//! faz fallback para o próximo boot entry (o pendrive). Nosso instalador garante
//! que o pendrive permanece intacto e é configurado como fallback no NVRAM.

use alloc::string::String;
use alloc::format;

/// Estado do boot: qual dispositivo foi o último boot bem-sucedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootSource {
    Pendrive,
    Target,
    Unknown,
}

/// Configuração de boot para suportar rollback.
#[derive(Debug, Clone)]
pub struct RollbackConfig {
    /// Fonte atual de boot
    pub current: BootSource,
    /// Tentativas restantes antes de marcar como falho
    pub remaining_attempts: u8,
    /// Se true, o target já bootou com sucesso ao menos uma vez
    pub target_confirmed: bool,
}

impl RollbackConfig {
    pub fn new() -> Self {
        Self {
            current: BootSource::Unknown,
            remaining_attempts: 3,
            target_confirmed: false,
        }
    }

    /// Marca o target como bem-sucedido — próxima vez que检测
    /// não tenta mais fallback.
    pub fn confirm_target(&mut self) {
        self.target_confirmed = true;
        self.current = BootSource::Target;
        crate::slog_nano!("ROLLBACK", "info", "Target confirmed — rollback disabled");
    }

    /// Decrementa tentativas. Retorna true se ainda pode tentar.
    pub fn record_boot_failure(&mut self) -> bool {
        if self.remaining_attempts > 0 {
            self.remaining_attempts -= 1;
            crate::slog_nano!("ROLLBACK", "warn",
                "Target boot failed. {} attempts remaining", self.remaining_attempts);
            self.remaining_attempts > 0
        } else {
            crate::slog_nano!("ROLLBACK", "error",
                "All boot attempts exhausted. Pendrive fallback required.");
            false
        }
    }
}

/// Salva configuração de rollback no boot ramlog para persistência.
/// No boot, o bootloader verifica se há pendrive conectado e tenta
/// boot pelo target. Se falhar 3x, marca o target como falho e
/// usa o pendrive como fallback.
pub fn save_rollback_state(config: &RollbackConfig) {
    let state = format!(
        "ROLLBACK source={:?} attempts={} confirmed={}",
        config.current, config.remaining_attempts, config.target_confirmed,
    );
    crate::slog_nano!("ROLLBACK", "info", "{}", state);
}

/// Verifica se o boot atual veio do target ou do pendrive.
/// ponytail: implementação simplificada — checa se o dispositivo
/// de boot tem mais que 1 partição (target tem ESP+NeuralFS).
pub fn detect_boot_source(target: &mut dyn crate::block_dev::BlockDevice) -> BootSource {
    let parts = crate::gpt::probe_gpt(target);
    match parts {
        Some(p) if p.len() >= 2 => BootSource::Target,
        Some(_) => BootSource::Pendrive,
        None => BootSource::Unknown,
    }
}
