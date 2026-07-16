# SESSION 111 — Handoff voz 107 → Sprint Sound + pista limpa ADR-0042

**Data:** 2026-07-16  
**Tipo:** docs-only (sem mudança de runtime)  
**Versão nota:** v1.7.3 (docs)

## Objetivo
Limpar a pista de desenvolvimento: fechar Sprint 107 para voz, migrar leftovers para **Sprint Sound (reaberta)**, e apontar o próximo trabalho ativo para **ADR-0042 N2→N5**.

## Contexto
- Sprint 107 já estava **FECHADA** operacionalmente (PASS parcial forte+ — clima e2e, HWEXPERT, Piper neural-lite, EventBus skinny). Ver `SESSION_107_CLOSE.md` / `SESSION_110.md`.
- Gaps de voz ainda apareciam como “107” / “→108” em TODO, ROADMAP, AGENTS, STATE, IDEA_BANK, ADR-0045.
- Sprint Sound historicamente ✅ na base (HDA/Piper/VAD/SER) mas IDEA ainda tinha futuros — **reaberta** como home do backlog.

## Entregue nesta sessão (docs)
| Doc | Mudança |
|-----|---------|
| TODO.md | 107 ✅ fechada; seção Sound reaberta; ADR-42 pista ativa |
| ROADMAP.md | idem; próximos passos liderados por N2 |
| AGENTS.md | Current Sprint = ADR-0042; 107 ✅; Sound ▶️ |
| STATE.md | v1.7.3 docs; pista limpa; backlog → Sound |
| IDEA_BANK | #84, #315.25, #438 voz → Sound; #435–437 → ADR-42 |
| TECNOLOGIAS §5 | leftovers → Sound; tools 11.6/11.7 STT/Piper |
| ADR-0045 | gaps → Sound; WakeWord registrado alinhado |
| SESSION_INDEX | entrada 111 |
| CHANGELOG | [1.7.3] docs |

## Tabela de migração (voz)

| Item | Era (107) | Agora |
|------|-----------|-------|
| STT retrain PCM-real | gap 107 / →108 | Sprint Sound |
| Mic→Wake→STT→LLM→TTS runtime e2e | skinny ✅; e2e aberto | Sprint Sound |
| Piper VITS pleno | neural-lite ▶️ | Sprint Sound |
| Soft-float voice latency | ❌ blocker 107 | Sprint Sound (defer) |
| UAC (#84) | 107+ | Sprint Sound |
| jarbas/audio wire | Part B ▶️ | Sprint Sound |
| VAD refinements | ⏳ 107 | Sprint Sound |
| SER refinements | base Sound | Sprint Sound (polish) |
| Wake ML polish / Mic→WAKE e2e | registrado ✅ | Sprint Sound |
| WakeWord registry | ✅ 107 | permanece ✅ (entregue) |
| Clima GEN+TTS+FB / EventBus skinny | ✅ 107 | permanece ✅ (entregue) |

## Não feito (honesto)
- Nenhuma feature de áudio implementada nesta sessão.
- Soft-float / STT / Piper VITS **não** “resolvidos” — só rehomeados.
- Sem push; commit docs-only.

## Próximo passo sugerido
**ADR-0042 N2** — k-ai SelfHeal gated (HEALTH_ISSUE / inventário VID / Trust).  
Voz em paralelo só se explicitamente priorizada sob Sprint Sound.
