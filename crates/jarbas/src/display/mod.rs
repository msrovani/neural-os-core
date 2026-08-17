//! Display subsystem — framebuffer, Soul Mirror, cards, ChatWindow.
//!
//! Composição: orb (brand) + grafo mesh opcional + HUD mínimo + cards on-demand.
//! O painel Hermes Console foi removido (SESSION_261); SysInfo card idem.

pub mod fb;
pub mod eg;
pub mod card;
pub mod console;
pub mod font;
pub mod agent;
pub mod overlay;
pub mod theme;
pub mod compositor;
pub mod avatar;
pub mod avatar8;
pub mod soul_mirror;
pub mod ui_spec;
pub mod embed_viz;
pub mod gauges;
pub mod metrics_agent;

// FASE 1.1 — WM cosmic-like (ADR-0065)
pub mod decorations;
pub mod workspaces;
pub mod focus;
pub mod shortcuts;
pub mod dock;
pub mod window;
pub mod tiling;
pub mod notifications;
pub mod chat_window;
pub mod render_registry;

// GPU Backend Bridge (Phase 2 — k_hal GPU BE integration)
pub mod gpu_backend;
