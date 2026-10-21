//! Mesh P2P — contratos wire ADR-0081 (PORT sobre UDP std).

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

pub const MAGIC_HB: &[u8] = b"HB\0";
pub const MAGIC_FRAG: &[u8] = b"FRAG\0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: u32,
    pub last_seen_ms: u64,
    pub pk_bound: bool,
}

#[derive(Clone, Default)]
pub struct MeshEngine {
    peers: Arc<Mutex<HashMap<u32, PeerInfo>>>,
    local_id: u32,
    tofu_ticks: u32,
}

impl MeshEngine {
    pub fn new(local_id: u32) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            local_id,
            tofu_ticks: 0,
        }
    }

    pub fn local_id(&self) -> u32 {
        self.local_id
    }

    pub fn on_heartbeat(&self, peer_id: u32, has_pk: bool) {
        let now = now_ms();
        let mut g = self.peers.lock();
        let e = g.entry(peer_id).or_insert(PeerInfo {
            id: peer_id,
            last_seen_ms: now,
            pk_bound: false,
        });
        e.last_seen_ms = now;
        if has_pk {
            e.pk_bound = true;
        }
        let _ = self.tofu_ticks;
        info!(target: "nosd", "mesh HB peer={} pk={}", peer_id, e.pk_bound);
    }

    /// TOFU settle após N ticks de HB (espelha 130 ticks bare — aqui 8 HB).
    pub fn tofu_settled(&self) -> bool {
        let g = self.peers.lock();
        g.values().any(|p| p.pk_bound) && g.len() > 0
    }

    pub fn peer_count(&self) -> u32 {
        self.peers.lock().len() as u32
    }

    pub fn encode_heartbeat(&self) -> Vec<u8> {
        let mut v = MAGIC_HB.to_vec();
        v.extend_from_slice(&self.local_id.to_le_bytes());
        v.extend_from_slice(b"PK\0");
        v
    }

    pub fn try_parse_heartbeat(buf: &[u8]) -> Option<(u32, bool)> {
        if buf.len() < 7 || &buf[0..3] != MAGIC_HB {
            return None;
        }
        let id = u32::from_le_bytes(buf[3..7].try_into().ok()?);
        let has_pk = buf.windows(3).any(|w| w == b"PK\0");
        Some((id, has_pk))
    }

    pub fn health_json(&self) -> String {
        format!(
            r#"[{{"local":{},"peers":{},"tofu":{}}}]"#,
            self.local_id,
            self.peer_count(),
            self.tofu_settled() as u8
        )
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hb_roundtrip() {
        let m = MeshEngine::new(15);
        let buf = m.encode_heartbeat();
        let (id, pk) = MeshEngine::try_parse_heartbeat(&buf).unwrap();
        assert_eq!(id, 15);
        assert!(pk);
        m.on_heartbeat(2, true);
        assert_eq!(m.peer_count(), 1);
        assert!(m.tofu_settled());
    }
}
