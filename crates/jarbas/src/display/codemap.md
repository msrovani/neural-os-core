# crates/jarbas/src/display/ — Display FE: framebuffer + compositor + cards

**Responsibility**: BGRA32 framebuffer (`fb.rs`: `DoubleBuffer` heap back-buffer
+ `swap()`, `GpuDevice`/`GPU` static, `probe_raw_framebuffer`, `fb_remap_uc`,
`claim_graphics`), layered compositor/WM (`compositor.rs`: `JarbasDesktop`,
`Layer` Z-order OrbBackground < HermesOverlay < AppWindows < DockBar, retained
`WindowContent::Card` windows, power dialog, dock/workspaces/tiling/focus),
declarative cards (`card.rs`: `UiDeclaration`/`Widget`, `parse_card`,
`render_card`, `card_json_schema_hint`; `eg.rs`: `FbTarget` =
`embedded-graphics::DrawTarget` over `DoubleBuffer`), avatar/orb (`avatar.rs`,
`avatar8.rs`), HUD gauges, notifications, chat console, shortcuts, TTF engine.

**Key symbols**: `fb::{DoubleBuffer, GpuDevice, GPU, probe_raw_framebuffer}`;
`compositor::{COMPOSITOR, JarbasDesktop, spawn_card, card_click, Layer,
MOUSE_X/Y/BUTTONS, POWER_STATE}`; `card::{UiDeclaration, Widget, parse_card,
render_card}`; `eg::FbTarget`; `agent::DisplayAgent` (EventBus-driven
Continuous agent that builds the desktop on first tick and renders each tick).

**Integration**: bin calls `probe_raw_framebuffer` (limine_boot.rs) and
`fb_remap_uc` (main.rs:2537), registers `DisplayAgent` (main.rs:2540); cards
are spawned from `UI_SPEC` JSON (LLM/skill), clicks return
`"close|resize|drag|btn|focus|miss"` for `CARD_ACTION` routing; SysInfoAgent
(bin) reads `COMPOSITOR` + card types directly.
