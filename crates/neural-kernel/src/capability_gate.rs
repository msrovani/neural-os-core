//! Facade — logic lives in k_hal::cap_gate (R1) + k_nano::paging (R0 Cap).
//! ADR-0041 §11: k_nano R0 = paging/TSS/IST, k_hal R1 = CapGate/HalOffer with int 0x90.
pub use k_hal::cap_gate::{
    allow_count, check, deny_count, demo_hermes_caps, host_send_tcp, host_write_ring,
    required_cap, Cap, HOST_FN_DEMAND_PAGE, HOST_FN_MAP_DMA, HOST_FN_MAP_FB,
    HOST_FN_MAP_FILE, HOST_FN_MAP_WEIGHTS, HOST_FN_PIN_DMA, HOST_FN_PRESENT_FB,
    HOST_FN_SEND_TCP, HOST_FN_VRING_SETUP, HOST_FN_WRITE_RING,
};
