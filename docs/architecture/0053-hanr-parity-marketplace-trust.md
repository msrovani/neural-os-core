# ADR-0053: HANR Parity — Marketplace + Trust Signed + Memory

**Data:** 2026-07-17  
**Status:** Accepted (MVP)  
**Lifecycle (INDEX):** `fazendo`  
**Relacionadas:** ADR-0015 (Hermes bare-metal), ADR-0051 (PackageHub), ADR-0052 (Artifact Spec), IDEA #315.19

## Contexto

O Hermes Agent (Nous Research / HANR) define premissas: closed learning loop, skills progressivas, memória USER/MEMORY/SOUL, marketplace, MCP, trust. O Neural Hermes já era o SO; faltava paridade semântica + endurecer assinatura.

## Decisão

1. **Session Ed25519** (`k_nano::identity`) — keypair no boot; `sign_session` + `verify_trusted` aceita trusted **ou** session PK. Serial `[TRUST] session_pk=…` (nunca secret).
2. **PackageHub auto-sign** — drafts `hermes_created` e seeds recebem `content_hash` + `signature`; unsigned → Deny (nunca Auto).
3. **Merkle Audit Ed25519** (`k_ai::audit`) — cada entry assina `entry_hash`; wire em PackageHub / Approval / self_evolve.
4. **Memória HANR tier-1** — `/mnt/neural/USER.md`, `MEMORY.md`, `SOUL.md` + `/remember` `/soul` `/memory`.
5. **Progressive disclosure** — `/skills` L0 (≤60 chars) + `/skill <name>` L1.
6. **Marketplace** — local NeuralFS (`/market …`) + Net allowlist HTTP (`/market fetch`) com resign session.
7. **MCP mínimo** — JSON-RPC `tools/list` + `tools/call` via `/mcp`.
8. **HITL UI** — Confirm/Escalate → Jarbas por default (`HITL_REQUEST`); `/ui terminal` abre overlay estilo HANR com catálogo `/xxx` (`/commands`). Hermes não é a superfície do usuário.

## Fora de escopo

Messaging gateway 20+, Docker/Modal, Honcho, FTS5, scrape genérico do Skills Hub Nous.

## Critérios MVP

- [x] Session sign + verify
- [x] Audit signed
- [x] `/market` local + fetch allowlist
- [x] `/skills` / `/skill` / memória
- [x] MCP JSON-RPC mínimo
- [x] SOUL path Jarbas → `/mnt/neural/SOUL.md`
- [x] HITL via Jarbas ou terminal HANR (`/ui`)

---

## Planos Cursor implementados

### `HANR Hermes Port` (`hanr_hermes_port`)

| Wave | Entrega | Status |
|------|---------|--------|
| **0** Trust | Session Ed25519 + `verify_trusted(session)` + PackageHub auto-sign + Merkle Audit Ed25519 | ✅ |
| **1** Skills/Mem | Progressive disclosure L0/L1 + USER/MEMORY/SOUL.md + SkillMarket unificado | ✅ |
| **2** Market local | `/market` search/install/promote/remove + INDEX + HITL | ✅ |
| **3** Market net | HTTP fetch allowlist → sandbox → re-sign → NeuralFS | ✅ |
| **4** MCP/SOUL/docs | MCP JSON-RPC mínimo + SOUL wire + ADR/SESSION/TECNOLOGIAS | ✅ |

**Fora de escopo (cumprido):** messaging gateway 20+, Docker/Modal, Honcho, fine-tune Nous.

**Residuals:** TCP listener MCP pleno; FTS5; scrape genérico Skills Hub.

Dependências: ADR-0051 (hub), ADR-0052 (contrato), ADR-0041 CapGate (sandbox parcial).
