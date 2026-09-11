# SESSION_331 — Orb v2 (MCU-JARVIS) + FFT Goertzel + Hub Health panel (agent-driven) — 2026-09-11

**Objetivo:** revisar todo o mecanismo do orb do Jarbas (o maintainer achou "tosco" mesmo a 10-15 fps) e agregar o diagnóstico do hub numa janela semi-transparente (1 foto = toda a saúde).

**Fonte:** análise profunda (oracle + designer) do pipeline de render; metal s330 (10-15 fps, mouse morto).

---

## 1. Diagnóstico (por que era "tosco" e lento)

1. **fps capado por LCM acidental** — dois gates de tick independentes (`%3` do orb vs `hz/30` do frame) batiam a cada 4 ticks = **15 fps** a 60 Hz (10 a 30, 30 a 120). O renderer NÃO era o gargalo.
2. **Animação por contagem de frames, não por tempo** — `spin += 3.2`/frame, `sinf(frame*0.028)`, partículas `frame*speed` → mudar o rail muda o tempo.
3. **Transição de cor = serrilhado perpétuo** — `lerp(prev, old, frame%24)` exibido para sempre.
4. **Paleta de estado morta por mismatch de string** — `jarbas_color` casava `"DREAM|THINK|IDLE|ALERT"` mas o writer emitia `"SPEAK|PROCESS|LISTEN|IDLE"`.
5. **2 divisões inteiras por pixel blended** (`fill_circle_gradient_scanline`) + scanlines opacas cortando o core.
6. **Grid piscava 1-em-4** (blit só em `frame%4` com bbox limpa todo paint).
7. **FFT = assassino do sistema** — ~129k `sinf/cosf` por buffer de áudio, no caminho do mixer/voice.
8. `avatar8.rs` morto (compositor descartava) com `sqrtf` por pixel; `draw_voice_waveform` sem callers.

## 2. Orb v2 (implementado)

- **Gate único por wall-clock** (`now_us - last_present >= 1e6/30`), sem `%3`; 1 paint/tick; dirty-rect mantido.
- **Time-driven**: dt do TSC acumula fase (spin/breath/partículas/ripples); fallback ao período de tick se `tsc_hz()==0`.
- **Cor converge** (`prev += (target-prev)*k`, k≈51/256) — sem serrilhado.
- **`OrbState` enum** (Idle/Listening/Thinking/Speaking/Dreaming/Degraded/Alert/Updating) com dwell ≥400ms, ALERT preempta, SPEAKING tail 1.5s — dirigido por sinais REAIS (WAKEWORD, `infer_queue::has_work`/LLM_STREAM, AUDIO_OUT/INFER_TTS_PARTIAL, HEALTH_ISSUE, SLEEP_PHASE novo, INSTALLER_BUSY).
- **Paleta em rails**: corpo sempre ciano; acentos só em rim/ticks/halo (violeta=THINK, âmbar=DEGRADED, vermelho=ALERT, verde=UPDATE, ice=core).
- **Camadas**: bbox → hex lattice (cache, todo paint) → halo em bandas de alpha (SWAR, **zero divisão por pixel**) → corpo/inner/core/specular (spans flat) → rim → 2-3 anéis elípticos (`ring_spans` de LUT de meia-largura, sem trig por ponto) → 24 ticks → 12 sparks → ≤8 peers. bbox ~2.9R.
- **Primitivas novas no `fb.rs`**: `fill_rect_darken_tint` (SWAR), `fill_circle_alpha_bands` + `HALF_WIDTH_LUT`, `ring_spans`, `sin_q8/cos_q8`.
- **LOD** do `frame_cost_us()` (dwell 32 paints): degrada, nunca congela.
- **Bench host**: **112-130 µs/frame** (~264k px) — ~30× sob o alvo de 4 ms.
- `avatar8.rs` deletado (281 LOC, `sqrtf` por pixel); `draw_voice_waveform` removido (0 callers); `SLEEP_PHASE` publicado pelo SleepCycleAgent (sinal real do DREAM).

## 3. FFT Goertzel (fixer)

`process_audio_fft` reescrito: Goertzel por bin (2 mul + 2 add/amostra, **sem trig**), janela Hann em LUT (`spin::Once`), tap capado em 1024 amostras (256 transformadas) — um burst de 64 chunks do HDA não trava o tick; VAD/STT continuam vendo tudo. Fallback idle via `SIN_LUT` (sem libm). Teste host: silêncio→0, tom 1kHz→bin certo.

## 4. Hub Health panel (agent-driven — decisão do maintainer)

**Arquitetura: Agente + compositor** (política fora da UI):
- **`HubHealthAgent`** (hermes, EventDriven, `crates/hermes/src/hub_health.rs`): assina `HUB_HEALTH_CMD` (toggle/open/close de F12/orb click/badge/Esc), `HEALTH_ISSUE` (auto-open 8s), `MESH_HEALTH`, `TIMER_CAP`; amostra o snapshot a ~2 Hz; decide pill OK/WARN/FAIL + worst subsystem; publica `HUB_HEALTH_STATE` só em mudança. Registrado no fleet.
- **Compositor**: **zero política** — lê o estado do agente (`panel_gen`/`sync_hub_policy`), anima o slide na rising edge, desenha pill/highlight dos códigos do agente, rows do snapshot `Copy`.
- **Dados medidos, não hardcoded**: `HubHealth` (Copy) preenchido de `timer_cap`/`apic::tick_hz`/`TIMER_TICKS`/`frame_cost_us`/`xhci_last_stage`/`host_ccs_count`/`USB_MSC`/heap/agents/`LAST_EXC_*` (lock-free no IRQ). **Dado ausente = `n/a`, nunca 0.**
- **Render**: placa de vidro à direita (α≈216/255 `#0A1220`, borda `#0A6E96`, acento ciano), ~13 rows ASCII 8x16, cores ok/warn/fail; `dirty_panel` no early-exit + `swap_rect`; F12/orb click/badge abrem; badge de 4 LEDs (USB/MEM/NET/AGENTS) no HUD quando fechado.
- `E0308` do crash fechado (`fnv1a`); testes: jarbas **67 pass**/3 fail (pré-existentes `jarvis::soul_*`), hermes hub_health 2 pass.

## 5. Estado

- Tree: +2245/−693 em 17 arquivos; `cargo nk`/`cargo check --release` = 0 erros.
- **Pendências**: USB/MSC no metal (diagnóstico FB `xHCI cands/stage/ccs` pronto p/ ler), mouse (USB-HID/I2C-HID), modelos (`skip models (no MSC)`), licença AGPL vs MIT (#555).

## 6. Lições

- **Instrumento invisível = instrumento inexistente**: `slog` não pinta FB; diagnóstico de metal precisa de canal FB (`boot_progress_line`/painel).
- **Gate por LCM acidental** é invisível no código e capa fps — simular a aritmética dos gates antes de culpar o renderer.
- **Política fora da UI**: o painel decidiu nada; o agente decide, o compositor desenha (padrão "tudo é agente").
- **`n/a` ≠ 0**: painel de diagnóstico que mente derrota o propósito.
