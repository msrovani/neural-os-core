use alloc::vec::Vec;

use crate::mcp::McpManifest;

pub trait Skill: Send + Sync {
    fn manifest(&self) -> McpManifest;

    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str>;

    /// Pre-flight verification: check preconditions before execution.
    /// Return Err(reason) if the skill cannot run safely.
    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }
}
