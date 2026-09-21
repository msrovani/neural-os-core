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
            slog_kai!("SELF", "warn", "NSGDB ingest failed: {}", e);
        } else {
            slog_kai!("SELF", "ok", "NSGDB ingest OK: {}", key);
        }
    }

    /// Ingest a recovery action result into NSGDB.
    /// Key format: `selfheal_result/{tick:07}` at L3.
    /// Honesty: `status` must reflect real outcome (`executed` only if Act succeeded).
    fn ingest_recovery_to_nsgdb(action: &str, daemon: &str, tick: u64, status: &str) {
        let key = format!("selfheal_result/{:07}", tick);
        let content = format!(
            "action={} daemon={} tick={} status={}",
            action, daemon, tick, status
        );
        if let Err(e) = crate::sgdb::store::put_kv(
            &format!("md/L3/{}", key),
            content.as_bytes(),
        ) {
            slog_kai!("SELF", "warn", "NSGDB result ingest failed: {}", e);
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
        slog_kai!("SELF", "ok", "User notified: {}", msg);
    }

    // ── AI Diagnosis (Phase 3 + NSGDB enrichment) ────────────────────────────

    /// Phase 3: Apply AI-generated diagnosis from HEALING_LLM_RESPONSE.
    /// Parses the JSON response and executes the recommended action.
    fn apply_ai_diagnosis(&self, response: &str) {
        // `"action":` / `"reason":` = 9 chars each. Old code used +11/+10 (off-by-two).
        fn json_string_after(hay: &str, key: &str) -> Option<String> {
            let pos = hay.find(key)?;
            let mut i = pos + key.len();
            let b = hay.as_bytes();
            while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
                i += 1;
            }
            if i >= b.len() || b[i] != b'"' {
                return None;
            }
            i += 1;
            let start = i;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = i.saturating_add(2);
                    continue;
                }
                if b[i] == b'"' {
                    return Some(String::from(
                        core::str::from_utf8(&b[start..i]).unwrap_or(""),
                    ));
                }
                i += 1;
            }
            None
        }

        let Some(action_str) = json_string_after(response, "\"action\":") else {
            slog_kai!("SELF", "warn", "AI diagnosis: no \"action\" field");
            return;
        };
        let reason_owned = json_string_after(response, "\"reason\":");
        let reason = reason_owned.as_deref().unwrap_or("");

        let daemon = self
            .pending_diagnosis
            .as_ref()
            .map(|ctx| ctx.daemon.as_str())
            .unwrap_or("unknown");
        let tick = TIMER_TICKS.load(Ordering::Relaxed) as u64;

        k_nano::slog_kai!(
            "SELF",
            "ok",
            "AI diagnosis: action='{}' reason='{}'",
            action_str,
            reason
        );

        match action_str.as_str() {
            "restart_daemon" => {
                let pushed = push_respawn(daemon);
                let status = if pushed { "executed" } else { "bridge_missing" };
                slog_kai!(
                    "SELF",
                    if pushed { "ok" } else { "warn" },
                    "AI->RestartDaemon '{}' pushed={}",
                    daemon,
                    pushed
                );
                SelfHealAgent::ingest_recovery_to_nsgdb(&action_str, daemon, tick, status);
                let mut heal = GLOBAL_SELF_HEAL.lock();
                heal.record_failure(daemon.into(), "ai_restart".into(), tick);
                if pushed {
                    SelfHealAgent::notify_user("restart", daemon, reason, tick);
                } else {
                    slog_kai!("SELF", "warn", "AI restart skipped notify — bridge missing");
                }
            }
            "checkpoint_restore" => {
                slog_kai!("SELF", "ok", "AI->CheckpointRestore");
                let ok = {
                    let mut heal = GLOBAL_SELF_HEAL.lock();
                    heal.restore_checkpoint()
                };
                let status = if ok { "executed" } else { "restore_failed" };
                SelfHealAgent::ingest_recovery_to_nsgdb(&action_str, daemon, tick, status);
                if ok {
                    SelfHealAgent::notify_user("checkpoint", daemon, reason, tick);
                } else {
                    slog_kai!("SELF", "warn", "AI checkpoint restore failed — no notify");
                }
            }
            "create_skill" => {
                slog_kai!("SELF", "ok", "AI->CreateSkill (reason: {})", reason);
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: "SKILL_CREATE".into(),
                    payload: reason.as_bytes().to_vec(),
                    token: CapabilityToken::Legacy(1),
                });
                SelfHealAgent::ingest_recovery_to_nsgdb(&action_str, daemon, tick, "attempted");
                SelfHealAgent::notify_user("create_skill", daemon, reason, tick);
            }
            "log_continue" => {
                slog_kai!("SELF", "ok", "AI->LogContinue (reason: {})", reason);
                SelfHealAgent::ingest_recovery_to_nsgdb(&action_str, daemon, tick, "logged");
            }
            _ => {
                slog_kai!(
                    "SELF",
                    "warn",
                    "AI->unknown action '{}', logging only",
                    action_str
                );
                SelfHealAgent::ingest_recovery_to_nsgdb(&action_str, daemon, tick, "unknown");
            }
        }
    }

    fn execute_recovery(&self, action: RecoveryAction) {
        let tick = TIMER_TICKS.load(Ordering::Relaxed) as u64;
        match action {
            RecoveryAction::RestartDaemon(name, verify) => {
                slog_kai!("SELF", "ok", "RestartDaemon: {} (via RESPAWN_QUEUE bridge)", name);
                let pushed = push_respawn(&name);
                let status = if pushed { "executed" } else { "bridge_missing" };
                if !pushed {
                    slog_kai!("SELF", "warn", "RestartDaemon: bridge not registered -- fallback log only");
                }
                SelfHealAgent::ingest_recovery_to_nsgdb("restart", &name, tick, status);
                if pushed {
                    SelfHealAgent::notify_user("restart", &name, "auto-heal restart", tick);
                }
                if let Some(check) = verify {
                    if !check() {
                        let mut heal = GLOBAL_SELF_HEAL.lock();
                        heal.record_failure(name, "restart_failed".into(), tick);
                    }
                }
            }
            RecoveryAction::CreateSkill(daemon, fix, verify) => {
                slog_kai!("SELF", "ok", "CreateSkill: {} - {}", daemon, fix);
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
                SelfHealAgent::ingest_recovery_to_nsgdb("create_skill", &daemon, tick, "attempted");
                SelfHealAgent::notify_user("create_skill", &daemon, "generating fix", tick);
                if let Some(check) = verify {
                    if !check() {
                        let mut heal = GLOBAL_SELF_HEAL.lock();
                        heal.record_failure(daemon, "create_failed".into(), tick);
                    }
                }
            }
            RecoveryAction::AwaitLLM(daemon) => {
                slog_kai!("SELF", "warn", "AwaitLLM: {}", daemon);
                let mut heal = GLOBAL_SELF_HEAL.lock();
                heal.lessons.push(FailedStrategy {
                    error_msg: daemon,
                    attempted_action: "await_llm".into(),
                    tick,
                });
            }
            RecoveryAction::CheckpointRestore => {
                slog_kai!("SELF", "ok", "CheckpointRestore requested");
                let ok = {
                    let mut heal = GLOBAL_SELF_HEAL.lock();
                    heal.restore_checkpoint()
                };
                let status = if ok { "executed" } else { "restore_failed" };
                SelfHealAgent::ingest_recovery_to_nsgdb("checkpoint_restore", "system", tick, status);
                if ok {
                    SelfHealAgent::notify_user("checkpoint", "system", "restoring checkpoint", tick);
                } else {
                    slog_kai!("SELF", "warn", "CheckpointRestore failed — no notify");
                }
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
                slog_kai!("SELF", "warn", "HEALING_LLM_RESPONSE: {}", response);
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
                        slog_kai!("SELF", "warn", "NSGDB history for '{}': {}", ctx.daemon, history);
                    }
                    if !past_recoveries.is_empty() {
                        slog_kai!("SELF", "warn", "NSGDB past recoveries for '{}': {}", ctx.daemon, past_recoveries);
                    }

                    let action = {
                        let mut heal = GLOBAL_SELF_HEAL.lock();
                        heal.analyze(&ctx, true)
                    };
                    self.execute_recovery(action);
                } else {
                    slog_kai!("SELF", "warn", "budget exhausted -- logging only");
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
