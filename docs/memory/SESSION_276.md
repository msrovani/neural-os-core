# SESSION_276 — Jarbas compositor honesto + HDA único + infer_in_flight

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-21  
**Escopo:** Port seletivo de `ac4e853` (branch `aios-chj-premissas`) para `main` **sem** colidir com SESSION_275 mesh. Cita SESSION_261 (paint só no `render()`).

## O que entrou

| Peça | Fix |
|------|-----|
| Compositor | Click/dock/`show_app` floating; toasts/overlays no `render()` |
| `overlay.rs` | Snapshots EMBED/RENDER — tick grava, `render()` pinta |
| CARD_ACTION | Botão do card publica no EventBus (FeedbackAgent já escutava) |
| HDA | Fonte única `k_nano::audio::hda` + facade `k_hal` (sem double GCTL) |
| HUD | `cortex::infer_in_flight` + `set_llm_busy` no generate; gauges sem inventar GPU/HD |

## O que NÃO veio do tip `c234138`

- agent-core CrewManifest / null-vtable (main já tem cache+stack guard)
- LAPIC one-shot
- mesh_tick em network_agent (Hermes/`bei_init` já cobrem)

## Verificação

- `cargo check --release` (OPT_LEVEL=1) — 0 erros nos crates tocados
- Aceite UI: dock/chat/card no QEMU `-Window` (smoke visual residual)

## Colisão de nome

SESSION_275 em `main` = mesh P2P GOAL1–3. O commit original rotulava isto como SESSION_275 jarbas — renumerado aqui para **276**.
