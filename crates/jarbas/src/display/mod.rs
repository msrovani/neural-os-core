//! Display subsystem — framebuffer + embedded-graphics + Hermes Chat Console.
//!
//! ## Architecture
//! - `Framebuffer` — raw BGRA32 pixel writer
//! - `NeuralConsole` — Hermes Chat Console com histórico + input
//! - `DisplayAgent` — agent que subscreve HERMES_RESPONSE e renderiza o console
//!
//! O framebuffer é obtido via BootInfo::framebuffer (já mapeado pelo bootloader).
//! Interface simplificada (NousResearch-style, sem multi-window compositor).

pub mod fb;
pub mod console;
pub mod font;
pub mod agent;
pub mod theme;
pub mod compositor;
pub mod ttf_engine;
pub mod avatar;
pub mod ui_spec;
pub mod embed_viz;
pub mod gauges;
pub mod metrics_agent;
