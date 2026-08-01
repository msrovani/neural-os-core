# crates/skill-registry/

## Responsibility

No_std Skill abstraction + registry: the "everything is an Agent or a Skill" principle's Skill side. Provides `Skill` trait, `SkillRegistry` singleton with per-skill `ToolPolicy` (enabled / auto-approve), capability-token authorization, output contracts, plus index/cache/task/fan-out helpers. 10 source files: `skill.rs`, `mcp.rs`, `registry.rs`, `contract.rs`, `cache.rs`, `index.rs`, `task.rs`, `dynskill.rs`, `fanout.rs`, `lib.rs`.

## Design

- **`Skill` trait** (`skill.rs`): `Send + Sync`; `manifest() -> McpManifest`, `execute(payload) -> Result<Vec<u8>, &'static str>`, optional `verify()` pre-flight precondition check.
- **`McpManifest`** (`mcp.rs`): name, description, `required_tokens: Vec<u64>`, VFS `preconditions` paths, `context_links` (composition), `output_schema`, `idempotent`, `contracts`. `OutputSchema` (Any/String/Json(keys)) validates outputs.
- **`SkillRegistry`** (`registry.rs`): `BTreeMap<name, Box<dyn Skill>>` + policy map; `register`/`has_skill`/`set_policy`/`get_policy` (falls back to `"*"` wildcard)/`is_enabled`/`is_auto_approve`/`validate_token`. `execute_skill(name, payload, token)` enforces policy + token (unless auto-approve) + verify + output contracts; `execute_skill_unchecked` skips the token check. **`list_skills() -> Vec<(String, ToolPolicy)>`** returns `"name: description"` strings with their policies.
- **`contract.rs`**: `CompletionContract { validate: ValidationFn, on_failure }` with `ContractAction` WarnOnly / RejectOutput / RetrySkill; built-ins `CONTRACT_NONEMPTY` (WarnOnly) and `CONTRACT_UTF8` (RejectOutput).
- **`cache.rs`**: `OutputCache` — djb2 hash of (name+payload) → output with tick-based TTL, hit/miss stats, `evict_expired`; for idempotent skills.
- **`index.rs`**: `SkillIndex` (by_domain / by_capability / `relevant` intersection / `find` text search) for progressive disclosure, plus `McpCatalog` (public searchable `CatalogEntry` list).
- **`task.rs`**: `TaskSchema` + `JobPreconditions` (memory, resources, skills, timeout_ticks, max_retries) + `TaskStatus` (Pending/Running/Completed/Failed/TimedOut).
- **`dynskill.rs`**: `DynamicSkill` — LLM-generated skills from instructions, with optional `wasm: Option<Vec<u8>>` bytecode for hot-promote (ADR-0059 F5).
- **`fanout.rs`**: `FanOutPool` — spawn `FnOnce` subtask closures, `poll_all`, collect results.

## Flow

Kernel registers built-in skills at boot; the LLM generates `DynamicSkill`s at runtime which a SkillObserver registers; Cortex/Hermes execute via `execute_skill` with a `CapabilityToken`; idempotent outputs hit `OutputCache`; contracts post-validate outputs; `SkillIndex` selects context-relevant skills for Hermes.

## Integration

Depends on `event-bus` (for `CapabilityToken`). The singleton lives in `k_nano` (`SKILL_REGISTRY`) — per SESSION_217, the bin must not shadow it with a private copy or skills silently vanish from the crates. `hermes::app_factory` registers runtime WASM apps as skills.
