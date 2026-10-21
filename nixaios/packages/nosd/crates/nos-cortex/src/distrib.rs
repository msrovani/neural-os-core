//! Capacity-weighted peer assign (PORT mesh_distrib / ADR-0081).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapacity {
    pub peer_id: u32,
    pub vram_free_mb: u64,
    pub ram_free_mb: u64,
    pub unreachable: bool,
    /// RTT factor 1.0 = baseline; higher = worse.
    pub latency_factor: f32,
}

/// Escolhe peer com maior score; retorna None se ninguém útil.
pub fn capacity_weighted_assign(peers: &[PeerCapacity]) -> Option<u32> {
    let mut best: Option<(u32, f32)> = None;
    for p in peers {
        if p.unreachable {
            continue;
        }
        let mem = (p.vram_free_mb as f32) * 2.0 + (p.ram_free_mb as f32) * 0.1;
        let lat = p.latency_factor.max(0.25);
        let score = mem / lat;
        match best {
            None => best = Some((p.peer_id, score)),
            Some((_, s)) if score > s => best = Some((p.peer_id, score)),
            _ => {}
        }
    }
    best.map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_vram() {
        let peers = [
            PeerCapacity {
                peer_id: 1,
                vram_free_mb: 100,
                ram_free_mb: 8000,
                unreachable: false,
                latency_factor: 1.0,
            },
            PeerCapacity {
                peer_id: 2,
                vram_free_mb: 8000,
                ram_free_mb: 1000,
                unreachable: false,
                latency_factor: 1.0,
            },
        ];
        assert_eq!(capacity_weighted_assign(&peers), Some(2));
    }

    #[test]
    fn skips_unreachable() {
        let peers = [PeerCapacity {
            peer_id: 1,
            vram_free_mb: 9999,
            ram_free_mb: 9999,
            unreachable: true,
            latency_factor: 1.0,
        }];
        assert_eq!(capacity_weighted_assign(&peers), None);
    }
}
