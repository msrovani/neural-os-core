use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use event_bus::{Event, CapabilityToken};
use agent_core::{Agent, AgentManifest, AgentKind, ScheduleKind, AgentTickResult};
use k_nano::{EVENT_BUS, interrupts::TIMER_TICKS, slog_kai};
use crate::self_heal::{
    ErrorContext, RecoveryAction, FailedStrategy,
    BudgetedRecovery, SilentFailureDetector,
    GLOBAL_SELF_HEAL, push_respawn,
    TOPIC_HEALING_LLM_REQUEST, TOPIC_HEALING_LLM_RESPONSE,
};

/// Topic for user-visible self-heal notifications (displayed via Hermes/Jarbas)
const TOPIC_SELFHEAL_USER: &str = "SELFHEAL_USER_NOTIFY";

pub struct SelfHealAgent {
    budget: BudgetedRecovery,
    silent: SilentFailureDetector,
    kernel_error_rx: event_bus::Receiver,
    healing_response_rx: event_bus::Receiver,
    /// Pending error context waiting for LLM diagnosis (daemon -> ErrorContext)
    pending_diagnosis: Option<ErrorContext>,
}

impl SelfHealAgent {
    pub fn new() -> Self {
        SelfHealAgent {
            budget: BudgetedRecovery::new(10, 60_000),
            silent: SilentFailureDetector::new(5000),
            kernel_error_rx: EVENT_BUS.subscribe("KERNEL_ERROR"),
            healing_response_rx: EVENT_BUS.subscribe(TOPIC_HEALING_LLM_RESPONSE),
            pending_diagnosis: None,
        }
    }

    // ── NSGDB Integration ────────────────────────────────────────────────────

    /// Ingest an error context into NSGDB for long-term pattern analysis.
    /// Key format: `selfheal/{tick:07}` at L3 (episodic).
    /// Content: structured text with daemon, class, message, tick.
    fn ingest_error_to_nsgdb(ctx: &ErrorContext, tick: u64) {
        let key = format!("selfheal/{:07}", tick);
        let content = format!(
            "daemon={} kind={} msg={} tick={}",
            ctx.daemon, ctx.kind, ctx.message, tick
        );
        // put_kv writes to TickvLite + NSGDB index
        if let Err(e) = crate::sgdb::store::put_kv(
            &format!("md/L3/{}", key),
            content.as_bytes(),
        ) {
            slog_kai!("SELF", "HEAL", "NSGDB ingest failed: {}", e);
        } else {
            slog_kai!("SELF", "HEAL", "NSGDB ingest OK: {}", key);
        }
    }

    /// Ingest a recovery action result into NSGDB.
    /// Key format: `selfheal_result/{tick:07}` at L3.
    fn ingest_recovery_to_nsgdb(action: &str, daemon: &str, tick: u64) {
        let key = format!("selfheal_result/{:07}", tick);
        let content = format!(
            "action={} daemon={} tick={} status=executed",
            action, daemon, tick
        );
        if let Err(e) = crate::sgdb::store::put_kv(
            &format!("md/L3/{}", key),
            content.as_bytes(),
        ) {
            slog_kai!("SELF", "HEAL", "NSGDB result ingest failed: {}", e);
        }
    }

    /// Query NSGDB for similar past errors from the same daemon.
    /// Returns a summary string for LLM context enrichment.
    fn query_nsgdb_for_patterns(daemon: &str) -> String {
        // Lexical search for past selfheal records mentioning this daemon
        let query = format!("selfheal daemon={}", daemon);
        let hits = crate::sgdb::nsgdb_bridge::recall_lexical_bridge(&query, 5);
        if hits.is_empty() {
            return String::new();
        }
        let mut summary = format!("HISTORICAL_ERRORS[{}]: ", hits.len());
        for (i, hit) in hits.iter().enumerate() {
            if i > 0 { summary.push_str("; "); }
            // Truncate text to keep prompt manageable
            let text = if hit.text.len() > 80 { &hit.text[..80] } else { &hit.text };
            summary.push_str(text);
        }
        summary
    }

    /// Query NSGDB for successful recovery patterns (what worked before).
    fn query_nsgdb_for_successful_recoveries(daemon: &str) -> String {
        let query = format!("selfheal_result daemon={} status=executed", daemon);
        let hits = crate::sgdb::nsgdb_bridge::recall_lexical_bridge(&query, 3);
        if hits.is_empty() {
            return String::new();
        }
        let mut summary = format!("PAST_RECOVERIES[{}]: ", hits.len());
        for (i, hit) in hits.iter().enumerate() {
            if i > 0 { summary.push_str("; "); }
            let text = if hit.text.len() > 60 { &hit.text[..60] } else { &hit.text };
            summary.push_str(text);
        }
        summary
    }

    // ── User Notification ─────────────────────────────────────────────────────

    /// Notify user about self-healing action via Hermes/Jarbas.
    /// Publishes to HERMES_RESPONSE so DisplayAgent shows it.
    fn notify_user(action: &str, daemon: &str, reason: &str, tick: u64) {
        let msg = format!(
            "[SelfHeal] {}:{} — {} (t={})",
            action, daemon, reason, tick
        );
        // Publish to Hermes response (displayed on screen via ConsoleAgent/DisplayAgent)
        // HERMES_RESPONSE is the canonical topic — k_ai can't reference hermes crate,
        // so we use the string literal.
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from("HERMES_RESPONSE"),
            payload: msg.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });
        // Also publish to dedicated self-heal topic for future consumers
        let detail = format!("{}|{}|{}", action, daemon, reason);
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from(TOPIC_SELFHEAL_USER),
            payload: detail.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
        slog_kai!("SELF", "HEAL", "User notified: {}", msg);
    }

    // ── AI Diagnosis (Phase 3 + NSGDB enrichment) ────────────────────────────

    /// Phase 3: Apply AI-generated diagnosis from HEALING_LLM_RESPONSE.
    /// Parses the JSON response and executes the recommended action.
    fn apply_ai_diagnosis(&self, response: &str) {
        // Simple JSON parser for: {"action":"restart_daemon","reason":"...","params":{...}}
        let action_start = response.find("\"action\":");
        let reason_start = response.find("\"reason\":");
        let dq: char = '"';
        let bs: char = '\\';

        if let Some(a_pos) = action_start {
            let a_val_start = a_pos + 11; // len of "\"action\":"
            let a_val_end = response[a_val_start..]
                .find(',')
                .unwrap_or(response[a_val_start..].find('}').unwrap_or(0))
                + a_val_start;
            let action_str = response[a_val_start..a_val_end]
                .trim()
                .trim_matches(dq)
                .trim_matches(bs);

            let reason = if let Some(r_pos) = reason_start {
                let r_val_start = r_pos + 10; // len of "\"reason\":"
                let r_val_end = response[r_val_start..]
                    .find(',')
                    .unwrap_or(response[r_val_start..].find('}').unwrap_or(0))
                    + r_val_start;
                response[r_val_start..r_val_end]
                    .trim()
                    .trim_matches(dq)
                    .trim_matches(bs)
            } else {
                ""
            };

            let daemon = self
                .pending_diagnosis
                .as_ref()
                .map(|ctx| ctx.daemon.as_str())
                .unwrap_or("unknown");
            let tick = TIMER_TICKS.load(Ordering::Relaxed) as u64;

            k_nano::slog_kai!("SELF", "HEAL", "AI diagnosis: action='{}' reason='{}'", action_str, reason);

            // Ingest recovery result to NSGDB
            SelfHealAgent::ingest_recovery_to_nsgdb(action_str, daemon, tick);

            match action_str {
                "restart_daemon" => {
                    let pushed = push_respawn(daemon);
                    slog_kai!("SELF", "HEAL", "AI->RestartDaemon '{}' pushed={}", daemon, pushed);
                    let mut heal = GLOBAL_SELF_HEAL.lock();
                    heal.record_failure(daemon.into(), "ai_restart".into(), tick);
                    // Notify user
                    SelfHealAgent::notify_user("restart", daemon, reason, tick);
                }
                "checkpoint_restore" => {
                    slog_kai!("SELF", "HEAL", "AI->CheckpointRestore");
                    let mut heal = GLOBAL_SELF_HEAL.lock();
                    heal.restore_checkpoint();
                    SelfHealAgent::notify_user("checkpoint", daemon, reason, tick);
                }
                "create_skill" => {
                    slog_kai!("SELF", "HEAL", "AI->CreateSkill (reason: {})", reason);
                    let _ = EVENT_BUS.publish(Event {
                        id: 0,
                        topic: "SKILL_CREATE".into(),
                        payload: reason.as_bytes().to_vec(),
                        token: CapabilityToken::Legacy(1),
                    });
                    SelfHealAgent::notify_user("create_skill", daemon, reason, tick);
                }
                "log_continue" => {
                    slog_kai!("SELF", "HEAL", "AI->LogContinue (reason: {})", reason);
                    // No notification for log_continue (too noisy)
                }
                _ => {
                    slog_kai!("SELF", "HEAL", "AI->unknown action '{}', logging only", action_str);
                }
            }
        }
    }

    fn execute_recovery(&self, action: RecoveryAction) {
        let tick = TIMER_TICKS.load(Ordering::Relaxed) as u64;
        match action {
            RecoveryAction::RestartDaemon(name, verify) => {
                slog_kai!("SELF", "HEAL", "RestartDaemon: {} (via RESPAWN_QUEUE bridge)", name);
                let pushed = push_respawn(&name);
                if !pushed {
                    slog_kai!("SELF", "HEAL", "RestartDaemon: bridge not registered -- fallback log only");
                }
                SelfHealAgent::ingest_recovery_to_nsgdb("restart", &name, tick);
                SelfHealAgent::notify_user("restart", &name, "auto-heal restart", tick);
                if let Some(check) = verify {
                    if !check() {
                        let mut heal = GLOBAL_SELF_HEAL.lock();
                        heal.record_failure(name, "restart_failed".into(), tick);
                    }
                }
            }
            RecoveryAction::CreateSkill(daemon, fix, verify) => {
                slog_kai!("SELF", "HEAL", "CreateSkill: {} - {}", daemon, fix);
                {
                    let mut heal = GLOBAL_SELF_HEAL.lock();
                    heal.pending_fixes.push((daemon.clone(), fix.clone()));
                }
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: "SKILL_CREATE".into(),
                    payload: fix.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                SelfHealAgent::ingest_recovery_to_nsgdb("create_skill", &daemon, tick);
                SelfHealAgent::notify_user("create_skill", &daemon, "generating fix", tick);
                if let Some(check) = verify {
                    if !check() {
                        let mut heal = GLOBAL_SELF_HEAL.lock();
                        heal.record_failure(daemon, "create_failed".into(), tick);
                    }
                }
            }
            RecoveryAction::AwaitLLM(daemon) => {
                slog_kai!("SELF", "HEAL", "AwaitLLM: {}", daemon);
                let mut heal = GLOBAL_SELF_HEAL.lock();
                heal.lessons.push(FailedStrategy {
                    error_msg: daemon,
                    attempted_action: "await_llm".into(),
                    tick,
                });
            }
            RecoveryAction::CheckpointRestore => {
                slog_kai!("SELF", "HEAL", "CheckpointRestore requested");
                let mut heal = GLOBAL_SELF_HEAL.lock();
                heal.restore_checkpoint();
                SelfHealAgent::ingest_recovery_to_nsgdb("checkpoint_restore", "system", tick);
                SelfHealAgent::notify_user("checkpoint", "system", "restoring checkpoint", tick);
            }
            RecoveryAction::LogAndContinue => {
                // Nothing to execute.
            }
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
        self.budget.maybe_reset();
        self.silent.set_tick(tick);

        // Phase 3: Process HEALING_LLM_RESPONSE (AI diagnosis from CortexAgent)
        while let Some(event) = self.healing_response_rx.try_receive() {
            let response = core::str::from_utf8(&event.payload).unwrap_or("");
            if !response.is_empty() {
                slog_kai!("SELF", "HEAL", "HEALING_LLM_RESPONSE: {}", response);
                self.apply_ai_diagnosis(response);
            }
        }

        // Process pending KERNEL_ERROR events via canonical GLOBAL_SELF_HEAL.
        while let Some(event) = self.kernel_error_rx.try_receive() {
            if let Ok(ctx) = ErrorContext::from_event_bytes(&event.payload) {
                // ── NSGDB: Ingest every error for historical analysis ──
                SelfHealAgent::ingest_error_to_nsgdb(&ctx, tick);

                if self.budget.can_execute() {
                    self.budget.consume();
                    self.pending_diagnosis = Some(ctx.clone());

                    // ── NSGDB: Query historical patterns before analysis ──
                    let history = SelfHealAgent::query_nsgdb_for_patterns(&ctx.daemon);
                    let past_recoveries = SelfHealAgent::query_nsgdb_for_successful_recoveries(&ctx.daemon);
                    if !history.is_empty() {
                        slog_kai!("SELF", "HEAL", "NSGDB history for '{}': {}", ctx.daemon, history);
                    }
                    if !past_recoveries.is_empty() {
                        slog_kai!("SELF", "HEAL", "NSGDB past recoveries for '{}': {}", ctx.daemon, past_recoveries);
                    }

                    let action = {
                        let mut heal = GLOBAL_SELF_HEAL.lock();
                        heal.analyze(&ctx, true)
                    };
                    self.execute_recovery(action);
                } else {
                    slog_kai!("SELF", "HEAL", "budget exhausted -- logging only");
                    let mut heal = GLOBAL_SELF_HEAL.lock();
                    let _ = heal.analyze(&ctx, false);
                }
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
