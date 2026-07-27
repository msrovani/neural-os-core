//! Cross-OS Ecosystem — descoberta inteligente e execucao de skills.
//! AIOS na veia: pesquisa em runtime, aprende com uso, evolui sozinho.

pub mod agent;
pub mod intent;
pub mod discoverer;

pub use agent::CrossOsAgent;
pub use intent::{CrossOsIntent, IntentCategory, IntentResult};
pub use discoverer::{CrossOsDiscoverer, SkillCandidate, SkillSource, SkillFormat, DiscoverResult};
