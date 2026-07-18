#![no_std]
#![allow(dead_code)]
extern crate alloc;

pub mod bus;
pub mod capability;
pub mod event;
pub mod latent;
pub mod dedup;

pub use bus::{EventBus, Receiver};
pub use capability::CapabilityToken;
pub use event::Event;
pub use latent::{LatentBus, LatentPacket, LatentReceiver, LATENT_DIM, TOPIC_THOUGHT_LLM};
