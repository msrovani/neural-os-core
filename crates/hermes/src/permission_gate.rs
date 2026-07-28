//! Permission Gate — conecta o `Verdict::Escalate` do Membrane ao HITL.
//! Inspirado por Folkering OS: "tool-call Permission Gate — Y/N gates land
//! on the kernel ringbuffer before the syscall returns."
//!
//! Flow:
//! 1. WASM skill chama host function perigosa
//! 2. check_cap() → bitmask OK, Membrane → Escalate
//! 3. PermissionGate classifica o risco via ApprovalGate
//! 4. Se Auto → Allow, se Deny → trap, se Confirm/Escalate → submete HITL
//! 5. Spincia até usuário aprovar/negar (fail-closed I3: timeout → Deny)

use crate::membrane::Verdict;

/// Nível de risco de uma host function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Auto-aprovado (log, debug, read)
    Auto,
    /// Confirmação simples (write, config)
    Confirm,
    /// Requer aprovação explícita (rede, exec, HW)
    Escalate,
    /// Sempre negado (mmio raw, dma)
    Deny,
}

impl RiskLevel {
    /// Mapeia namespace + nome da host function para RiskLevel.
    pub fn classify(namespace: &str, name: &str) -> Self {
        let full = alloc::format!("{}::{}", namespace, name);
        match full.as_str() {
            // Auto: operações seguras
            "aios::log" | "aios::debug" | "aios::get_tick" => RiskLevel::Auto,
            // Confirm: operações com efeito
            "aios_fs::fs_read" => RiskLevel::Confirm,
            // Escalate: perigosas
            "aios_fs::fs_write" | "aios_net::http_get" => RiskLevel::Escalate,
            // Deny: críticas
            _ if name.contains("dma") || name.contains("mmio") => RiskLevel::Deny,
            // Default: Confirm (seguro)
            _ => RiskLevel::Confirm,
        }
    }
}

/// Resultado do Permission Gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionVerdict {
    Allow,
    Deny,
    Pending { id: u64 },
}

/// Permission Gate — decide se uma operação pode prosseguir.
pub struct PermissionGate;

impl PermissionGate {
    /// Verifica permissão para uma host function.
    /// Retorna Allow, Deny, ou Pending (esperando HITL).
    pub fn check(
        namespace: &str,
        name: &str,
        membrane_verdict: Verdict,
    ) -> PermissionVerdict {
        // Se a Membrane já negou, nem tenta
        if membrane_verdict == Verdict::Deny {
            k_nano::telemetry::TELEMETRY.push(4, 0, &[0; 32]); // EV_CAP_DENY
            return PermissionVerdict::Deny;
        }

        // Se a Membrane já permitiu, passa direto
        if membrane_verdict == Verdict::Allow {
            k_nano::telemetry::TELEMETRY.push(5, 0, &[0; 32]); // EV_CAP_ALLOW
            return PermissionVerdict::Allow;
        }

        // Escalate: classifica por risco
        let risk = RiskLevel::classify(namespace, name);
        match risk {
            RiskLevel::Auto => {
                k_nano::telemetry::TELEMETRY.push(5, 0, &[0; 32]);
                PermissionVerdict::Allow
            }
            RiskLevel::Deny => {
                k_nano::telemetry::TELEMETRY.push(4, 0, &[0; 32]);
                PermissionVerdict::Deny
            }
            RiskLevel::Confirm | RiskLevel::Escalate => {
                // Submete ao ApprovalGate + HITL
                let full_name = alloc::format!("{}::{}", namespace, name);
                let reason = alloc::format!("Permission Gate: {} requer aprovação (risco={:?})",
                    full_name, risk);
                let level = if risk == RiskLevel::Escalate {
                    crate::approval::ApprovalLevel::Escalate
                } else {
                    crate::approval::ApprovalLevel::Confirm
                };

                let id = crate::globals::APPROVAL_GATE.lock().request(
                    &full_name, "wasm", &reason, level,
                );

                k_nano::slog_hermes!("PERM", "info",
                    "Gate #{}: {} risk={:?} — waiting HITL", id, full_name, risk);

                // Spin-loop aguardando aprovação (fail-closed I3)
                let full_name_clone = full_name.clone();
                let result = Self::wait_for_approval(id, &full_name_clone);

                k_nano::telemetry::TELEMETRY.push(
                    if result { 5 } else { 4 }, // CAP_ALLOW ou CAP_DENY
                    0, &id.to_ne_bytes(),
                );

                if result {
                    PermissionVerdict::Allow
                } else {
                    PermissionVerdict::Deny
                }
            }
        }
    }

    /// Spin-loop blocking wait por aprovação HITL.
    /// Timeout ~10000 iterações (~1s) — fail-closed I3: timeout → Deny.
    fn wait_for_approval(id: u64, skill: &str) -> bool {
        let max_retries: u64 = 10_000;
        for i in 0..max_retries {
            // Verifica se foi resolvido
            let gate = crate::globals::APPROVAL_GATE.lock();
            let can = gate.can_execute(skill);
            drop(gate);

            if can {
                return true;
            }

            // Verifica se foi explicitamente negado
            let gate = crate::globals::APPROVAL_GATE.lock();
            let denied = gate.pending().iter().any(|r| r.id == id && r.resolved && !r.approved);
            drop(gate);

            if denied {
                k_nano::slog_hermes!("PERM", "warn", "Gate #{}: denied by user", id);
                return false;
            }

            // Spin-loop hint (evita que o compilador otimize o loop)
            core::hint::spin_loop();

            // A cada 1000 iterações, loga que está esperando
            if i % 1000 == 999 {
                k_nano::slog_hermes!("PERM", "info",
                    "Gate #{}: waiting... ({}/{})", id, i + 1, max_retries);
            }
        }

        // Timeout — fail-closed I3
        k_nano::slog_hermes!("PERM", "error",
            "Gate #{}: TIMEOUT — denied by I3 fail-closed", id);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_classify() {
        assert_eq!(RiskLevel::classify("aios", "log"), RiskLevel::Auto);
        assert_eq!(RiskLevel::classify("aios", "get_tick"), RiskLevel::Auto);
        assert_eq!(RiskLevel::classify("aios_fs", "fs_read"), RiskLevel::Confirm);
        assert_eq!(RiskLevel::classify("aios_fs", "fs_write"), RiskLevel::Escalate);
        assert_eq!(RiskLevel::classify("aios_net", "http_get"), RiskLevel::Escalate);
    }

    #[test]
    fn test_permission_gate_auto() {
        let result = PermissionGate::check("aios", "log", Verdict::Allow);
        assert_eq!(result, PermissionVerdict::Allow);
    }

    #[test]
    fn test_permission_gate_membrane_deny() {
        let result = PermissionGate::check("aios", "anything", Verdict::Deny);
        assert_eq!(result, PermissionVerdict::Deny);
    }

    #[test]
    fn test_permission_gate_dma_is_deny() {
        let result = PermissionGate::check("aios", "dma_alloc", Verdict::Escalate);
        assert_eq!(result, PermissionVerdict::Deny);
    }
}
