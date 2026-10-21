//! Parsers e contratos puros para NixAIOS (std).
//! Evita dual-truth com bare-metal: golden tests cobrem GGUF magic e CapGate bits.

#![forbid(unsafe_code)]

pub mod cap;
pub mod gguf;
pub mod model_slot;
pub mod score;

pub use cap::{CapBits, CapGate};
pub use gguf::{GgufHeaderInfo, parse_gguf_header};
pub use model_slot::ModelSlot;
pub use score::{NosScore, Sev};
