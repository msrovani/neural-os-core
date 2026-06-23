use alloc::string::String;
use alloc::vec::Vec;

pub struct McpManifest {
    pub name: String,
    pub description: String,
    pub required_tokens: Vec<u64>,
}
