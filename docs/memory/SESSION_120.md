# SESSION 120 — Marco v1.8.0 (pós-jornada K²CHJ)

**Data:** 2026-07-16  
**Versão:** v1.8.0  
**Pista:** Consolidação ADR-0042 + wire crates N2.5→N5.7

## Objetivo

Fechar pós-tarefas: aprender → memorizar → documentar → versionar `v1.8.0` → push.

## Marco alcançado

| Critério | Status |
|----------|--------|
| ADR-0042 N1–N5 funcionais | ✅ v1.7.0…v1.7.7 |
| Wire crates no bin | ✅ N2.5 v1.7.8 … N5.7 v1.7.11 |
| Sprint 107 Voice | ✅ PASS parcial forte+ |
| `cargo clean -p neural-kernel && cargo nk` | ✅ 0 erros |
| Documentação STATE/CHANGELOG/AGENTS/TODO/ADR | ✅ |
| Tag `v1.8.0` + push | ✅ esta sessão |

## Lições aprendidas (wire monólito → crates)

1. **Alias dep `*-crate`** — evita conflito entre `mod cortex` e dep `cortex` (`cortex-crate`, `hermes-crate`, `jarbas-crate`).
2. **`k_nano` sem `global-alloc`** — feature default OFF; único `#[global_allocator]` no bin `neural-kernel`.
3. **Bridge globals** — `memory` → `k_nano::memory`; `EVENT_BUS` → `k_nano::globals::EVENT_BUS` (SelfHeal HEALTH_ISSUE no mesmo bus).
4. **Deletar espelhos em lotes** — N2.5 (2) → N3.5 (9) → N4.6 (~37) → N5.7 (29); ~75 arquivos removidos do monólito.
5. **Residuals intencionais** — integração bin-only onde API diverge: `cortex.rs`, `bpe.rs`, `audio/*`, `agents.rs`, `net*`, `fs/*`, `jarbas_fb.rs`, `jarbas_bridge.rs`.
6. **ADR-0045 audio** — truth permanece `neural-kernel/src/audio` até Sprint Sound convergir wakeword path com `jarbas::audio`.
7. **Gate v2.0.0** — pré-requisitos funcionais ✅; declaração exige review formal (qualidade voz ≠ gate N2–N5).

## Commits da jornada wire (referência)

```
8740bfd feat(adr-42): N2.5 — wire k_ai into neural-kernel bin
9a9ab57 feat(adr-42): N3.5 — wire cortex crate into neural-kernel bin
574cf41 feat(adr-42): N4.6 — wire hermes crate into neural-kernel bin
95f8967 feat(adr-42): N5.7 — wire jarbas crate into neural-kernel bin
```

## Próximo

| Track | Ação |
|-------|------|
| **Sprint Sound** | STT retrain, Mic→Wake runtime, Piper VITS, soft-float, UAC |
| **Gate v2.0.0** | Review conjunto ADR-0042 + qualidade voz |
| **Sprint 108** | Self-evolving agents (paralelo) |
| **Residuals** | Migrar `cortex.rs`/`agents.rs`/`net*` quando APIs estabilizarem |

## Não é v1.8.0

- `v2.0.0` declarado automaticamente
- Voz production-grade (→ Sprint Sound)
- Eliminação total de código duplicado no monólito (residuals documentados)
