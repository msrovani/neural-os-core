#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
extern crate alloc;

pub mod mcp;
pub mod registry;
pub mod skill;
pub mod cache;
pub mod index;
pub mod task;
pub mod contract;
pub mod dynskill;
pub mod fanout;

pub use mcp::{McpManifest, OutputSchema};
pub use registry::{SkillRegistry, ToolPolicy};
pub use skill::Skill;
pub use cache::OutputCache;
pub use index::{SkillIndex, McpCatalog, CatalogEntry};
pub use task::{JobPreconditions, TaskSchema, TaskStatus};
pub use contract::{CompletionContract, ContractAction, ValidationFn, CONTRACT_NONEMPTY, CONTRACT_UTF8};
pub use dynskill::DynamicSkill;
pub use fanout::FanOutPool;
