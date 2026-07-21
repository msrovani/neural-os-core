use alloc::format;
use core::sync::atomic::Ordering;
use event_bus::{Event, CapabilityToken};
use agent_core::{Agent, AgentManifest, AgentKind, ScheduleKind, AgentTickResult};
use k_nano::{EVENT_BUS, interrupts::TIMER_TICKS, slog_kai};
use crate::self_heal::{SelfHeal, ErrorContext, RecoveryAction, FailedStrategy, BudgetedRecovery, SilentFailureDetector};

pub struct SelfHealAgent {
    heal: SelfHeal,
    budget: BudgetedRecovery,
    silent: SilentFailureDetector,
    kernel_error_rx: event_bus::Receiver,
}

impl SelfHealAgent {
    pub fn new() -> Self {
        SelfHealAgent {
            heal: SelfHeal::new(),
            budget: BudgetedRecovery::new(5, 60_000),
            silent: SilentFailureDetector::new(5000),
            kernel_error_rx: EVENT_BUS.subscribe("KERNEL_ERROR"),
        }
    }
    
    fn execute_recovery(&mut self, action: RecoveryAction) {
        match action {
            RecoveryAction::RestartDaemon(name, verify) => {
                slog_kai!("SELF", "HEAL", "RestartDaemon: {}", name);
                let msg = name.clone();
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: "DAEMON_RESPAWN".into(),
                    payload: msg.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                if let Some(check) = verify {
                    if !check() { self.heal.record_failure(name, "restart".into(), TIMER_TICKS.load(Ordering::Relaxed) as u64); }
                }
            }
            RecoveryAction::CreateSkill(daemon, fix, verify) => {
                slog_kai!("SELF", "HEAL", "CreateSkill: {} - {}", daemon, fix);
                self.heal.pending_fixes.push((daemon.clone(), fix.clone()));
                let msg = fix.clone();
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: "SKILL_CREATE".into(),
                    payload: msg.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                if let Some(check) = verify {
                    if !check() { self.heal.record_failure(daemon, "create".into(), TIMER_TICKS.load(Ordering::Relaxed) as u64); }
                }
            }
            RecoveryAction::AwaitLLM(daemon) => {
                slog_kai!("SELF", "HEAL", "AwaitLLM: {}", daemon);
                self.heal.lessons.push(FailedStrategy { 
                    error_msg: daemon.clone(), 
                    attempted_action: "await_llm".into(), 
                    tick: TIMER_TICKS.load(Ordering::Relaxed) as u64 
                });
            }
            _ => {}
        }
    }
}

impl Agent for SelfHealAgent {
    fn manifest(&self) -> &AgentManifest {
        static MANIFEST: AgentManifest = AgentManifest {
            name: "SelfHealAgent",
            kind: AgentKind::System,
            schedule: ScheduleKind::PollEvery(1000),
            auto_start: true,
            persist: false,
        };
        &MANIFEST
    }
    
    fn tick(&mut self, tick: u64, _tick_count: u64) -> AgentTickResult {
        self.budget.set_tick(tick);
        self.silent.set_tick(tick);
        
        // Process pending KERNEL_ERROR events
        while let Some(event) = self.kernel_error_rx.try_receive() {
            if let Ok(ctx) = ErrorContext::from_event_bytes(&event.payload) {
                let action = self.heal.analyze(&ctx, true);
                self.execute_recovery(action);
            }
        }
        
        // Self-health heartbeat
        self.silent.heartbeat("SelfHealAgent");
        
        // Detect silent agents
        for agent in self.silent.detect_silent() {
            let msg = format!("I5:{}:silent", agent);
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: "HEALTH_ISSUE".into(),
                payload: msg.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }
        
        AgentTickResult::Done
    }
}

