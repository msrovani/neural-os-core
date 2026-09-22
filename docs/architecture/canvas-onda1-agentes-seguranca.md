# Canvas — Onda 1: §1 Sistema Neural + §8 Agentes + §9 Segurança (s393)

> Premissas AIOS: tudo-é-agente, fail-closed, honestidade (zero telemetria mentirosa), HITL, sem stub que finge pronto.

## Achados (exp-3) — ordem de aplicação
### HIGH
- **H1** `k_ai/audit.rs:71` Merkle encadeia entry com `signature=zeros` → refuse/Quarantine + `verify_chain` checar sig.
- **H2** `k_ai/safety_invariants.rs:34,134` I3 sempre Warning + `all_pass` ignora Warning → Warning conta como violation; I3 sem dado pós-Phase 6 = Violation.
- **H3** `hermes/agents.rs:2904` AutoLearn persiste `dummy_trace` + "TRINITY APRENDEU!" → ramo vazio: sem persist, `bootstrap_skipped/degraded`, gate em `n>0`.
- **H4** `k_nano/identity.rs:59` + `k_hal/gpu/kernel_pack.rs:196` pack aceita session-PK → packs/skills/marketplace só-pinned; sessão só p/ audit local.
- **H5** `hermes/cognitive_bridge.rs:130` "BGE" é hash-bucket → tag `[SESSION+HASH-FALLBACK]`, HNSW off até BGE real.
- **H6** `hermes/agents.rs:891` Consciousness com literais `0,0` + autocontadores → derivar de registry/budget ou flag `degraded self_report`.
- **H7** `hermes/self_evolve.rs:215` sign-fallback + verify só-estrutural → sem `Ok(sign)` = Reject; registro exige wasmi-pass ou HITL.
- **H8** `hermes/wasmi_rt.rs:326` `generate_wasm_module` = `i32.const 42` → `#[cfg(test)]`/rename; produtivo sem bytes = `Err("no-wasm-bytes")`.
### MED (M9 cross-os trust · M10 threshold 0.05→0.35+margem · M11 MHI tiers mortos · M12 membrane deny · M13 budget gate real · M14 DHCP fonte NetPhy · M15 HalOffer AbsentCached · M16 urgency default)
### LOW (L17 Box/Vec doc · L18 UI-boost double-tick · L19 modal damage-rect · L20 sleep proposal-only)
### Simplificações: merkle única · TRINITY fonte única · `tick(&Snapshot)` · `with_profile()` · bind 1 struct · deletar órfãos/stubs (`compute_port stub`, `host_sse_stub`)

## Upstream (lib-2) — só ideias, nenhum bump pendente
- wasmi/smoltcp/embedded-graphics/ed25519-compact **já no latest**. Swarm morto→Agents SDK (`inputFilter`/handoffs); AutoGen em manutenção→Agent Framework (não aprofundar).
- embedded-graphics: `embedded-text` (wrap), unicodefonts (acentos PT-BR), `simulator` (teste host) — sem trocar versão.
- Papers confirmam `r3.rs` (2510.11370); increments: idade+`tau_max`, prioridade por |vantagem|, fresh-anchored, filtro GRESO-like.
- **MCP 2026-07-28 breaking** (handshake removido, stateless, `server/discover`) → `mcp_client` precisa probe de negociação (mais acionável).
- Wetware checklist → membrane; WeftOS≠weft na ADR-0076.

## Verificação por lane
`cargo check -p k-ai/hermes/k-nano/k-hal/cortex/agent-core/jarbas` 0 errors; sem XMM; post: commit+tag s393.
