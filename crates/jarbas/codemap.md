# crates/jarbas/ — JARVIS Ring 3: Display FE + Persona + UI Cards

Ring 3 (R3) user-facing layer of the K³CHJ workspace. Depends on k_nano (R0),
k_hal (R1), cortex, hermes. `no_std`, ~62 `.rs` files, 12 top-level modules
(see `src/lib.rs`). Wired into the bin as `jarbas-crate` (alias, `package = "jarbas"`).

## Responsibility

- **Display frontend**: BGRA32 framebuffer (`DoubleBuffer`), layered compositor
  (`JarbasDesktop`), window manager (tiling + floating + workspaces), JARVIS
  avatar/orb, HUD gauges, notifications, Hermes chat console.
- **UI cards**: declarative card desktop (ADR-0058) — `UiDeclaration` /
  `Widget` rendered via an `embedded-graphics` `DrawTarget` (`FbTarget`).
- **Persona**: JARVIS virtual assistant (SOUL/PERSONA.md profile, emotion
  model, greeting via LLM), plus `DisplayAgent` — the `agent-core::Agent`
  that drives the desktop from the EventBus.
- **Audio & GPU FE**: voice pipeline mirror and GPU front-end (GPU MMIO
  backend lives in k_hal; jarbas only adds the desktop-cube demo).

## Design Patterns

- **DoubleBuffer + compositor**: every draw primitive (`set_pixel`,
  `fill_rect`, `fill_circle_glow`, `draw_char`, text) writes to a heap
  `Vec<u8>` back buffer; `DoubleBuffer::swap()` does a single chunked (u64)
  copy to the physical FB — tear-free, ~1M bus writes @1080p. All consumers
  read geometry from `GpuDevice`, never hardcode bpp/stride.
- **Layered Z-order**: `Layer` enum `OrbBackground < HermesOverlay <
  AppWindows < DockBar`; `JarbasDesktop.render()` paints in fixed order:
  power-state overlay, background grid + orb + avatar, status bar (28px),
  Hermes right panel (35% width, translucency), workspace tiled/floating
  windows, notifications + focus indicator + power dialog, vconsole, mouse
  cursor — then `swap()`.
- **embedded-graphics DrawTarget over fb**: `FbTarget<'a>` (display/eg.rs) is
  the *only* seam to the embedded-graphics ecosystem; `Rgb888` pixels route
  through `DoubleBuffer::set_pixel` (channel order handled by `rgb_order`).
  Orb/dock/HUD stay on native primitives; only cards draw through eg.
- **Declarative UI**: cards are pure data — `UiDeclaration { id, title, x, y,
  w, h, body: Vec<Widget> }` with `Widget::Text/KeyValue/Gauge/Bars/List/
  Divider/Button/Panel`. `parse_card()` is a minimal no_std JSON parser;
  `card_json_schema_hint()` constrains the LLM's structured decoding (ADR-0057
  #412) to emit valid cards. Gauge/bars values are integers — the soft-float
  kernel disables SSE.
- **CardWindow retention in compositor**: cards live as `WindowContent::Card`
  in `JarbasDesktop.windows` (plus the workspace `floating_windows` list), so
  they persist across frames, focus cycles and workspace switches — the
  compositor re-renders retained geometry each tick instead of re-spawning.

## Data and Control Flow

- **Card render**: Hermes/Cortex or a WASM skill publishes JSON on `UI_SPEC`
  → `DisplayAgent::apply_ui_spec()` detects `"body"` → `card::parse_card()`
  → `desktop.spawn_card(decl)` (pushes `WindowContent::Card` into `windows` +
  floating list) → next `render()` tick draws it via `render_card()` /
  `FbTarget`/embedded-graphics and returns `ButtonHit` rects.
- **Display tick** (`DisplayAgent::tick`, Continuous): first tick builds
  `DoubleBuffer::from_gpu(&GPU)` + `Avatar8` → `JarbasDesktop::new` →
  `*COMPOSITOR = Some(desktop)` → `claim_graphics()`; subsequent ticks drain
  EventBus receivers (HERMES_RESPONSE, LLM_STREAM, USER_INTENT, STT_TEXT,
  HITL, TOAST, RENDER_*, KEY_EVENT), poll mouse, then
  `desktop.render(tick, avatar, state)` (FPS-gated by `TARGET_FRAME_TICKS`).
- **Mouse flow**: PS/2 IRQ updates `k_nano::interrupts::MOUSE_ABS_*` → tick
  detects button edge (`MOUSE_ABS_BTN ^ prev`) → `handle_pointer_click()`
  (power dialog, OFF button, notification hit-test, chat/ambient focus) →
  card hit-testing via `desktop.card_click(cx, cy)` returns
  `"close" | "resize" | "drag" | "btn" | "focus" | "miss"`; button hits are
  stored in `card_hit_button: Option<(u32, usize)>` for the subsequent
  `CARD_ACTION` EventBus event; `card_drag_step`/`card_resize_step` move
  retained card geometry while button held.
- **Keyboard flow**: `KEY_EVENT` payload `[scancode, ctrl, alt, shift,
  super_key, pressed]` → `dispatch_key_event()` → `WmAction` (workspaces,
  focus, tiling, app launch, power menu, help card).

## Integration

- **Consumers** — `neural-kernel` bin: `pub use jarbas_crate::{display, gpu,
  jarvis, uvc_driver, virtio_gpu, vision_agent}` (main.rs:100); registers
  `display::agent::DisplayAgent` (main.rs:878/2540, boot_ckpt 41→42); calls
  `display::fb::fb_remap_uc()` (main.rs:2537) and
  `display::fb::probe_raw_framebuffer()` (limine_boot.rs:155); `SysInfoAgent`
  (bin) consumes `jarbas_crate::display::card::{UiDeclaration, Widget}` +
  `compositor::COMPOSITOR`; `bei_init` uses `display::soul_mirror::SoulMirrorState`.
- **Key public exports**:
  - `display::fb::{GpuDevice, GPU, DoubleBuffer, FramebufferInfo,
    probe_raw_framebuffer, fb_remap_uc, claim_graphics, paint_tts_response}` —
    note: `probe_uefi_framebuffer` was **removed** in the Limine migration
    (SESSION_232); FB geometry now comes from the bootloader/Limine via
    `probe_raw_framebuffer` → `GpuDevice::from_probe`. **bpp/stride**: always
    `GpuDevice::bytes_per_pixel()`/`stride_bytes()` (fed from GOP
    `info.bytes_per_pixel`); never infer bpp from `PixelFormat` (Bgr/Rgb ≠
    24-bit, SESSION_139 lesson). `resolve_bytes_per_pixel` accepts 3|4,
    falls back to 4.
  - `display::compositor::{COMPOSITOR, JarbasDesktop, Layer, spawn_card,
    card_click, MOUSE_X/Y/BUTTONS, POWER_STATE}`.
  - `display::card::{UiDeclaration, Widget, parse_card, render_card,
    card_json_schema_hint, demo_*_card}`; `display::eg::{FbTarget, self_test}`.
  - `display::agent::DisplayAgent` (the agent wiring).
  - `jarvis::{SoulProfile, Emotion, default_jarbas, load_from_vfs}`.
- **Audio mirror note**: `src/audio/` is the voice pipeline (VAD → wakeword →
  STT → USER_INTENT, HERMES_RESPONSE → Piper/formant TTS → mixer). Per
  ADR-0045 the historical truth was `neural-kernel/src/audio`, but since the
  E4 "emagrecer" refactor the bin's `audio` module is only a facade
  (`pub use jarbas_crate::audio::*;`, neural-kernel/src/audio/mod.rs) — jarbas
  is now the **single source**; the bin wires its agents
  (`audio::voice::JarbasVoiceAgent`, `audio::jarvis::JarbasAgent`, etc.) at
  main.rs:2562–2577.

## Submodule Map

| Submodule | Responsibility |
|---|---|
| `src/audio/` | JARVIS voice pipeline (ADR-0045): VAD, wake-word, STT (CTC), Piper + formant TTS, SER, UAC USB, mixer, skills; agents `JarbasVoiceAgent`/`JarbasAgent`/`AudioPipelineAgent`. |
| `src/cards/` | Data-only `UiDeclaration` builders (ADR-0040 #419, ADR-0079): storage card (reads `k_nano::ATA_DRIVER`), install progress card. |
| `src/display/` | Framebuffer + DoubleBuffer + compositor/WM + cards + avatar/orb + HUD + chat console + shortcuts (largest submodule, 30 files). |
| `src/gpu/` | GPU FE: re-exports `k_hal::gpu::*` (MMIO BE in k_hal, ADR-0041 H2) + `cube` (workspace crossfade demo on DoubleBuffer). |
| `src/*.rs` (top) | `jarvis` persona (SoulProfile/Emotion), `vconsole` (6 virtual consoles), `ide` (BitNet IDE), `image_viewer`, `screensaver`, `uvc_driver` + `virtio_gpu` + `vision_agent` (camera FE via HalOffer), `clipboard_notify` (toast bridge). |
