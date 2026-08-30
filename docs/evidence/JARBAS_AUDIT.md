# 📊 Jarbas Crate — Auditoria Completa de Otimização

**13,288 LOC** | 65 arquivos | 400+ pub fns | 0 testes host | 129 lock() calls | 282 alocações no hotpath

---

## 🏗️ Arquitetura Atual

```
jarbas/src/
├── display/          (27 arquivos)  ~8,500 LOC  — compositor, WM, cards, font, theme
│   ├── compositor.rs    1,435 LOC  — render loop principal (hotpath)
│   ├── agent.rs           960 LOC  — DisplayAgent (14 receivers EventBus)
│   ├── fb.rs              961 LOC  — framebuffer + double buffer + SIN LUT
│   ├── card.rs            472 LOC  — cards declarativos (embedded-graphics)
│   ├── chat_window.rs     507 LOC  — chat Onyx-style
│   ├── gauges.rs          355 LOC  — HUD métricas
│   ├── soul_mirror.rs     184 LOC  — orb afetivo
│   ├── avatar8.rs         281 LOC  — avatar animado
│   ├── theme.rs           190 LOC  — paletas Dark/Light/Cosmic
│   └── ... (17 arquivos)
├── audio/            (19 arquivos)  ~3,200 LOC  — TTS, STT, VAD, Piper, wake word
│   ├── piper.rs           343 LOC  — Piper TTS engine
│   ├── stt.rs             460 LOC  — STT engine
│   ├── tts.rs             314 LOC  — Formant synthesis
│   ├── jarvis.rs          421 LOC  — JarbasAgent (greeting, streaming TTS)
│   ├── voice.rs           222 LOC  — JarbasVoiceAgent (wake-gated STT)
│   ├── vad.rs             130 LOC  — Voice Activity Detection
│   ├── ser.rs             186 LOC  — Speech Emotion Recognition
│   ├── wakeword.rs        192 LOC  — Wake word detection
│   └── ... (11 arquivos)
├── cards/            (3 arquivos)    ~400 LOC   — file manager, terminal, disk selection
├── jarvis.rs                     679 LOC  — JarbasEngine (emotion, persona, cache, session)
├── vconsole.rs                   182 LOC  — virtual console
├── uvc_driver.rs                 290 LOC  — USB Video Class
├── virtio_gpu.rs                  40 LOC  — VirtIO-GPU bridge
├── screensaver.rs                 30 LOC  — screensaver stub
└── lib.rs                         16 LOC  — module declarations
```

---

## 🔴 Gaps Críticos (P0)

### 1. **ZERO testes host** — Nenhum teste unitário no módulo inteiro

| Módulo | LOC | Testes | Veredicto |
|--------|-----|--------|-----------|
| display/compositor.rs | 1,435 | 0 | ❌ CRÍTICO |
| display/agent.rs | 960 | 0 | ❌ CRÍTICO |
| audio/jarvis.rs | 421 | 0 | ❌ |
| jarvis.rs (engine) | 679 | 0 | ❌ |
| audio/stt.rs | 460 | 0 | ❌ |
| audio/tts.rs | 314 | 0 | ❌ |
| ALL | 13,288 | **0** | ❌ |

**Impacto:** Qualquer refactor quebra silenciosamente. O Redox tem ~200 testes no Orbital.

### 2. **DisplayAgent com 14 receivers** — God Object

```
DisplayAgent::new() subscreve:
  1. HERMES_RESPONSE
  2. KEYBOARD_ECHO
  3. USER_INTENT
  4. STT_TEXT
  5. RENDER_REGISTER
  6. RENDER_WINDOW
  7. MOUSE_MOVED
  8. MOUSE_CLICK
  9. UI_SPEC
  10. HITL_REQUEST
  11. HITL_TERMINAL
  12. MEMORY_NUDGE
  13. TOAST
  14. KEY_EVENT
  15. LLM_STREAM
  16. MESH_HEALTH (lazy)
  17. INSTALL_UI (lazy)
```

**Impacto:** tick() do DisplayAgent processa TUDO (mouse, keyboard, LLM stream, mesh, HITL, cards, toasts, avatars). Um recv lento trava todo o render.

### 3. **129 lock() no hotpath** — IrqSafeLock no render loop

O compositor locka `COMPOSITOR`, `MOUSE_X`, `MOUSE_Y`, `MOUSE_BUTTONS`, `FOCUS_MODE`, `POWER_STATE`, `POWER_BANNER`, `WELCOME_BANNER`, `CHAT_WINDOW`, `RENDER_REGISTRY` — tudo no mesmo tick de 60Hz.

**Impacto:** Priority inversion em IRQ context. O Redox usa locks minimizados + dirty flags.

### 4. **282 alocações Vec/String no render loop**

`fill_rect_fast`, `draw_text`, `format!` — cada frame aloca e libera memória.

**Impacto:** Fragmentação do heap + pauses visíveis em 60Hz.

---

## 🟡 Gaps Médios (P1)

### 5. **SoulMirrorRenderer não tem teste de normalização**

O `from_affect()` converte AffectVector → SoulMirrorState mas não tem teste que valide se `valence=-1.0` → cor vermelha ou `valence=1.0` → cor verde.

### 6. **mesh_health_json::parse é hand-written parser sem teste**

50+ linhas de parsing JSON manual — qualquer byte inesperado causa panic em no_std.

### 7. **EmotionAnalysis.classify_weighted usa const W[16][7] hardcoded**

Os pesos são "treinados" mas nunca validados contra um holdout. O classificador pode estar retornando neutral para tudo.

### 8. **NotificationQueue não tem limite duro**

`push()` nunca recusa — se 1000 notificações chegarem, todas entram no Vec.

### 9. **ChatWindow::process_packet aloca VecDeque de TimelineStep sem limite**

Cada tool call do LLM cria um TimelineStep com `content: String`. Sem cap, memória cresce infinitamente.

### 10. **TTS Formant synthesis não tem fallback para PT-BR**

O PHONEMES array tem 36 fonemas do inglês. Português tem sons diferentes (ã, õ, lh, nh, rr).

---

## 🟢 O que Funciona Bem

| Componente | Estado | Nota |
|-----------|--------|------|
| Dirty-rects per-layer (FASE 2) | ✅ | Orb/HUD/windows/mesh/cursor cada um com flag |
| FPS control (TARGET_FRAME_TICKS=3) | ✅ | ~60Hz sem overrun |
| SoulMirror from AffectVector | ✅ | Pipeline completo: affect → color/pulse/rotation |
| Cards declarativos (ADR-0058) | ✅ | UiDeclaration → embedded-graphics render |
| ChatWindow streaming typewriter | ✅ | Timeline expandível |
| Power dialog (OFF/Hibernate/Reboot) | ✅ | Hit-test + confirmation |
| Per-core load bars (HUD) | ✅ | 16 cores, color-coded |
| Wake word → VAD → STT → LLM | ✅ | Pipeline completo |
| P2P mesh graph visual | ✅ | Node visualization no orb |

---

## 📐 Plano de Correção

### Fase 1: Fundação (testes + god object split)

| # | Ação | LOC estimado | Impacto |
|---|------|-------------|---------|
| **P0-1** | Adicionar 30+ testes host ao jarvis.rs (EmotionAnalysis, SoulProfile, SessionHistory, NotificationGate, DreamEngine, EgoLayer, Heartbeat) | +200 | Elimina gap de qualidade |
| **P0-2** | Split DisplayAgent em 3: InputHandler (mouse/keyboard), StreamProcessor (LLM/STT/HITL), Renderer (compositor) | Refactor ~300 | Elimina god object |
| **P0-3** | Substituir IrqSafeLock por AtomicU64/AtomicBool onde possível (MOUSE_X/Y, dirty flags, POWER_STATE) | ~50 | Reduz lock contention |

### Fase 2: Performance (hotpath)

| # | Ação | LOC estimado | Impacto |
|---|------|-------------|---------|
| **P1-1** | Pool de buffers fixos para draw_text (String pre-allocada no compositor) | +40 | Elimina allocs/frame |
| **P1-2** | Dirty-rect computation: calcular bounding box do que mudou e só blitar essa região | +80 | Reduz 70% do fill_rect |
| **P1-3** | NotificationQueue com cap duro (max 32) | +5 | Evita OOM |
| **P1-4** | ChatWindow com max_history (últimas 100 msgs) | +10 | Evita crescimento infinito |

### Fase 3: Funcionalidade

| # | Ação | LOC estimado | Impacto |
|---|------|-------------|---------|
| **P2-1** | Testes para EmotionAnalysis com holdout de 20 frases PT-BR | +80 | Valida classifier |
| **P2-2** | mesh_health_json com fallback (não-panic em byte inválido) | +20 | Robustez |
| **P2-3** | PT-BR fonemas extras no TTS formant (ã, õ, lh, nh) | +40 | Cobertura PT-BR |
| **P2-4** | Snapshot de GaugeSnapshot (CPU/MEM) como static Atomics (não lock) | +30 | Performance HUD |

### Fase 4: Polimento

| # | Ação | LOC estimado | Impacto |
|---|------|-------------|---------|
| **P3-1** | Theme caching (evita current_theme() por frame) | +10 | Performance |
| **P3-2** | Window animation via lerp (não recompute a cada tick) | +30 | Visual suave |
| **P3-3** | UVC driver: fallback para canvas cinza (não crash sem HW) | +15 | Robustez |
| **P3-4** | Screensaver: animate particles (hoje stub de 30 LOC) | +40 | Visual idle |

---

## 🎯 Comparativo com Redox OS (Orbital)

| Feature | Redox Orbital | Jarbas | Gap |
|---------|--------------|--------|-----|
| Testes host | ~200 | **0** | ❌ |
| Dirty-rects | Sim | Sim | ✅ |
| Per-window backbuffer | Sim | **Não** | ❌ |
| WM com i3/Sway tiling | Sim | Sim (COSMIC) | ✅ |
| Font scaling | TTF/OTF | Bitmap 8x16 | ❌ |
| Clipboard manager | Sim | Stub | ❌ |
| Multi-monitor | Sim | **Não** | ❌ |
| Accessibility (a11y) | Parcial | **Não** | ❌ |
| Themed theming | CSS-like | Static const | ⚠️ |
| IPC viaRPC | Message-passing | EventBus (polling) | ⚠️ |

---

## 🔧 Quick Wins (implementáveis agora)

1. **`#[cfg(test)] mod tests` em jarvis.rs** — 15 testes para EmotionAnalysis, SoulProfile, SessionHistory
2. **`static THEME_CACHE: AtomicPtr<Theme>`** — evita Mutex lock por frame
3. **`NotificationQueue.max = 32`** — uma linha, evita OOM
4. **`mesh_health_json::parse` com bounds check** — evita panic em JSON malformado
5. **`GaugeSnapshot` via atomics** — elimina Mutex no sample_cpu/sample_mem

---

## 📊 Resumo

| Métrica | Valor | Meta |
|---------|-------|------|
| LOC total | 13,288 | — |
| Testes host | **0** | ≥50 |
| God objects | 1 (DisplayAgent) | 0 |
| Locks no hotpath | 129 | <30 |
| Alocações/frame | ~282 | <50 |
| Dead code | ~340 LOC | 0 |
| TODOs | 1 | 0 |

**Prioridade imediata:** P0-1 (testes) + P0-3 (atomics) + P1-1 (buffer pool)
