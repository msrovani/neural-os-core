//! T-024 hermes provision — hook NET_READY + SELF.STATE first_boot → ModelProvisioner.
//! Reusa `k_ai::provision` (canônico R2); hermes só faz wire via EventBus e log.
//! ponytail: sem download aqui — o bin `model_provisioner` baixa; este módulo só decide.

use k_nano::boot_mode;

/// Re-export do gate canônico (R2) — sem duplicar lógica.
pub use k_ai::provision::should_auto_provision;

/// Verdadeiro se o OS está em estado para auto-provision (Installed + first_boot).
pub fn gate_reason() -> &'static str {
    k_ai::provision::gate_reason()
}

/// Hook para NET_READY (chamado pelo NetAgent ou EventBus).
/// Retorna true se o gate passou — sinal para provisionar.
/// Em hermes, publica `PROVISION_REQUEST` no EventBus para o bin consumir.
pub fn on_net_ready() -> bool {
    let pass = should_auto_provision();
    if pass {
        k_nano::slog_hermes!("PROV", "info", "NET_READY hermes gate PASS → PROVISION_REQUEST");
        let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from("PROVISION_REQUEST"),
            payload: b"auto".to_vec(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
    } else {
        // usa peek para não lockar ATA em host test
        k_nano::slog_hermes!(
            "PROV",
            "info",
            "NET_READY hermes gate skip: installed={:?} first_boot={} reason={}",
            boot_mode::peek(),
            k_ai::self_state::is_first_boot(),
            gate_reason()
        );
    }
    pass
}

/// Payload exemplo para evidência/docs: UPDATE.CFG que o ModelProvisioner consome.
pub fn example_update_cfg() -> alloc::string::String {
    alloc::string::String::from("UPDATE_URL=http://10.0.2.2:8080/UPDATE.MANIFEST\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_nano::boot_mode::{set_boot_mode, BootMode};

    static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());

    #[test]
    fn hermes_gate_delegates_to_kai() {
        let _g = TEST_LOCK.lock();
        set_boot_mode(BootMode::Installed);
        let kai = k_ai::provision::should_auto_provision();
        assert_eq!(should_auto_provision(), kai);
        assert_eq!(on_net_ready(), kai);
        set_boot_mode(BootMode::Live);
        assert!(!should_auto_provision());
        set_boot_mode(BootMode::Unknown);
    }

    #[test]
    fn example_payload() {
        let cfg = example_update_cfg();
        assert!(cfg.contains("UPDATE_URL="));
        assert!(cfg.contains("10.0.2.2:8080"));
    }
}
