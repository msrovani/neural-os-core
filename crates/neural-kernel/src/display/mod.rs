//! Display subsystem — framebuffer + embedded-graphics + neural console.
//!
//! ## Architecture
//! - `Framebuffer` — raw BGRA32 pixel writer, implements embedded_graphics::DrawTarget
//! - `ConsoleRegion` — multi-region text layout sobre o framebuffer
//! - `DisplayAgent` — agent que subscreve HERMES_RESPONSE e renderiza o console
//!
//! O framebuffer é obtido via BootInfo::framebuffer (já mapeado pelo bootloader).
//! O QEMU padrão usa resolução 1024x768, BGRA32 (4 bytes/pixel).

pub mod fb;
pub mod console;
pub mod font;
pub mod agent;
pub mod theme;
pub mod compositor;
pub mod icons;
