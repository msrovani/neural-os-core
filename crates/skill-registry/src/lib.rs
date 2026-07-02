#![no_std]
extern crate alloc;

pub mod mcp;
pub mod registry;
pub mod skill;
pub mod cache;
pub mod index;

pub use mcp::{McpManifest, OutputSchema};
pub use registry::{SkillRegistry, ToolPolicy};
pub use skill::Skill;
pub use cache::OutputCache;
pub use index::SkillIndex;
