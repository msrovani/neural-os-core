use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub enum OutputSchema {
    Any,
    String,
    Json(Vec<String>),
}

impl OutputSchema {
    pub fn validate(&self, output: &[u8]) -> bool {
        match self {
            OutputSchema::Any => true,
            OutputSchema::String => core::str::from_utf8(output).is_ok(),
            OutputSchema::Json(keys) => {
                let s = match core::str::from_utf8(output) { Ok(s) => s, _ => return false };
                keys.iter().all(|k| s.contains(k.as_str()))
            }
        }
    }
}

pub struct McpManifest {
    pub name: String,
    pub description: String,
    pub required_tokens: Vec<u64>,
    /// Caminhos VFS para carregar antes de executar (JobPreconditions)
    pub preconditions: Vec<String>,
    /// Skills relacionadas para composicao
    pub context_links: Vec<String>,
    /// Schema de output esperado (para validacao e composicao)
    pub output_schema: OutputSchema,
    /// Se true, output pode ser cacheado
    pub idempotent: bool,
}
