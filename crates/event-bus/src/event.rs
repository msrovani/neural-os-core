use alloc::string::String;
use alloc::vec::Vec;
use crate::capability::CapabilityToken;

#[derive(Debug, Clone)]
pub struct Event {
    pub id: u64,
    pub topic: String,
    pub payload: Vec<u8>,
    pub token: CapabilityToken,
}
