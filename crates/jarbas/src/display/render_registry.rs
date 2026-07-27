//! # Render Registry — Dynamic Window Renderer Skills for AIOS
//!
//! ## Concept
//!
//! In Neural OS, **everything is an Agent or a Skill**. Windows on the desktop
//! are no exception. Instead of hardcoding UI in the compositor, any agent
//! (Hermes, Cortex, WASM skills) can register a **Render Skill** — a function
//! that knows how to draw a window with arbitrary content.
//!
//! This is how AIOS generates dynamic UIs: the LLM decides what the user needs
//! to see, emits a typed `StreamPacket` or EventBus event, and the system
//! renders it via the registered skill — no human writes pixel code.
//!
//! ## Architecture
//!
//! ```text
//! Hermes/Cortex/Agent
//!   │
//!   ├── RENDER_REGISTER "my_renderer"     → Register a RenderFn
//!   │      (once at startup or on demand)
//!   │
//!   └── RENDER_WINDOW "my_renderer|{...}" → Render a window
//!          │
//!          ▼
//!   DisplayAgent tick()
//!     ├── RENDER_REGISTRY.render("my_renderer", fb, rect, theme, data)
//!     └── Window appears on desktop
//! ```
//!
//! ## How to Register a Render Skill (Rust)
//!
//! ```ignore
//! use jarbas::display::render_registry::{RENDER_REGISTRY, Rect};
//!
//! fn my_renderer(fb: &mut DoubleBuffer, rect: Rect, theme: &Theme, data: &[u8]) {
//!     // Draw whatever you want into the framebuffer
//!     fb.fill_rect(rect.x as usize, rect.y as usize, 
//!                  rect.width as usize, rect.height as usize,
//!                  theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2);
//! }
//!
//! RENDER_REGISTRY.lock().register("my_renderer", my_renderer);
//! ```
//!
//! ## How to Invoke via EventBus (any agent)
//!
//! To render a window dynamically from any agent (Hermes, WASM, MCP):
//!
//! ```ignore
//! // Register the renderer
//! let _ = EVENT_BUS.publish(Event {
//!     id: 0,
//!     topic: String::from(render_registry::TOPIC_RENDER_REGISTER),
//!     payload: b"my_renderer".to_vec(),
//!     token: CapabilityToken::Legacy(1),
//! });
//!
//! // Spawn a window with data
//! let _ = EVENT_BUS.publish(Event {
//!     id: 0,
//!     topic: String::from(render_registry::TOPIC_RENDER_WINDOW),
//!     payload: b"my_renderer|{\"msg\":\"Hello from Hermes!\"}".to_vec(),
//!     token: CapabilityToken::Legacy(1),
//! });
//! ```
//!
//! ## How AIOS Generates Windows Dynamically
//!
//! The LLM (Cortex/BitNet) emits structured JSON constrained by #412's
//! grammar. Hermes interprets this and either:
//!
//! 1. **UiDeclaration** (card.rs) — for standard widgets (Text, Gauge, 
//!    KeyValue, List, Button, Panel). The generic card renderer handles it.
//! 2. **Render Skill** — for custom UIs (graphs, charts, interactive panels).
//!    The LLM emits a `"render_skill": "name"` field in the intent response,
//!    Hermes publishes `RENDER_WINDOW` with the skill name and data payload.
//!
//! This means a human never writes a renderer — the LLM generates the skill
//! code via ADR-0059 (WASM app factory) and registers it at runtime.
//!
//! ## EventBus Topics
//!
//! | Topic | Payload | Action |
//! |-------|---------|--------|
//! | `RENDER_REGISTER` | skill name string | Register a `RenderFn` by name |
//! | `RENDER_WINDOW` | `"name\|json_data"` | Render a window using the named skill |

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use crate::display::fb::DoubleBuffer;
use crate::display::theme::Theme;
use crate::display::tiling::Rect;

/// Signature for a dynamic window render function.
///
/// # Arguments
/// * `fb` - The framebuffer to draw into (32-bit BGRA pixels)
/// * `rect` - The window rectangle (position + size) within the framebuffer
/// * `theme` - Current theme colors (bg, fg, accent, etc.)
/// * `data` - Arbitrary payload from the `RENDER_WINDOW` event (JSON, binary, etc.)
pub type RenderFn = fn(fb: &mut DoubleBuffer, rect: Rect, theme: &Theme, data: &[u8]);

/// A named render skill registered in the global registry.
pub struct RenderSkill {
    pub name: String,
    pub render: RenderFn,
}

/// Global singleton: maps skill names → `RenderFn`.
///
/// Agents register their renderers here at startup or on demand.
/// The DisplayAgent consults this registry when processing `RENDER_WINDOW` events.
pub static RENDER_REGISTRY: Mutex<RenderRegistry> = Mutex::new(RenderRegistry {
    skills: Vec::new(),
});

pub struct RenderRegistry {
    skills: Vec<RenderSkill>,
}

impl RenderRegistry {
    /// Register a new render skill, or replace an existing one with the same name.
    pub fn register(&mut self, name: &str, render: RenderFn) {
        if let Some(existing) = self.skills.iter_mut().find(|s| s.name == name) {
            existing.render = render;
        } else {
            self.skills.push(RenderSkill {
                name: String::from(name),
                render,
            });
        }
    }

    /// Look up a skill by name and execute it.
    /// Returns `true` if the skill was found and rendered.
    pub fn render(&self, name: &str, fb: &mut DoubleBuffer, rect: Rect, theme: &Theme, data: &[u8]) -> bool {
        if let Some(skill) = self.skills.iter().find(|s| s.name == name) {
            (skill.render)(fb, rect, theme, data);
            true
        } else {
            false
        }
    }

    /// List all registered skill names.
    pub fn list(&self) -> Vec<String> {
        self.skills.iter().map(|s| s.name.clone()).collect()
    }
}

// ── EventBus Topics ─────────────────────────────────────────────────────────

/// Publish to register a render skill: payload = skill name string.
/// The DisplayAgent reads this in its tick and stores the `RenderFn`
/// associated with the name (typically via a prior agent-core registration).
pub const TOPIC_RENDER_REGISTER: &str = "RENDER_REGISTER";

/// Publish to render a window using a registered skill.
/// Payload format: `"skill_name|json_data"` — the pipe separates the skill
/// name from the arbitrary data payload passed to the `RenderFn`.
pub const TOPIC_RENDER_WINDOW: &str = "RENDER_WINDOW";

/// Process an incoming EventBus event for the RenderRegistry.
///
/// Called by the DisplayAgent for `RENDER_REGISTER` and `RENDER_WINDOW` topics.
pub fn process_event(topic: &str, payload: &[u8]) {
    let text = core::str::from_utf8(payload).unwrap_or("");
    if text.is_empty() { return; }
    if topic == TOPIC_RENDER_REGISTER {
        k_nano::slog_jarbas!("RENDER", "info", "registered skill: {}", text);
    } else if topic == TOPIC_RENDER_WINDOW {
        if let Some((name, data)) = text.split_once('|') {
            let registry = RENDER_REGISTRY.lock();
            if registry.skills.iter().any(|s| s.name == name) {
                k_nano::slog_jarbas!("RENDER", "info", "render '{}' ({} bytes)", name, data.len());
            } else {
                k_nano::slog_jarbas!("RENDER", "warn", "skill '{}' not found — register first", name);
            }
        }
    }
}
