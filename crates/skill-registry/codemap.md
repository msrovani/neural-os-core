# crates/skill-registry/

## Responsibility

No_std Skill abstraction + registry (Skill side of "everything is an Agent or a Skill").
`Skill` trait, `SkillRegistry` with `ToolPolicy`, capability-token auth, output contracts,
`OutputCache` (idempotent only), `DynamicSkill` (text / WASM promote stub).

Singleton canônico: `k_nano::SKILL_REGISTRY` (SESSION_217 — sem shadow no bin).

## Honesty (SESSION_377)

- `DynamicSkill::with_wasm`: bytecode stored; `execute` → `Err("wasm_runtime_unwired")` until wasmi bridge.
- `list_skills` → `SkillListEntry { name, description, policy }` (não blob `"name: desc"`).
- `is_enabled` default **false** sem policy; boot seta `"*"`.
- `required_tokens: []` ⇒ deny sob gate.
- `OutputCache`: caller só deve cachear se `is_idempotent`; hits/misses + cap 64.
- Removidos módulos mortos: `index`, `task`, `fanout` (0 callers).

## Design

- **`Skill`**: `manifest` / `execute` / optional `verify`.
- **`McpManifest`**: tokens, preconditions, context_links, output_schema, idempotent, contracts.
- **`OutputSchema::Json`**: substring contains — **não** parse JSON.
- **`execute_skill`**: policy + token (exceto auto_approve) + verify + schema + contracts.
- **`execute_skill_unchecked`**: sem token (só após auth do caller — preferir `execute_skill`).
- **`ContractAction::RetrySkill`**: retorna Err (`retry_unavailable`) — sem loop.

## Integration

Depends on `event-bus` (`CapabilityToken`). Hermes Trust Contain must honor `check_or_cache`.
jarbas TTS/STT usam `required_tokens: [1]` (Legacy Hermes).
