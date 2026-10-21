//! ModelHub slots — espelho de cortex::model_hub::ModelSlot (ADR-0078).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ModelSlot {
    Active = 0,
    Vision = 1,
    GeneratorPro = 2,
    Reranker = 3,
    RustCoder = 4,
    HwExpert = 5,
    Learner = 6,
    Agent = 7,
}

impl ModelSlot {
    pub const ALL: [ModelSlot; 8] = [
        Self::Active,
        Self::Vision,
        Self::GeneratorPro,
        Self::Reranker,
        Self::RustCoder,
        Self::HwExpert,
        Self::Learner,
        Self::Agent,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Vision => "vision",
            Self::GeneratorPro => "generator_pro",
            Self::Reranker => "reranker",
            Self::RustCoder => "rust_coder",
            Self::HwExpert => "hw_expert",
            Self::Learner => "learner",
            Self::Agent => "agent",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.name() == s)
    }
}
