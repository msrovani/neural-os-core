//! T-024 K_AI provision gate — NET_READY + SELF.STATE first_boot → ModelProvisioner.
//! R2 gate canônico (ponytail: só verifica, não baixa — download real vive em
//! neural-kernel/src/model_provisioner.rs que reusa este gate).
//! Reused by `hermes::provision` (no duplicate logic).

use k_nano::boot_mode;

/// True iff auto-provision deve rodar: Installed + first_boot.
/// T-024: Live/Install não baixam; Installed sem first_boot também não.
pub fn should_auto_provision() -> bool {
    matches!(boot_mode::peek(), Some(boot_mode::BootMode::Installed))
        && crate::self_state::is_first_boot()
}

/// Texto de diagnóstico para logs/evidence.
pub fn gate_reason() -> &'static str {
    if !matches!(boot_mode::peek(), Some(boot_mode::BootMode::Installed)) {
        "skip not Installed"
    } else if !crate::self_state::is_first_boot() {
        "skip not first_boot"
    } else {
        "PASS Installed+first_boot"
    }
}

/// Hook chamado quando NET_READY dispara. Retorna true se o gate passou.
/// O download real é feito pelo bin (model_provisioner) — aqui só o veredito.
pub fn on_net_ready() -> bool {
    let pass = should_auto_provision();
    if pass {
        k_nano::slog_kai!("PROV", "info", "NET_READY gate PASS → provision (k_ai)");
    } else {
        k_nano::slog_kai!("PROV", "info", "NET_READY gate skip: {}", gate_reason());
    }
    pass
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_nano::boot_mode::{set_boot_mode, BootMode};

    // ponytail: tests share globals → serialize
    static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());

    #[test]
    fn gate_only_installed_first_boot() {
        let _g = TEST_LOCK.lock();
        set_boot_mode(BootMode::Installed);
        // first_boot=true quando SGDB vazio (sem SELF.STATE)
        // Em host sem SGDB, is_first_boot() == true
        let first = crate::self_state::is_first_boot();
        // Se first_boot, gate deve passar
        assert_eq!(should_auto_provision(), first);
        assert_eq!(on_net_ready(), first);

        set_boot_mode(BootMode::Live);
        assert!(!should_auto_provision(), "Live nunca provisiona");
        assert!(!on_net_ready());

        set_boot_mode(BootMode::Install);
        assert!(!should_auto_provision());

        set_boot_mode(BootMode::Unknown);
        assert!(!should_auto_provision());

        // Restaura
        set_boot_mode(BootMode::Unknown);
    }

    #[test]
    fn gate_reason_text() {
        let _g = TEST_LOCK.lock();
        set_boot_mode(BootMode::Live);
        assert!(gate_reason().contains("not Installed"));
        set_boot_mode(BootMode::Installed);
        // se first_boot true → PASS, senão skip not first_boot — ambos válidos
        let r = gate_reason();
        assert!(r.contains("PASS") || r.contains("not first_boot"));
        set_boot_mode(BootMode::Unknown);
    }
}
