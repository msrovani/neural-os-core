//! ComputePort — FE R2/R3 pede; BE R1 executa (ADR-0041 H2 → gpu::backend).

use crate::device_cap::DeviceId;
use crate::gpu::compute_abi::BackendState;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    NotBound,
    Bound,
    Ready,
    Quarantine,
    CpuFallback,
}

#[derive(Debug, Clone, Copy)]
pub struct ComputeStatus {
    pub status: PortStatus,
    pub device: Option<DeviceId>,
}

static COMPUTE_STATUS: Mutex<ComputeStatus> = Mutex::new(ComputeStatus {
    status: PortStatus::NotBound,
    device: None,
});

pub fn status() -> ComputeStatus {
    *COMPUTE_STATUS.lock()
}

pub fn set_status(s: ComputeStatus) {
    *COMPUTE_STATUS.lock() = s;
}

/// Sincroniza porta com estado real do backend GPU (pós-canário).
pub fn sync_from_backend() {
    let be = crate::gpu::backend::compute_state();
    let status = match be {
        BackendState::Ready => PortStatus::Ready,
        BackendState::Quarantine => PortStatus::Quarantine,
        BackendState::CpuOnly => PortStatus::CpuFallback,
        BackendState::Probed | BackendState::BringingUp => PortStatus::Bound,
    };
    set_status(ComputeStatus {
        status,
        device: None,
    });
}

/// Submit FE — Cap FeCompute (H5+); Ready só após golden; senão CPU fallback honesto.
pub fn submit_vector_add_stub() -> PortStatus {
    use crate::cap_gate::{self, CapResult, HalCap};
    if cap_gate::check_fe_bound(HalCap::FeCompute) == CapResult::Deny {
        set_status(ComputeStatus {
            status: PortStatus::Quarantine,
            device: None,
        });
        return PortStatus::Quarantine;
    }
    let st = status().status;
    if st == PortStatus::NotBound {
        PortStatus::NotBound
    } else if st == PortStatus::Ready {
        PortStatus::Ready
    } else {
        PortStatus::CpuFallback
    }
}
