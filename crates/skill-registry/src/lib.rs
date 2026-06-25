#![no_std]
extern crate alloc;

pub mod mcp;
pub mod registry;
pub mod skill;

pub use mcp::McpManifest;
pub use registry::{SkillRegistry, ToolPolicy};
pub use skill::Skill;
