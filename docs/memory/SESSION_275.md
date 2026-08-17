# SESSION_275 — Jarbas compositor honesto (J-01..J-09)

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-17  
**Escopo:** hit-test, chat/apps na tela, overlays no `render()`, HDA fonte única, GPU log honesto, CARD_ACTION, HITL card 8001, HUD LLM, dock+toasts. HEAD anterior = SESSION_274. Sem ADR nova (ADR-0045 áudio; ADR-0058 UI).

## Mapa

O compositor (`JarbasDesktop::render()`) é o único pintor do frame (SESSION_261). J-01..J-09 fecham mentiras/seams mortos na mesma regra:

| Fix | Onde | Problema |
|-----|------|----------|
| J-01/J-11 | `compositor::handle_desktop_click` | Hit-test 35% Hermes (já deletado no s261) ainda com `return` cedo → `card_click`/dock/drag mortos. Clique no orb/grafo deve ser miss. |
| J-02 | `show_app` / `toggle_app` / `ensure_hermes_overlay` | Janela em `windows[]` ≠ na tela — `render()` itera floating da workspace via `add_window_floating`. |
| J-03 | `display/overlay.rs` | EMBED/RENDER_WINDOW desenhados no `tick()` → apagados no `fill` do mesmo frame. |
| J-04 | `k_nano::audio::hda` + `k_hal` facade | Dois `init_hda` (k_nano IRQ 0x30 vs k_hal GCTL reset) → poll no controller/BAR errado. Oneshot resetava GCTL se já bound. |
| J-05 | `gpu_backend` + DisplayAgent log | `BackendState::Ready` lido como matmul GPU. Canário CE ≠ KernelPack. |
| J-06 | `card_click` + `sync_floating_rect` | Botão de card não publicava `CARD_ACTION`; drag só mexia `windows[]` (floating dessincronizado). |
| J-07 | DisplayAgent drain + card 8001 | HITL/HERMES ilimitado num tick (EventBus unbounded); sem `has_pending`; `/approve`/`/deny` só texto. |
| J-08 | `cortex::infer_in_flight` + gauges | HUD LLM ocupado só por flag; GPU/HD inventavam 0.15/0.55. Relógios `draw_status_gauges` **não** pintados (HUD s273 intacto). |
| J-09 | `render()` camada 4 | Dock e toasts fora do único path de swap. |

## Decisão

| Anel | Mudança |
|------|---------|
| `jarbas::display::compositor` | Hit-test canónico dock → cards → janelas; chat/apps via `add_window_floating`; `paint_overlays()` + `dock.render()` + `NotificationQueue` no `render()`; drag/resize sincroniza floating; `CARD_ACTION` no botão. |
| `jarbas::display::overlay` | `EMBED_MARKS` / `RENDER_OVERLAYS` (IrqSafeLock). Tick grava; `render()` pinta. |
| `jarbas::display::agent` | `handle_pointer_click` delega `handle_desktop_click`; toasts → NotificationQueue; drain HITL teto 16; card HITL 8001; `has_pending` nos receivers. |
| `k_nano::audio::hda` | Fonte única IRQ 0x30: `is_ready()`, `write_hda_playback` na mesma instância do BAR/SD1. |
| `k_hal::audio::hda` | Facade: Oneshot **não** reseta GCTL se `is_ready()`; senão `init_hda()` do k_nano. |
| `cortex` | `INFER_IN_FLIGHT` + `InferGuard` no `generate_via_model`; HUD lê `llm_busy() \|\| infer_in_flight()`. |
| `gauges` | GPU/HD honestos (`n/a` sem meter/setores); snapshot MetricsAgent; compositor **não** chama `draw_status_gauges`. |
| bin A-006 | `set_llm_busy(true/false)` em torno do generate. |

## O que NÃO foi feito (honesto)

- **SD1 pin OUT/DAC** — `write_hda_playback` escreve o BDL; roteamento de pin widget codec (OUT/DAC) continua **AWAITING_HW**. Sem evidência de speaker em silício.
- **EventBus unbounded** — teto 16/tick evita starvation do DisplayAgent; a fila em si não tem bound. Residual estrutural (#539).
- Card 8001 publica `CARD_ACTION`; Hermes ainda parseia `/approve`/`/deny` no chat — wire botão→comando HITL incompleto (#538).
- `draw_status_gauges` existe mas não entra no frame (de propósito). HUD compacto SESSION_273 permanece.
- KernelPack W2A8 / matmul no device — SESSION_274; J-05 só o log FE.

## Testes

- `cargo check -p jarbas --release --target-dir target/check-s275-jar` — 0 erros
- `cargo check -p neural-kernel --features fat-boot-log --target-dir target/check-s275-nk` — 0 erros

## Lição

O `tick()` pode **preparar** estado no mesmo quantum em que `render()` corre — qualquer `fill_rect`/`draw_*` no framebuffer antes do `render()` é lixo. Overlay, toast e dock só existem se o snapshot está num static e o paint é *dentro* de `render()`. Janela em `windows[]` sem `add_window_floating` é fantasma. Hit-test com `return` cedo mata a camada seguinte. Dois `init_hda` = IRQ no BAR A e poll no BAR B. `BackendState::Ready` é canário, não matemática.
