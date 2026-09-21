#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod mcp;
pub mod registry;
pub mod skill;
pub mod cache;
pub mod contract;
pub mod dynskill;

pub use mcp::{McpManifest, OutputSchema};
pub use registry::{SkillListEntry, SkillRegistry, ToolPolicy};
pub use skill::Skill;
pub use cache::OutputCache;
pub use contract::{
    CompletionContract, ContractAction, ValidationFn, CONTRACT_NONEMPTY, CONTRACT_UTF8,
};
pub use dynskill::{DynamicSkill, DYNSKILL_TOKEN};
