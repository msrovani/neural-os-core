# ADR-0024: Agent Frameworks Ecosystem Analysis (Tier 4)

**Status:** Active
**Date:** 2026-06-25
**Deciders:** Architecture Team
**Session:** SESSION_019

## Context

Following the analysis of Tier 0 (Crom, 75 repos), Tier 1 (Life OS, 20 repos), Tier 2 (Personal AI Assistants, 21 repos), and Tier 3 (Memory Systems, 14 repos), this ADR analyzes Tier 4: **Agent Frameworks** (6 repos). These frameworks define universal agent patterns — task planning, tool routing, permission systems, lifecycle hooks, session management, skill registries — that are directly portable to neural-os-core's bare-metal `no_std` architecture.

## Repos Analyzed

| Repo | Stars | Lang | Releases | Commits | Maturity |
|------|-------|------|----------|---------|----------|
| **Cline** (cline/cline) | 63,900 | TypeScript | 293 | 6,338 | HIGH — production monorepo, SDK/CLI/VS Code/Hub |
| **OpenHands** (All-Hands-AI/OpenHands) | 77,000 | Python/TS | 102 | 6,700+ | HIGH — $18.8M funded, event-sourced architecture |
| **Agent Zero** (agent0ai/agent-zero) | 18,200 | Python | 61 | 2,365 | HIGH — Docker-based, Plugin Hub, Skills framework |
| **Microsoft Agent Framework** (microsoft/agent-framework) | 11,700 | Python/C# | 1.0 GA | ~1,500 | HIGH — AutoGen + Semantic Kernel unification |
| **opencode** → Crush (anomalyco/opencode → charmbracelet/crush) | 13,100 | Go | ~30 | ~1,000 | ARCHIVED — moved to Crush |
| **open-agent** (Colinchen-333/open-agent) | 0 | TS | 0 | minimal | VERY NEW — limited analysis value |

## Cline Deep-Dive (Primary Analysis Target, 63.9k ★)

### Architecture Overview

Cline is a TypeScript monorepo with 4 SDK packages and 4 app surfaces:

```
sdk/packages/
├── core/       — Stateful runtime: session lifecycle, hub services, checkpoint/restore, ClineCore
├── agents/     — Stateless agent loop: AgentRuntime, hooks, tools, plugins, streaming
├── llms/       — Model catalog + provider settings + handler creation SDK
├── shared/     — Cross-package primitives: types, paths, session helpers, telemetry
└── sdk/        — Public API surface (re-exports)

apps/
├── cli/        — Terminal app
├── vscode/     — VS Code extension
├── cline-hub/  — Kanban multi-agent board (hub daemon + server + client)
└── examples/   — Usage examples
```

### AgentRuntime (agents package) — Core Loop

**Source:** `sdk/packages/agents/src/agent-runtime.ts` (~850 lines)

Key design:
- **Discriminated config union**: `AgentRuntimeConfigWithModel` (pre-built model, advanced) vs `AgentRuntimeConfigWithProvider` (provider ID + credentials, user-friendly)
- **Turn-based iteration**: `config.maxIterations` guard, inner loop: generate → parse tool calls → execute → check completion
- **Hooks lifecycle**: 7 hook points — `beforeRun`, `afterRun`, `beforeModel`, `afterModel`, `beforeTool`, `afterTool`, `onEvent` — composable via plugins
- **Tool policies**: Per-tool `{ enabled, autoApprove }` with wildcard `"*"` fallback, `requestToolApproval()` callback for human-in-the-loop
- **Streaming**: `for await...of` on model stream emits `text-delta`, `reasoning-delta`, `tool-call-delta`, `usage`, `finish` event types
- **Tool assembly**: Accumulates streaming JSON tool arguments, parses after stream ends, reports `invalidToolCalls` with parse errors
- **Completion terminal tools**: `lifecycle.completesRun` marks terminal tools (like SUBMIT_AND_EXIT); `completionPolicy.completionGuard` adds reminder messages
- **Usage tracking**: `usageDelta()` between before/after model calls, cumulative `AgentUsage` with `inputTokens`, `outputTokens`, `cacheReadTokens`, `cacheWriteTokens`, `totalCost`

### CronRunner (core/cron) — Scheduled Agents

**Source:** `sdk/packages/core/src/cron/runner/cron-runner.ts` (~380 lines)

- **SQLite-backed**: `SqliteCronStore` with `specs`, `runs`, `event_logs` tables
- **Claim-based polling**: `CronRunner.tick()` polls every 15s, atomically claims due runs via `claimDueRuns()`
- **Lease heartbeat**: `startClaimLeaseHeartbeat()` renews claim every ~45s to prevent double-execution
- **Resource limiter**: `ResourceLimiter` with per-spec `maxParallel` concurrency
- **Timeout handling**: `spec.timeoutSeconds` → `withTimeout()` → `TimeoutError` → abort session → mark failed
- **Report generation**: `writeCronRunReport()` writes markdown reports per run
- **Mode support**: `act`, `plan`, `yolo` modes with different tool policies
- **Extension system**: Rules, Skills, Plugins per spec
- **Hub schedule publishing**: `publishScheduleExecutionEvent()` for external event bus integration

### ClineCore (core) — Session Orchestration

**Source:** `sdk/packages/core/src/ClineCore.ts` (~350 lines)

- **Factory pattern**: `ClineCore.create(options)` → sets up RuntimeHost, AutomationService, FeatureFlags
- **Session lifecycle**: `start()` → `send()` → `stop()` / `abort()` → `dispose()`
- **Runtime modes**: `local`, `hub` (shared daemon), `remote` (hub discovery)
- **Checkpoint/restore**: `compareCheckpoint()` diffs workspace state, `restore()` rollback
- **Bootstraps**: `StartSessionBootstrap` with `applyToStartSessionInput()` + `dispose()` for pre-session setup
- **Event subscription**: Typed `CoreSessionEvent` stream, unsubscribe pattern

### Key Cline Patterns Portable to neural-os-core

1. **Hook-based lifecycle** → Hermes daemon EventBus hooks
2. **Tool policies** → SkillRegistry `CapabilityToken` validation + TrustCache
3. **Completion terminal tools** → `lifecycle.completesRun` marks skills as terminal
4. **Turn-based iteration** → NeuralExecutor cooperative polling loop
5. **Usage tracking** → `hardware_context_tensor()` metrics
6. **Claim-based cron** → `ScheduleService` for periodic tasks
7. **Session checkpoint/restore** → MHI snapshot for state persistence
8. **Hub discovery** → EventBus multi-instance coordination

---

## Other Repos Summary

### OpenHands (77k ★) — Event-Sourced Architecture

- **V1 SDK**: 4 packages — SDK, Tools, Workspace, Server
- **Event source**: `ConversationState` as single mutable object, `Action`/`Observation` typed Pydantic events
- **Key insight**: Agent as pure function `f(history) -> next event` — pause, resume, fork, deterministic replay
- **Sandbox**: Docker container per task, MicroK8s sandbox
- **Portable pattern**: Event-sourced agent state → Hermes conversation buffer as immutable event log

### Agent Zero (18.2k ★) — Skills Framework

- **Plugin Hub**: 100+ community plugins, one-click install with AI-driven security scanning
- **Skills framework**: `Skills` teach agent to operate tools, desktop, browser — wraps custom capabilities
- **Memory**: Agentic RAG with auto-consolidation, timestamp-based staleness
- **Time Travel**: Workspace snapshots with `.gitignore` resilience
- **Portable pattern**: Plugin Hub index pattern → MCP tool registration; Time Travel snapshots → MHI checkpoint

### Microsoft Agent Framework (11.7k ★) — Graph Orchestration

- **Graph-based workflows**: `sequential`, `concurrent`, `handoff` between agents
- **Agent Harness**: Automatic context compaction, `FileMemoryProvider`, `TodoProvider`, `AgentModeProvider` (plan/execute)
- **OpenTelemetry**: Built-in observability, tracing, logging
- **IL (Instructional Language)**: Extension pattern for custom agent behaviors
- **Portable pattern**: Graph orchestration → Hermes multi-daemon coordination via EventBus; Context compaction → `hardware_context_tensor()` memory budget

### opencode → Crush (13.1k ★) — Auto-Compact

- **Auto-compact**: Automatically summarize conversation when approaching context limit
- **Archived**: Moved to `charmbracelet/crush`
- **Portable pattern**: Context budget management → Hermes daemon auto-summary of conversation buffer (3+ cycles → trigger `summarize_context` skill)

---

## Classified Ideas (#228-#249)

### Imediatas / Simples — Sprint 23 (immediately actionable, < 150 LOC each)

| # | Idea | Source | LOC | File Target |
|---|------|--------|-----|-------------|
| 228 | **Tool Policy Registry** — Extend `SkillRegistry` with `{ enabled: bool, autoApprove: bool }` per tool, wildcard fallback, `validate_tool_call()` denies blocked tools | Cline | ~80 | `crates/skill-registry/src/policy.rs` |
| 229 | **Usage Tracker** — Lightweight token/metrics accumulator for `hardware_context_tensor()`: track `input_chars`, `output_chars`, `cache_hits`, `iterations` per daemon | Cline | ~50 | `crates/neural-kernel/src/metrics.rs` |
| 230 | **Auto-Compact Hermes Buffer** — After 3+ conversation cycles without user input, trigger `summarize_context` skill to compact buffer into single `[System Note]` | opencode | ~60 | `crates/event-bus/src/compaction.rs` |
| 231 | **Event-Sourced Conversation State** — Replace mutable `String` buffer in Hermes with `VecDeque<ConversationEvent { type, payload, timestamp }>` immutable event log | OpenHands | ~100 | `crates/event-bus/src/conversation.rs` |

### Complexidade Baixa — Sprint 24

| # | Idea | Source | LOC | Sprint |
|---|------|--------|-----|--------|
| 232 | **Cron Scheduler** — `ScheduleService` with `CronSpec { prompt, schedule, enabled, model_id, tool_policies }`, periodic poll via LAPIC timer, markdown report writer | Cline | ~350 | 24 |
| 233 | **Session Checkpoint/MHI Snapshot** — `checkpoint()` saves kernel state + MHI tier stats to reserved frames; `restore()` rollback on Double Fault | Cline | ~200 | 24 |
| 234 | **Plan/Execute Modes** — Hermes dual-mode: `plan` mode (analysis only, no tool execution), `act` mode (full execution with auto-approve) | Cline + MS Agent | ~150 | 24 |
| 235 | **Graph-Based Multi-Daemon Orchestration** — Extend EventBus with `sequential` / `concurrent` / `handoff` choreography patterns for daemon chains | MS Agent | ~250 | 24 |

### Complexidade Média — Sprint 25

| # | Idea | Source | LOC | Sprint |
|---|------|--------|-----|--------|
| 236 | **Plugin Hub / MCP Index** — `McpRegistry` can discover and install skill packages from a remote index, with AI-driven security scanning | Agent Zero | ~400 | 25 |
| 237 | **Completion Terminal Skills** — `lifecycle.completes_run: bool` field on `McpManifest`, `SkillRegistry` auto-routes terminal response to `HERMES_RESPONSE` | Cline | ~120 | 25 |
| 238 | **Claim-Based Daemon Lease** — `LeaseDaemon` skill: acquire lease with TTL, heartbeat via LAPIC timer, release on completion; prevents double-execution | Cline | ~200 | 25 |
| 239 | **Time Travel / Workspace Snapshot** — Capture BitmapFrameAllocator state + MHI tiers at checkpoint, restore on Double Fault or `/restore` command | Agent Zero | ~300 | 25 |
| 240 | **Context Compaction with Ebbinghaus Decay** — Conversation events decay via Ebbinghaus formula; `summarize_context` runs automatically when budget < 20% remaining | Cline + Tier 3 | ~150 | 25 |

### Complexidade Alta — Sprint 26+

| # | Idea | Source | LOC | Sprint |
|---|------|--------|-----|--------|
| 241 | **OpenTelemetry-Like Observability** — Trace event logging via serial output, structured log format, per-daemon latency/usage metrics | MS Agent | ~500 | 26+ |
| 242 | **AI-Driven Security Scan for Skills** — Use Intent MLP to classify skill behavior as safe/suspicious/malicious before execution | Agent Zero | ~350 | 26+ |
| 243 | **Hub Discovery / Multi-Instance Board** — EventBus instances discover each other via shared MHI memory region, coordinate task assignment | Cline | ~400 | 26+ |
| 244 | **Human-in-the-Loop Approval** — `request_tool_approval()` blocks tool execution until keyboard confirmation via `/approve` or `/deny` | MS Agent + Cline | ~250 | 26+ |

### Ideias Futuras (pós-MVP, roadmap beyond Sprint 26)

| # | Idea | Source |
|---|------|--------|
| 245 | **Remote Agent Execution** — Hub daemon on separate machine, EventBus over VirtIO-net TCP | Cline |
| 246 | **Skill Marketplace** — Signed, versioned MCP packages published to remote registry; Hermes `/install <pkg>` | Agent Zero |
| 247 | **Automatic Context Compaction Agent** — Dedicated daemon that monitors conversation budget and proactively compacts | Cline |

### Descartadas

| # | Idea | Reason |
|---|------|--------|
| 248 | **Docker Sandbox** (Agent Zero) | Incompatible with bare-metal `no_std`; no container runtime at Ring 0-2 |
| 249 | **Python/.NET Runtime** (MS Agent, OpenHands) | Language barrier; Python requires OS where neural-os-core replaces the OS |

## Total Count

- **Imediatas (Sprint 23):** 4 ideas (#228-231)
- **Baixa (Sprint 24):** 4 ideas (#232-235)
- **Média (Sprint 25):** 5 ideas (#236-240)
- **Alta (Sprint 26+):** 4 ideas (#241-244)
- **Futuras:** 3 ideas (#245-247)
- **Descartadas:** 2 ideas (#248-249)
- **Total Tier 4:** 22 ideas

## Running Total (All Tiers 0-4)

| Tier | Repos | Ideas | ADR |
|------|-------|-------|-----|
| 0 (Crom) | 75 | 22 | ADR-0020 |
| 1 (Life OS) | 20 | 26 | ADR-0021 |
| 2 (Personal AI Assistants) | 21 | 15 | ADR-0022 |
| 3 (Memory Systems) | 14 | 14 | ADR-0023 |
| 4 (Agent Frameworks) | 6 | 22 | ADR-0024 |
| **Total** | **136** | **99** | |

## Conclusions

1. **Cline is the richest source of portable patterns** (11 ideas ranked immediate to medium) — its hook lifecycle, tool policies, cron scheduler, and claim-based lease directly map to Hermes daemon + EventBus + TrustCache + LAPIC timer

2. **Agent Frameworks (Tier 4) are more portable than Memory Systems (Tier 3)** because they define universal agent patterns that are language-agnostic and architectural — hook points, permissions, lifecycle, scheduling — whereas memory systems tend to be library- and OS-dependent

3. **Hermes daemon already mirrors Cline architecture** (skill system = tools, EventBus = hooks, TrustCache = tool policies, Intent MLP = model) — this confirms the design direction is correct

4. **Top 3 immediate actions (Sprint 23):** Tool Policy Registry (#228), Usage Tracker (#229), Event-Sourced Conversation State (#231) — collectively ~230 LOC, zero new dependencies
