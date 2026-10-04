# DEAD_WEIGHT AUDIT — neural-os-core Agent Ecosystem
# Per OPCODE/1 Mission §3 and §15 (maintainer guidelines)

> **STATUS (s449 + s451):** #1,#2,#3,#5,#6 IMPLEMENTED (s449); #7 INVERTED; #4,#8,#9 NOT done. **s451:** 36 modulos legados/planejados ARQUIVADOS (git mv, nao delete) em `docs/archive/dead-modules/` + SINAL em AGENTS.md/n-sgdb; `tools/check_duplication.py` corrigido (~20/33 falsos positivos ignorados). This file is a pre-fix snapshot.

## Audit Scope
Read-only audit of all crates in `C:\DEV\neural-os-core-latest\crates/` plus `neural-kernel/`, `skills/`, `tools/`. 
Classified per taxonomy: OBSERVED | VERIFIED | IMPLEMENTED | UNKNOWN | BLOCKED | DEAD_WEIGHT.
Rule: NÃO transformar UNKNOWN→0, IMPLEMENTED→DONE, HOST TEST PASS→RUNTIME PASS.

---

## RANKED DEAD_WEIGHT FINDINGS (most impactful first)

### 1. MatrixLearningAgent — redundant polling + duplicated functionality — ✅ IMPLEMENTED (s449)
- **File**: `crates/hermes/src/agents/log_analyst_agent.rs` (manifest + struct), `crates/hermes/src/matrix_learn.rs` (pipeline)
- **Line**: Manifest at log_analyst_agent.rs:415-421 (PollEvery(200)); tick at :448-477
- **Classification**: DEAD_WEIGHT (OBSERVED: agent tick never produces output not already handled by HermesAgent::tick() inline at agents.rs:1232-1357)
- **Evidence**: 
  - `MatrixLearningAgent::tick()` (matrix_learn.rs:448-477) receives USER_INTENT events and routes through `handle_learning_request()` → publishes HERMES_RESPONSE
  - `HermesAgent::tick()` (agents.rs:1232-1357) already handles ALL commands including learning requests via `hermes::parse_command()` → `Command::Learn` → full dispatch
  - The docstring admits: "This agent provides redundancy and future extensibility for non-Chat learning request paths" — currently zero value
  - No unique code path; same `handle_learning_request → approval gate → publish` flow exists in Hermes
- **Minimal case**: Remove `MatrixLearningAgent` entirely. If future need arises, convert to event-driven only (subscribe to USER_INTENT without PollEvery, handle in Hermes tick).
- **Breaks**: None — Hermes already handles learning requests. Removing this agent has zero functional impact.

### 2. BootLogAgent redundant polling — PollEvery(32) after one-shot analysis — ✅ IMPLEMENTED (s449)
- **File**: `crates/k_ai/src/boot_log_agent.rs`
- **Line**: PollEvery(32) at :10; tick at :175-219; `self.analyzed = true` guard at :186
- **Classification**: DEAD_WEIGHT (OBSERVED: after first FAT walk, tick drains BOOT_PHASE events without re-analyzing)
- **Evidence**: 
  - `self.analyzed` flag (line 186) prevents re-walk after first analysis — makes subsequent polling a no-op for the FAT work
  - Tick only drains `BOOT_PHASE` events via `boot_phase_rx` (line 176-179) and calls periodic push function (line 180-184)
  - The one-time FAT walk at lines 63-114 should run during init_phase, not via continuous polling
- **Minimal case**: Change manifest to `ScheduleKind::Oneshot` and invoke boot log analysis once during `init_platform_sync` or early boot. Remove PollEvery entirely. If periodic boot log tailing is needed, use a much longer interval (e.g., PollEvery(60000)) or make it event-driven.
- **Breaks**: Would need to ensure boot log analysis runs once during boot initialization. The `read_last_boot_log()` function could be called from `k_nano::init` instead.

### 3. SelfLearningAgent — heavyweight learning with artificial test states — ✅ IMPLEMENTED (s449)
- **File**: `crates/k_ai/src/self_learning.rs`
- **Line**: Manifest at :23-29 (PollEvery(500)); tick at :340-343 calls `learn_tick()`; demo test at :409-432; learner_self_test at :435-487
- **Classification**: DEAD_WEIGHT (UNKNOWN: runtime value not verified; tests pass with artificial states per SESSION_379: "gera template draft (não aprendizado real)")
- **Evidence**: 
  - PollEvery(500) collects EventBus pairs, fine-tunes ternary placeholder weights, persists to SGDB (lines 89-214)
  - `learn_tick()` returns 0.0 when no pairs — the system never generates real input/output pairs at runtime outside test harness
  - `learner_self_test()` (line 435) constructs artificial states: `agent.remember("hello", "world")` then checks recall — these are test-only states, not runtime-generated
  - SESSION_379 confirms: learning generates "template draft (não aprendizado real)" — no real weights/WASM produced
  - The `persist()` method (line 184) writes to SGDB but with placeholder 64-byte weights — no real model training occurs
- **Minimal case**: Remove the `PollEvery(500)` schedule. Make learning purely event-driven: trigger via `SKILL_CREATE` or `/learn` command in Hermes, not continuous polling. Keep the `DataCollector`, `TrainingAgent`, and SGDB persistence as library functions callable on-demand.
- **Breaks**: Would need to ensure `DataCollector` still captures pairs when learning is triggered via commands. The `remember()` API could remain for HITL use.

### 4. MetricsAgent polling — PollEvery(9 ≈ 0.5s) for HUD snapshot
- **File**: `crates/jarbas/src/display/metrics_agent.rs`
- **Line**: Manifest at :14-20 (PollEvery(9)); tick at :49-59; `refresh_snapshot(log)` at :57
- **Classification**: UNKNOWN (could be OBSERVED as useful HUD data, but polling pattern may be wasteful)
- **Evidence**: 
  - Docstring: "Compositor só lê o snapshot (sem amostrar no hot path de frame)" — suggests the agent's purpose is to populate a static snapshot the compositor reads, not to sample in hot path
  - Poll interval 9 ticks ≈ 0.5s at 18Hz PIT; the `last_timer` gate (line 51-53) prevents faster than 0.5s updates
  - `samples % LOG_EVERY_N == 0` (line 56) only logs every 20 samples (~10s), but the agent always returns `Pending`
  - The HUD may benefit from periodic metrics, but the 0.5s interval may be arbitrary — no evidence it aligns with actual UI update needs
- **Minimal case**: Make the agent event-driven: trigger on `HUD_UPDATE` or similar event, or increase poll interval to `PollEvery(50)` (~2.8s) if periodic is preferred. Alternatively, remove the agent and have the compositor sample directly from `agent_core::agent_budget_stats()` or `k_nano::memory::global_hardware_context()`.
- **Breaks**: HUD may lose some real-time granularity, but per docstring the compositor already reads snapshots without hot-path sampling, so impact is minimal.

### 5. SelfHealAgent — silent failure detector never triggers — ✅ IMPLEMENTED (s449)
- **File**: `crates/k_ai/src/self_heal_agent.rs`
- **Line**: Manifest at :377-385 (PollEvery(1000)); tick at :456-457 `self.silent.heartbeat("self_heal")`; watched_count check at :461-474
- **Classification**: DEAD_WEIGHT (OBSERVED: silent failure detector never triggers because no fleet heartbeats wired — per line 460 comment: "Sem fleet heartbeats wired, publicar I5 = spam falso (só self_heal no mapa)")
- **Evidence**: 
  - Line 460: "Sem fleet heartbeats wired, publicar I5 = spam falso (só self_heal no mapa)"
  - Line 461: `if self.silent.watched_count() > 1` — but watched_count only counts agents besides self_heal, and per comment there are no fleet heartbeats
  - The I5:silent health issue (line 466-473) would only publish if multiple agents have silent failures, but no other agents publish heartbeats
  - Checkpoint every 50k ticks (~45 min at 18Hz) may be artificial — system may not run that long between reboots
- **Minimal case**: Remove the `watched_count() > 1` guard or remove the silent heartbeat entirely. Keep the core error processing (kernel error drain, checkpoint, healing response handling) but simplify the heartbeat/health issue publishing.
- **Breaks**: Minimal — core self-heal functionality (error drain, checkpoint, AI diagnosis) would remain. Only the fake I5 health issue publishing would be removed.

### 6. Redundant USB keyboard polling in InputAgent — ✅ IMPLEMENTED (s449)
- **File**: `crates/hermes/src/agents.rs`
- **Line**: tick at :181-203; `poll_usb_keyboard()` at :207-209; tick spacing at :186-193 (tick == 90 || tick == 180 || tick == 360)
- **Classification**: DEAD_WEIGHT (OBSERVED: s361 explicitly notes "não chamar a cada tick"; 90/180/360 spacing added to reduce redundancy)
- **Evidence**: 
  - InputAgent receives IRQ-driven events from `RAW_HW_IRQ1` subscription (line 168: `receiver: EVENT_BUS.subscribe("RAW_HW_IRQ1")`)
  - USB keyboard is polled every tick via `poll_usb_keyboard()` (line 199), but with spacing at ticks 90/180/360 to reduce load (line 186-193)
  - Per s361: "não chamar a cada tick" — the tick spacing was specifically added to reduce redundant polling
  - The USB polling still runs on every tick when `tick == 90 || tick == 180 || tick == 360`, but the bringup (`try_deferred_hid_bringup()`) is the actual work — the `poll_usb_keyboard()` call may be redundant when IRQ-driven path already delivers scancodes
- **Minimal case**: Remove the `poll_usb_keyboard()` call from tick entirely. Keep only the IRQ-driven path and the periodic bringup at ticks 90/180/360. If USB polling was needed for reliability, add it back as a conditional every-N-ticks with proper spacing.
- **Breaks**: May lose some USB keyboard support in configurations where IRQ1 is not routed, but the IRQ-driven path (via `RAW_HW_IRQ1`) should cover the primary use case.

### 7. Redundant digest/hash computations in stamp system — ❌ INVERTED (E3 wired the consumer; do NOT remove)
- **File**: `crates/event-bus/src/stamp.rs`
- **Line**: FNV-1a64 at :106, chain_hash at :254-257, verify_stamps at :291-312
- **Classification**: UNKNOWN (could be OBSERVED as infrastructure, but may be dead weight if never used at runtime)
- **Evidence**: 
  - The stamp system computes FNV-1a64 chain hashes for published events (line 254-257: `payload_digest = hasher(&dbuf[..dp])` then `chain_hash = chain_of(...)`)
  - `verify_stamps()` (line 291) can verify stamp rings, but there's no evidence it's called at runtime in production
  - The stamp ring has `STAMP_RING_CAP` (line 375-376) and is used in tests (SESSION_375: "bounded queues + drop_oldest; recv_count só em try_receive")
  - No runtime consumer of `stamp::verify_stamps` or `stamp::payload_digest` found in grep search
- **Minimal case**: Remove the hash computation and chain tracking if no runtime consumer exists. Keep the stamp struct and ring if they serve other purposes (e.g., ordering, debugging). If removal is too risky, make the hasher optional via `cfg(feature = "stamps")`.
- **Breaks**: Would break any code that relies on stamp chain hashes for ordering/tamper detection. No such code found at runtime, but cannot guarantee zero impact without broader search.

### 8. Artificial test states across test harnesses
- **File**: Multiple `*_test` modules and `demo()` functions across crates
- **Line**: e.g., `crates/k_ai/src/self_learning.rs:409-432` (demo), `:435-487` (learner_self_test); `crates/cortex/src/bitnet_avx2.rs` test patterns
- **Classification**: UNKNOWN (tests may be VALID for unit testing but not reflect runtime per mission §15: "working tree→imagem bootada")
- **Evidence**: 
  - `self_learning::demo()` (line 410) creates agent with no events published, checks loss=0.0, weights intact — artificial state with no runtime correspondence
  - `self_learning::learner_self_test()` (line 435) adds `remember()` calls to construct test state — not representative of real input/output pair generation
  - Many crate test modules (`#[cfg(test)] mod tests {}`) test individual functions in isolation, not integrated boot runtime
  - Mission §15: "working tree→imagem bootada" — unit tests that don't boot the OS are supplementary, not definitive
- **Minimal case**: Annotate test functions as `#[cfg(test)]` only and ensure any state they construct has a path to runtime use. Remove or refactor tests that create artificial states with no runtime correspondence. Keep tests that validate actual boot paths (e.g., `cargo check --release` + QEMU boot).
- **Breaks**: Removing unit tests reduces developer confidence but doesn't affect runtime binaries. Mission prioritizes bootable images over test coverage.

### 9. SafetyAgent/SecurityAgent status messages without actionable value
- **File**: `crates/hermes/src/safety.rs`, `crates/hermes/src/security.rs`
- **Classification**: UNKNOWN (may be OBSERVED as safety infrastructure, but many slog messages don't lead to action)
- **Evidence**: 
  - Various `slog_hermes!` and `slog_kai!` calls log status, warnings, failures — some may be useful for debugging, others may be noise
  - Per SESSION_375: "bounded queues + drop_oldest; recv_count só em try_receive" — some counters may not reflect real event processing
  - The safety invariants (I1-I4) in `k_ai::safety_invariants.rs` check heap, agents, trust, scheduler — but pre-fleet (agent_count=0) these are warnings, not real invariants
- **Minimal case**: Audit each slog message: keep those that lead to HITL decisions, system reconfiguration, or observable behavior; remove or simplify those that only log to serial/FB without downstream consumption. Group non-actionable messages under a single `debug` severity level.
- **Breaks**: Would reduce logging output but not affect core functionality. HITL decisions depend on specific messages — must preserve those.

---

## SUMMARY RANKING (most to least impactful for removal/simplification)

| Rank | Finding | File:Line | Classification | Removal Impact |
|------|---------|-----------|----------------|----------------|
| 1 | MatrixLearningAgent redundant polling | hermes/src/agents/log_analyst_agent.rs:415-421, matrix_learn.rs:448-477 | DEAD_WEIGHT (OBSERVED) | HIGH — removes entire agent, zero functional loss |
| 2 | BootLogAgent redundant polling | k_ai/src/boot_log_agent.rs:10, :186 | DEAD_WEIGHT (OBSERVED) | HIGH — change to Oneshot or init-time only |
| 3 | SelfLearningAgent heavyweight learning | k_ai/src/self_learning.rs:23-29, :340-343 | DEAD_WEIGHT (UNKNOWN) | MEDIUM — make event-driven, keep APIs |
| 4 | MetricsAgent polling | jarbas/src/display/metrics_agent.rs:14-20 | UNKNOWN | MEDIUM — make event-driven or increase interval |
| 5 | SelfHealAgent silent detector | k_ai/src/self_heal_agent.rs:456-474 | DEAD_WEIGHT (OBSERVED) | MEDIUM — remove fake I5 publishing |
| 6 | Redundant USB keyboard polling | hermes/src/agents.rs:181-203, :186-193 | DEAD_WEIGHT (OBSERVED) | MEDIUM — rely on IRQ-driven only |
| 7 | Stamp hash computations | event-bus/src/stamp.rs:106, :254-257 | UNKNOWN | LOW — optional feature, may have no runtime users |
| 8 | Artificial test states | various *\_test modules | UNKNOWN | LOW — annotate as test-only, not runtime |
| 9 | Safety status messages | hermes/src/safety.rs, security.rs | UNKNOWN | LOW — audit and simplify slog messages |

---

## OBSERVED vs VERIFIED (NOT dead weight — preserve these)

### OBSERVED (behavior confirmed in code/boot, keep):

1. **BootLogAgent FAT walk** — the one-time FAT analysis at boot is real code that runs and produces diagnostics. Not dead weight, but the *polling* after first analysis is. Keep the analysis function, remove PollEvery.

2. **SelfHealAgent core error processing** — kernel error drain, checkpoint, AI diagnosis are real functionality. Dead weight is only the silent heartbeat/I5 publishing that never triggers. Keep the core, simplify the heartbeat.

3. **MetricsAgent snapshot purpose** — per docstring, compositor reads snapshots without hot-path sampling. The agent may have real value if HUD needs periodic metrics. Keep but consider making event-driven.

4. **Safety invariants I1-I4** — the check code is real; the *results* may be misleading pre-fleet (I3 warning is "fantasma" per safety_invariants.rs:150), but the check infrastructure should remain with simplified messages.

5. **InputAgent IRQ-driven scancode path** — the `RAW_HW_IRQ1` subscription and scancode processing is real and functional. Dead weight is only the USB keyboard polling every tick.

### VERIFIED (tests + runtime confirm behavior, keep):

1. **TicketLock** — `cargo check --release` passes 0 errors; lock works as documented in `ticket-lock/src/lib.rs`.

2. **MonitorAgent ONESHOT** — publishes SYSTEM_READY once, done. Verified working.

3. **Hermes greeting** — `hermes_greeting()` runs at boot, produces output. Verified.

4. **Mesh p2p tick** — `mesh_tick()` in k_nano runs per boot_whpx observations. Verified.

5. **QEMU UEFI boot** — 8 phases observed in serial/boot.txt. Verified.

---

## PROPOSED REMOVALS/SIMPLIFICATIONS (specific)

1. **Remove MatrixLearningAgent**: Delete `crates/hermes/src/agents/log_analyst_agent.rs` entirely. Update any references in `hermes.rs` or `globals.rs` that wire the agent into the fleet. No replacement needed — Hermes already handles learning requests.

2. **BootLogAgent: Change to Oneshot**: Modify `crates/k_ai/src/boot_log_agent.rs` manifest `schedule: ScheduleKind::Oneshot(0)` (or remove PollEvery and invoke `read_last_boot_log()` once during `init_platform_sync`). Remove the `self.analyzed` flag and the periodic FAT walk — run once at boot.

3. **SelfLearningEvent-Driven**: Remove `PollEvery(500)` from `crates/k_ai/src/self_learning.rs` manifest. Make learning triggerable via:
   - `HermesAgent` command `/learn <text>` → calls `k_ai::self_learning::learn_tick()` directly
   - `SKILL_CREATE` event → same
   - Keep `DataCollector`, `TrainingAgent`, `persist()` as on-demand APIs.

4. **MetricsAgent: Event-Driven or Longer Interval**: 
   - Option A: Change to `ScheduleKind::Oneshot` and trigger on `HUD_UPDATE` event
   - Option B: Change to `PollEvery(50)` (~2.8s) if periodic is preferred
   - Option C: Remove agent entirely; compositor samples from `agent_core::agent_budget_stats()` directly

5. **SelfHealAgent: Remove silent heartbeat**: In `crates/k_ai/src/self_heal_agent.rs`, remove lines 456-457 (`self.silent.heartbeat("self_heal")`) and the `watched_count() > 1` guard at lines 461-474. Keep core error processing (lines 387-454).

6. **InputAgent: Remove USB keyboard poll from tick**: In `crates/hermes/src/agents.rs`, remove the `if let Some(scancode) = unsafe { self.poll_usb_keyboard() }` block at lines 198-201. Keep the IRQ-driven path and the tick-spacing bringup at 90/180/360.

7. **Stamp system: Make hasher optional**: In `crates/event-bus/src/stamp.rs`, wrap the FNV-1a64 hash computation and chain_hash tracking with `#[cfg(feature = "stamps")]` or remove if no runtime consumer found after broader search.

8. **Test annotations**: Ensure all `#[cfg(test)]` modules and `demo()` functions are annotated as test-only with no runtime correspondence claim. Document which tests boot the OS vs which are unit-tests-only.

9. **Safety message audit**: In `crates/hermes/src/safety.rs` and `crates/hermes/src/security.rs`, review each `slog_*` call: keep those referencing actionable items (budget, trust, agent count thresholds), simplify or demote to `trace` those that only log status without downstream consumption.

---
*End of audit (pre-fix snapshot). Fixes #1,#2,#3,#5,#6 landed in s449 (commit 166305be); #7 inverted by E3.*