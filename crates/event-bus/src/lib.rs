#![no_std]
#![allow(dead_code)]
extern crate alloc;

pub mod bus;
pub mod capability;
pub mod channel;
pub mod event;
pub mod latent;
pub mod message_bus;

pub use bus::{EventBus, Receiver};
pub use capability::CapabilityToken;
pub use channel::BoundedChannel;
pub use event::Event;
pub use latent::{LatentBus, LatentPacket, LatentReceiver, LATENT_DIM, TOPIC_THOUGHT_LLM};
pub use message_bus::{self_test as message_bus_self_test, AgentId, Envelope, MessageBus};

