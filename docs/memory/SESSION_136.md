# SESSION_136 — HANR → Neural Hermes (marketplace + trust signed)

**Data:** 2026-07-17  
**ADR:** 0053 (+ residual 0051 session key)

## Entregue

### Wave 0 — Trust
- `k_nano::identity`: `init_session_identity`, `sign_session`, `verify_trusted` (trusted ∪ session)
- `k_ai::audit`: `signature` por entry; verify chain+sig
- PackageHub `sign_artifact_md` / seeds e drafts assinados; unsigned Deny
- Audit wire: apply_approved, ApprovalGate.resolve, self_evolve verify

### Wave 1 — Premissas HANR
- `memory_store`: USER.md / MEMORY.md / SOUL.md + prompt_slice
- Progressive: `/skills` L0, `/skill` L1
- SkillMarket unificado (`skill_market.rs`) + outcomes self_evolve/wasm

### Wave 2–3 — Marketplace
- `marketplace.rs`: list/search/install/promote/rm/index + fetch HTTP allowlist + resign
- Comandos Hermes `/market …`

### Wave 4
- MCP JSON-RPC mínimo (`tools/list`, `tools/call`) via `/mcp`
- Jarbas SOUL path `/mnt/neural/SOUL.md`
- Docs ADR-0053, TECNOLOGIAS 9.2–9.4, IDEA #315.19 ✅

## Aceite
- `cargo check --release -p hermes` 0 erros (wave)
- Bin: `cargo clean -p neural-kernel && cargo check --release` (gate sessão)

## Fora / residual
- DNS para hostnames no market fetch (só IP MVP)
- MCP TCP listener pleno
- Messaging gateway / Honcho / FTS5
