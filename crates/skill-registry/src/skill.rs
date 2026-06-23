use alloc::vec::Vec;

use crate::mcp::McpManifest;

pub trait Skill: Send + Sync {
    fn manifest(&self) -> McpManifest;
    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str>;
}
