# SESSION_395 — Onda 1: §1 Sistema Neural + §8 Agentes + §9 Segurança (loop TECNOLOGIAS)

**Programa:** loop por tecnologia do TECNOLOGIAS.md (premissas → bughunt → upstream → canvas → fixes High→Low → pós-tarefas).
**Canvas:** `docs/architecture/canvas-onda1-agentes-seguranca.md`.
> Numeração: s393 (onda1) foi ocupada por commit paralelo f8773265 (BEI); esta sessão usa **s395**.

## Achados (exp-3) → fixes (3 lanes)
H1–H8/M10–16/L17–19 conforme canvas: audit sem sig (H1), safety warnings (H2), autolearn honesty (H3), verify_pinned (H4), hash-fallback (H5), consciousness drivers reais (H6), self-evolve HITL (H7), canned wasm module test-only (H8), trinity margin (M10), mhi tiers (M11), membrane /jail (M12), HalOffer Absent (M15), urgency default (M16), KV doc (L17), modal rect (L19).

## Upstream (lib-2) — nenhum bump pendente
wasmi/smoltcp/embedded-graphics/ed25519-compact no latest; Swarm→Agents SDK; AutoGen manutenção; papers R3/GRPO confirmam r3.rs; **MCP 2026-07-28 breaking** → mcp_client precisa probe; desambiguí URL/commit na ADR-0076.

## Pós-tarefas
`cargo check --release` 0 errors; hermes 205/0; sem DUP novo; commit + tag s395.
