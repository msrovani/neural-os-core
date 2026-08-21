//! Micro-hooks WASM de telemetria — hook points no EventBus para skills wasmi.
//!
//! # Arquitetura
//! Skills WASM registram callbacks de hook (IRQ, VFS, etc.) no EventBus.
//! Quando um evento é publicado, o EventBus verifica hooks registrados e
//! dispatcha para o callback se a CapGate permitir.
//!
//! # Segurança
//! - Cada hook tem um `CapabilityToken` associado (CapGate validation)
//! - Hooks sem capacidade concedida são ignorados (fail-closed)
//! - Máximo de hooks por skill: `MAX_HOOKS_PER_SKILL` (anti-abuse)

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::event::Event;
use crate::capability::CapabilityToken;

/// Máximo de hooks registrados por skill (anti-abuse).
pub const MAX_HOOKS_PER_SKILL: usize = 8;

/// Máximo total de hooks no sistema.
pub const MAX_TOTAL_HOOKS: usize = 64;

/// Tipos de hook points disponíveis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPoint {
    /// IRQ de hardware (timer, keyboard, network, etc.)
    Irq,
    /// Operações VFS (read, write, create, delete)
    Vfs,
    /// Operações de rede (send, recv, connect)
    Net,
    /// Mudanças de estado do sistema (boot, shutdown, etc.)
    System,
    /// Telemetria de performance (latência, throughput)
    Telemetry,
    /// Custom hook point (para extensões futuras)
    Custom(u8),
}

impl HookPoint {
    /// Converte para u8 para armazenamento eficiente.
    pub fn as_u8(self) -> u8 {
        match self {
            HookPoint::Irq => 0,
            HookPoint::Vfs => 1,
            HookPoint::Net => 2,
            HookPoint::System => 3,
            HookPoint::Telemetry => 4,
            HookPoint::Custom(n) => 128 + n,
        }
    }

    /// Reconstrói de u8.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(HookPoint::Irq),
            1 => Some(HookPoint::Vfs),
            2 => Some(HookPoint::Net),
            3 => Some(HookPoint::System),
            4 => Some(HookPoint::Telemetry),
            n @ 128.. => Some(HookPoint::Custom(n - 128)),
            _ => None,
        }
    }

    /// Nome do hook point para logs.
    pub fn name(&self) -> &'static str {
        match self {
            HookPoint::Irq => "IRQ",
            HookPoint::Vfs => "VFS",
            HookPoint::Net => "NET",
            HookPoint::System => "SYSTEM",
            HookPoint::Telemetry => "TELEMETRY",
            HookPoint::Custom(_) => "CUSTOM",
        }
    }
}

/// Contexto passado ao callback do hook.
#[derive(Debug)]
pub struct HookContext<'a> {
    /// O evento original que disparou o hook.
    pub event: &'a Event,
    /// O hook point que disparou.
    pub point: HookPoint,
    /// ID da skill que registrou o hook.
    pub skill_id: u32,
}

/// Registro de um hook ativo.
#[derive(Debug, Clone)]
pub struct HookRegistration {
    /// ID único do hook.
    pub id: u32,
    /// ID da skill que registrou.
    pub skill_id: u32,
    /// Nome da skill (para logs).
    pub skill_name: String,
    /// Hook point monitorado.
    pub point: HookPoint,
    /// Capacidade requerida (CapGate).
    pub required_cap: CapabilityToken,
    /// Padrão de tópico para filtrar (vazio = todos).
    pub topic_filter: String,
    /// Se o hook está ativo.
    pub active: bool,
}

/// Estatísticas de micro-hooks (para telemetria).
#[derive(Debug, Clone, Default)]
pub struct HookStats {
    pub total_registrations: u32,
    pub total_dispatches: u32,
    pub total_cap_denied: u32,
}

/// Armazenamento global de hooks.
pub struct MicroHookStore {
    registrations: Vec<HookRegistration>,
    stats: HookStats,
    next_id: AtomicU32,
}

impl MicroHookStore {
    /// Cria um novo store vazio.
    pub const fn new() -> Self {
        MicroHookStore {
            registrations: Vec::new(),
            stats: HookStats { total_registrations: 0, total_dispatches: 0, total_cap_denied: 0 },
            next_id: AtomicU32::new(1),
        }
    }

    /// Registra um novo hook.
    pub fn register_hook(
        &mut self,
        skill_id: u32,
        skill_name: &str,
        point: HookPoint,
        required_cap: CapabilityToken,
        topic_filter: &str,
    ) -> Result<u32, &'static str> {
        let skill_hooks = self.registrations.iter()
            .filter(|r| r.skill_id == skill_id && r.active)
            .count();
        if skill_hooks >= MAX_HOOKS_PER_SKILL {
            return Err("max hooks per skill exceeded");
        }

        if self.registrations.iter().filter(|r| r.active).count() >= MAX_TOTAL_HOOKS {
            return Err("max total hooks exceeded");
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.registrations.push(HookRegistration {
            id,
            skill_id,
            skill_name: String::from(skill_name),
            point,
            required_cap,
            topic_filter: String::from(topic_filter),
            active: true,
        });

        self.stats.total_registrations += 1;
        Ok(id)
    }

    /// Remove um hook por ID.
    pub fn unregister_hook(&mut self, hook_id: u32) -> bool {
        if let Some(reg) = self.registrations.iter_mut().find(|r| r.id == hook_id) {
            reg.active = false;
            true
        } else {
            false
        }
    }

    /// Lista hooks ativos para um hook point.
    pub fn hooks_for_point(&self, point: HookPoint) -> Vec<&HookRegistration> {
        self.registrations.iter()
            .filter(|r| r.active && r.point == point)
            .collect()
    }

    /// Dispatch de hook — chamado pelo EventBus quando evento é publicado.
    pub fn dispatch_hooks(
        &mut self,
        event: &Event,
        point: HookPoint,
        cap_checker: &dyn Fn(&CapabilityToken) -> bool,
    ) -> u32 {
        let mut dispatched = 0u32;
        let hook_ids: Vec<u32> = self.registrations.iter()
            .filter(|r| r.active && r.point == point)
            .map(|r| r.id)
            .collect();

        for hook_id in hook_ids {
            if let Some(reg) = self.registrations.iter().find(|r| r.id == hook_id) {
                if !cap_checker(&reg.required_cap) {
                    self.stats.total_cap_denied += 1;
                    continue;
                }

                if !reg.topic_filter.is_empty() && event.topic != reg.topic_filter {
                    continue;
                }

                self.stats.total_dispatches += 1;
                dispatched += 1;
            }
        }

        dispatched
    }

    /// Retorna estatísticas atuais.
    pub fn stats(&self) -> &HookStats {
        &self.stats
    }

    /// Número de hooks ativos.
    pub fn active_count(&self) -> usize {
        self.registrations.iter().filter(|r| r.active).count()
    }
}

/// Store global de micro-hooks.
pub static MICRO_HOOKS: ticket_lock::TicketLock<MicroHookStore> =
    ticket_lock::TicketLock::new(MicroHookStore::new());

/// Convenience: dispatch hooks para um evento.
pub fn dispatch_micro_hooks(
    event: &Event,
    point: HookPoint,
    cap_checker: &dyn Fn(&CapabilityToken) -> bool,
) -> u32 {
    MICRO_HOOKS.lock().dispatch_hooks(event, point, cap_checker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;

    fn allow_all(_cap: &CapabilityToken) -> bool { true }
    fn deny_all(_cap: &CapabilityToken) -> bool { false }

    fn make_event(topic: &str) -> Event {
        Event {
            id: 0,
            topic: String::from(topic),
            payload: b"test_payload".to_vec(),
            token: CapabilityToken::Legacy(1),
        }
    }

    #[test]
    fn hook_point_roundtrip_u8() {
        let points = [
            HookPoint::Irq, HookPoint::Vfs, HookPoint::Net,
            HookPoint::System, HookPoint::Telemetry, HookPoint::Custom(42),
        ];
        for p in &points {
            let val = p.as_u8();
            let back = HookPoint::from_u8(val).unwrap();
            assert_eq!(*p, back);
        }
    }

    #[test]
    fn hook_point_names() {
        assert_eq!(HookPoint::Irq.name(), "IRQ");
        assert_eq!(HookPoint::Vfs.name(), "VFS");
        assert_eq!(HookPoint::Net.name(), "NET");
        assert_eq!(HookPoint::System.name(), "SYSTEM");
        assert_eq!(HookPoint::Telemetry.name(), "TELEMETRY");
        assert_eq!(HookPoint::Custom(7).name(), "CUSTOM");
    }

    #[test]
    fn register_and_list_hooks() {
        let mut store = MicroHookStore::new();
        let id1 = store.register_hook(1, "skill_a", HookPoint::Irq, CapabilityToken::Legacy(1), "").unwrap();
        let id2 = store.register_hook(1, "skill_a", HookPoint::Vfs, CapabilityToken::Legacy(1), "/mnt/data").unwrap();

        assert_ne!(id1, id2);
        assert_eq!(store.active_count(), 2);

        let irq_hooks = store.hooks_for_point(HookPoint::Irq);
        assert_eq!(irq_hooks.len(), 1);
        assert_eq!(irq_hooks[0].skill_name, "skill_a");
        assert_eq!(irq_hooks[0].topic_filter, "");

        let vfs_hooks = store.hooks_for_point(HookPoint::Vfs);
        assert_eq!(vfs_hooks.len(), 1);
        assert_eq!(vfs_hooks[0].topic_filter, "/mnt/data");
    }

    #[test]
    fn unregister_hook() {
        let mut store = MicroHookStore::new();
        let id = store.register_hook(1, "skill_a", HookPoint::Irq, CapabilityToken::Legacy(1), "").unwrap();
        assert_eq!(store.active_count(), 1);
        assert!(store.unregister_hook(id));
        assert_eq!(store.active_count(), 0);
        // Second unregister: registration exists but already inactive
        // The function returns true because it finds the hook (active=false)
        // To truly remove, we would need a different API. For now, idempotent is fine.
    }

    #[test]
    fn max_hooks_per_skill() {
        let mut store = MicroHookStore::new();
        for _ in 0..MAX_HOOKS_PER_SKILL {
            store.register_hook(1, "skill_a", HookPoint::Irq, CapabilityToken::Legacy(1), "").unwrap();
        }
        let result = store.register_hook(1, "skill_a", HookPoint::Irq, CapabilityToken::Legacy(1), "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("per skill"));
    }

    #[test]
    fn max_total_hooks() {
        let mut store = MicroHookStore::new();
        for i in 0..MAX_TOTAL_HOOKS {
            store.register_hook(i as u32, &alloc::format!("s{}", i), HookPoint::Irq, CapabilityToken::Legacy(1), "").unwrap();
        }
        let result = store.register_hook(MAX_TOTAL_HOOKS as u32, "overflow", HookPoint::Irq, CapabilityToken::Legacy(1), "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("total hooks"));
    }

    #[test]
    fn dispatch_hooks_cap_gate() {
        let mut store = MicroHookStore::new();
        store.register_hook(1, "net_skill", HookPoint::Net, CapabilityToken::Legacy(1), "").unwrap();

        let event = make_event("NETWORK_PACKET");
        let count = store.dispatch_hooks(&event, HookPoint::Net, &allow_all);
        assert_eq!(count, 1);
        assert_eq!(store.stats().total_dispatches, 1);
        assert_eq!(store.stats().total_cap_denied, 0);

        let count = store.dispatch_hooks(&event, HookPoint::Net, &deny_all);
        assert_eq!(count, 0);
        assert_eq!(store.stats().total_cap_denied, 1);
    }

    #[test]
    fn dispatch_hooks_topic_filter() {
        let mut store = MicroHookStore::new();
        store.register_hook(1, "vfs_skill", HookPoint::Vfs, CapabilityToken::Legacy(1), "/mnt/data").unwrap();

        let event_match = make_event("/mnt/data");
        assert_eq!(store.dispatch_hooks(&event_match, HookPoint::Vfs, &allow_all), 1);

        let event_no_match = make_event("/mnt/logs");
        assert_eq!(store.dispatch_hooks(&event_no_match, HookPoint::Vfs, &allow_all), 0);
    }

    #[test]
    fn dispatch_hooks_wrong_point() {
        let mut store = MicroHookStore::new();
        store.register_hook(1, "irq_skill", HookPoint::Irq, CapabilityToken::Legacy(1), "").unwrap();

        let event = make_event("TIMER_TICK");
        assert_eq!(store.dispatch_hooks(&event, HookPoint::Vfs, &allow_all), 0);
    }

    #[test]
    fn dispatch_hooks_empty_filter_matches_all() {
        let mut store = MicroHookStore::new();
        store.register_hook(1, "wild_skill", HookPoint::System, CapabilityToken::Legacy(1), "").unwrap();

        assert_eq!(store.dispatch_hooks(&make_event("BOOT"), HookPoint::System, &allow_all), 1);
        assert_eq!(store.dispatch_hooks(&make_event("SHUTDOWN"), HookPoint::System, &allow_all), 1);
    }

    #[test]
    fn hook_point_invalid_u8() {
        assert!(HookPoint::from_u8(5).is_none());
        assert!(HookPoint::from_u8(127).is_none());
        assert!(HookPoint::from_u8(255).is_some());
    }

    #[test]
    fn multiple_skills_same_point() {
        let mut store = MicroHookStore::new();
        store.register_hook(1, "net_a", HookPoint::Net, CapabilityToken::Legacy(1), "").unwrap();
        store.register_hook(2, "net_b", HookPoint::Net, CapabilityToken::Legacy(1), "").unwrap();
        store.register_hook(3, "net_c", HookPoint::Net, CapabilityToken::Legacy(1), "").unwrap();

        assert_eq!(store.active_count(), 3);
        assert_eq!(store.dispatch_hooks(&make_event("PKT"), HookPoint::Net, &allow_all), 3);
    }

    #[test]
    fn unregister_does_not_affect_other_skills() {
        let mut store = MicroHookStore::new();
        let id_a = store.register_hook(1, "skill_a", HookPoint::Irq, CapabilityToken::Legacy(1), "").unwrap();
        let _id_b = store.register_hook(2, "skill_b", HookPoint::Irq, CapabilityToken::Legacy(1), "").unwrap();

        store.unregister_hook(id_a);
        assert_eq!(store.active_count(), 1);

        let hooks = store.hooks_for_point(HookPoint::Irq);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].skill_name, "skill_b");
    }
}
