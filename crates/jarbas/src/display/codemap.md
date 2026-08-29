# crates/jarbas/src/display/ — Display FE: framebuffer + compositor + cards

**Responsibility**: BGRA32 framebuffer (`fb.rs`: `DoubleBuffer` heap back-buffer
+ `swap()`, `GpuDevice`/`GPU` static, `probe_raw_framebuffer`, `fb_remap_uc`,
`claim_graphics`), layered compositor/WM (`compositor.rs`: `JarbasDesktop`,
`Layer` Z-order OrbBackground < HermesOverlay < AppWindows < DockBar, retained
`WindowContent::Card` windows, power dialog, dock/workspaces/tiling/focus),
declarative cards (`card.rs`: `UiDeclaration`/`Widget`, `parse_card`,
`render_card`, `card_json_schema_hint`; `eg.rs`: `FbTarget` =
`embedded-graphics::DrawTarget` over `DoubleBuffer`), overlays retidos
(`overlay.rs`: `EMBED_MARKS`, `RENDER_OVERLAYS` — tick grava, `render()` pinta),
avatar/orb (`avatar.rs`, `avatar8.rs`), HUD gauges (snapshot MetricsAgent;
compositor NÃO chama `draw_status_gauges` — HUD SESSION_273), notifications
(`NotificationQueue` no `render()`), chat console, shortcuts, TTF engine.

**Key symbols**: `fb::{DoubleBuffer, GpuDevice, GPU, probe_raw_framebuffer}`;
`compositor::{COMPOSITOR, JarbasDesktop, spawn_card, card_click,
handle_desktop_click, show_app, add_window_floating, paint_overlays, Layer,
MOUSE_X/Y/BUTTONS, POWER_STATE, TOPIC_CARD_ACTION}`;
`overlay::{EMBED_MARKS, RENDER_OVERLAYS, push_embed, set_render_overlay}`;
`card::{UiDeclaration, Widget, parse_card, render_card}`; `eg::FbTarget`;
`agent::DisplayAgent` (EventBus-driven Continuous; `has_pending`; HITL card 8001).

**Hot path (SESSION_294):** `fill_rect` = `fill_rect_fast` (bpp=4 `0..aw`, doubling memcpy se aw≥16);
`fill_circle_glow` scanline+`isqrt_u64` (não `sqrtf`/pixel); `TARGET_FRAME_TICKS=1` (PIT ~18 Hz);
orb ambient 1.35× max 2 rings; dock pinta uma vez. Grid/partículas saíram do frame.

**Integration**: bin calls `probe_raw_framebuffer` (limine_boot.rs) and
`fb_remap_uc` (main.rs:2537), registers `DisplayAgent` (main.rs:2540); cards
are spawned from `UI_SPEC` JSON (LLM/skill); hit-test canónico dock → cards →
janelas (orb/mesh = miss); clicks return `"close|resize|drag|btn|focus|miss|dock:*"`
e botão publica `CARD_ACTION` (`card_id:idx`); SysInfoAgent (bin) reads
`COMPOSITOR` + card types directly. HDA capture/playback: `k_nano::audio::hda`
(IRQ 0x30); k_hal = facade.
