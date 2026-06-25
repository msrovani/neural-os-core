# Session 025 — Tier 4 Agent Frameworks Analysis (v0.17.0)

**Date:** 2026-06-25
**Goal:** Analyze 6 Tier 4 Agent Frameworks repos for ideas portable to neural-os-core; deep-dive Cline (63.9k ★); create ADR-0024 with 22 classified ideas (#228-249)
**Version:** v0.17.0 (no code changes — documentation-only session)

## Summary

Completed the Tier 4 (Agent Frameworks) ecosystem analysis — the deepest tier so far for architectural pattern portability. Agent frameworks define universal patterns (hook lifecycle, tool policies, scheduling, session management) that map directly to Hermes daemon + EventBus + TrustCache.

## Repos Analyzed

| Repo | Stars | Lang | Analysis Depth |
|------|-------|------|----------------|
| **Cline** (cline/cline) | 63,900 | TypeScript | **Deep-dive** — read AgentRuntime (~850 LOC), CronRunner (~380 LOC), ClineCore (~350 LOC) |
| **OpenHands** (All-Hands-AI/OpenHands) | 77,000 | Python/TS | Medium — event-sourced architecture |
| **Agent Zero** (agent0ai/agent-zero) | 18,200 | Python | Medium — Plugin Hub, Skills framework |
| **Microsoft Agent Framework** (microsoft/agent-framework) | 11,700 | Python/C# | Medium — graph orchestration, Agent Harness |
| **opencode** (anomalyco/opencode → Crush) | 13,100 | Go | Light — auto-compact pattern |
| **open-agent** (Colinchen-333) | 0 | TS | Minimal — too new |

## Cline Deep-Dive Key Findings

1. **AgentRuntime** (agents package): ~850 lines of TypeScript implementing the core agent loop with:
   - 7 hook points (beforeRun, afterRun, beforeModel, afterModel, beforeTool, afterTool, onEvent)
   - Discriminated config union (pre-built model vs provider+modelId)
   - Streaming tool call assembly with JSON parse error recovery
   - Tool policies with wildcard `"*"` fallback + per-tool `{ enabled, autoApprove }`
   - Completion terminal tools (`lifecycle.completesRun`)
   - Turn-based iteration with `maxIterations` guard

2. **CronRunner** (core/cron): ~380 lines of scheduled agent execution with:
   - SQLite-backed store (specs, runs, event_logs)
   - Claim-based polling with lease heartbeat (prevents double-execution)
   - Resource limiter (per-spec maxParallel concurrency)
   - Timeout handling (spec.timeoutSeconds → withTimeout → abort)
   - Markdown report generation per run

3. **ClineCore** (core): ~350 lines of session orchestration:
   - Factory pattern (ClineCore.create)
   - Session lifecycle (start → send → stop/abort → dispose)
   - Runtime modes (local, hub, remote)
   - Checkpoint/restore with workspace diff
   - Event subscription with typed CoreSessionEvent stream

## Documents Created/Modified

| File | Action |
|------|--------|
| `docs/architecture/0024-agent-frameworks-analysis.md` | **Created** — ADR-0024, full Tier 4 synthesis |
| `docs/memory/IDEA_BANK.md` | **Modified** — added 22 items (#228-249), updated total to 249 |
| `docs/memory/STATE.md` | **Modified** — added ecosystem analysis summary |
| `docs/memory/SESSION_025.md` | **Created** — this file |

## Key Decisions

1. **Cline is the richest pattern source of all 136 repos analyzed across Tiers 0-4** — its hook lifecycle, tool policies, cron scheduler, and claim-based leases map directly to Hermes daemon + EventBus + TrustCache + LAPIC timer

2. **Agent Frameworks (Tier 4) are more portable than Memory Systems (Tier 3)** because they define universal agent patterns that are language-agnostic — hook points, permissions, lifecycle, scheduling — whereas memory systems tend to be library- and OS-dependent

3. **Top 3 immediate actions (Sprint 23):** Tool Policy Registry (#228), Usage Tracker (#229), Event-Sourced Conversation State (#231) — collectively ~230 LOC, zero new dependencies

4. **Hermes daemon already mirrors Cline architecture** (skill system = tools, EventBus = hooks, TrustCache = tool policies, Intent MLP = model) — confirms design direction is correct

## Next Steps

1. Begin implementing Sprint 23 items: Tool Policy Registry, Usage Tracker, Event-Sourced Conversation
2. Consider Tier 5 analysis (Language-Specific Runtimes — WASM, Rust native agents, C ABI tools)
3. Return to kernel development: network stack (VirtIO-net + smoltcp)
