#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
extern crate alloc;

pub mod bus;
pub mod capability;
pub mod channel;
pub mod event;
pub mod latent;
pub mod message_bus;
pub mod stamp;

pub use bus::{
    clone_churn_snapshot, EventBus, Receiver, DEFAULT_QUEUE_DEPTH, STREAM_QUEUE_DEPTH,
};
pub use capability::CapabilityToken;
pub use channel::BoundedChannel;
pub use event::Event;
pub use latent::{LatentBus, LatentPacket, LatentReceiver, LATENT_DIM, TOPIC_THOUGHT_LLM};
pub use message_bus::{self_test as message_bus_self_test, AgentId, Envelope, MessageBus};
pub use stamp::{
    build_stamp, fnv1a64_pad32, verify_stamps, HashFn, PublisherFn, Stamp, StampRing,
    HASH_PREFIX_MAX, STAMP_RING_CAP,
};
