//! Cortex std: ModelHub, scanner, capacity, HotSwap, distrib assign.

#![forbid(unsafe_code)]

pub mod capacity;
pub mod distrib;
pub mod header_scan;
pub mod hotswap;
pub mod hub;
pub mod offload;
pub mod route;

pub use capacity::{CapacityProber, NodeCapacity};
pub use distrib::{PeerCapacity, capacity_weighted_assign};
pub use header_scan::{ModelMeta, scan_path};
pub use hotswap::{
    EngineKind, HotSwapController, InferenceBackend, LlamaCliBackend, SwapError, find_llama_cli,
};
pub use hub::ModelHub;
pub use offload::plan_n_gpu_layers;
pub use route::{RouteDecision, SystemLane, route_intent};
