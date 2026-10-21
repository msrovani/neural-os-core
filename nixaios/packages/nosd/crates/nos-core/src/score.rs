//! NOSD_SCORE — sev contract (ADAPT ADR-0092).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sev {
    Ok,
    Warn,
    Fail,
    Trace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NosScore {
    pub phase: String,
    pub sev: Sev,
    pub msg: String,
    pub hub_slots: u8,
    pub memory_ok: bool,
    pub mesh_peers: u32,
}

impl NosScore {
    pub fn line(&self) -> String {
        format!(
            "NOSD_SCORE phase={} sev={:?} hub={} mem={} peers={} | {}",
            self.phase,
            self.sev,
            self.hub_slots,
            self.memory_ok as u8,
            self.mesh_peers,
            self.msg
        )
    }
}
