# SESSION_379 — hermes bughunt AIOS

**Sprint:** v1.9.99-s379 TEST  
**Data:** 2026-09-21  
**Foco:** CapGate/WASM/TLS/evolve honesty + wasmi 0.47.2  
**Canvas:** [hermes-bughunt-s379.canvas.tsx](/Users/msrov/.cursor/projects/c-DEV-neural-os-core-latest/canvases/hermes-bughunt-s379.canvas.tsx)

## Premissas

- Hermes = R3 orquestração (ADR-0059 wasmi A; B/C gated Ring3)
- Cap granted ≠ op wired — stub Ok é honesty falsa (ADR-0088)
- TLS bridge lesson 241: https:// sem bridge = deny, nunca HTTP :443
- slog ADR-0092: sev ∈ {ok,warn,fail,trace}
- PackageHub deny-by-default + signed só se Ed25519 confere (ADR-0052)

## Aplicado (ordem)

| ID | Fix |
|----|-----|
| H1 | `evolve::hot_swap` sandbox primeiro; `live`/`prev`; registry só se OK; 2 tests |
| H2 | wasmi `check_cap` via RiskLevel; net/fs/gpu unwired=Deny+trap (não Ok) |
| H4 | `ApprovalGate::resolution` + `can_execute` honra deny; PERM slog ADR-0092 |
| H3 | TLS ready⇔bridge; `fetch_https` → `tls_not_ready`; smoke exige bridge |
| M1 | matrix_learn: draft HITL Escalate; sign fail-closed; sem "Aprendi" |
| M2 | package_hub seed `signed=check_signature_content` |
| M3 | `session_load` clear + HNSW só entries |
| M4 | wasm_build GPU tests → assert trap sem KernelPack |
| L1 | elf_loader APK magic + orphan headers; adaptation stub; fs/codemap MHI |
| L2 | wasmi **0.47.2** (0.47.0 yanked); 2.x = IDEA #599 |

## Verificação

- `cargo test -p hermes --lib -- --test-threads=1` → **203/203**
- `cargo check -p hermes --release` / `neural-kernel --release` → 0 erros

## Residual

- IDEA #599 wasmi 2.x sprint
- IDEA #600 DynSkill CapGate token
- IDEA #597 smoltcp/spin/x86_64 workspace
- IDEA #604 wire aios_net/fs + orphans cleanup
