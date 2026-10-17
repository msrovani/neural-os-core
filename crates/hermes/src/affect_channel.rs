//! Typed EventBus channel for AFFECT_SNAPSHOT (Item 14).
//!
//! Replaces the global `AFFECT_SNAPSHOT` TicketLock with a typed EventBus channel.
//! Provides type-safe publish/subscribe for AffectSnapshot updates.

use alloc::vec::Vec;
use alloc::string::String;
use event_bus::{EventBus, Event, Receiver};
use crate::affect::AffectVector;
use crate::globals::AffectSnapshot;
use k_nano::EVENT_BUS;

/// Topic name for AFFECT_SNAPSHOT updates.
pub const AFFECT_SNAPSHOT_TOPIC: &str = "AFFECT_SNAPSHOT";

/// Serialize AffectSnapshot to bytes (little-endian, fixed layout).
/// Layout: 8 f32s (valence, arousal, dominance, uncertainty, urgency, fatigue, curiosity, coherence) + 1 u32 (phase_deg) = 36 bytes.
pub fn serialize_affect_snapshot(snap: &AffectSnapshot) -> Vec<u8> {
    let mut buf = Vec::with_capacity(36);
    buf.extend_from_slice(&snap.valence.to_le_bytes());
    buf.extend_from_slice(&snap.arousal.to_le_bytes());
    buf.extend_from_slice(&snap.dominance.to_le_bytes());
    buf.extend_from_slice(&snap.uncertainty.to_le_bytes());
    buf.extend_from_slice(&snap.urgency.to_le_bytes());
    buf.extend_from_slice(&snap.fatigue.to_le_bytes());
    buf.extend_from_slice(&snap.curiosity.to_le_bytes());
    buf.extend_from_slice(&snap.coherence.to_le_bytes());
    buf.extend_from_slice(&snap.phase_deg.to_le_bytes());
    buf
}

/// Deserialize bytes to AffectSnapshot.
/// Returns None if buffer is not exactly 36 bytes.
pub fn deserialize_affect_snapshot(bytes: &[u8]) -> Option<AffectSnapshot> {
    if bytes.len() != 36 {
        return None;
    }
    let valence = f32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let arousal = f32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let dominance = f32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let uncertainty = f32::from_le_bytes(bytes[12..16].try_into().ok()?);
    let urgency = f32::from_le_bytes(bytes[16..20].try_into().ok()?);
    let fatigue = f32::from_le_bytes(bytes[20..24].try_into().ok()?);
    let curiosity = f32::from_le_bytes(bytes[24..28].try_into().ok()?);
    let coherence = f32::from_le_bytes(bytes[28..32].try_into().ok()?);
    let phase_deg = u32::from_le_bytes(bytes[32..36].try_into().ok()?);
    Some(AffectSnapshot {
        valence,
        arousal,
        dominance,
        uncertainty,
        urgency,
        fatigue,
        curiosity,
        coherence,
        phase_deg,
    })
}

/// Publish an AffectSnapshot update to the EventBus.
/// Returns number of subscribers delivered to (0 = no subscribers).
pub fn publish_affect_snapshot(snap: &AffectSnapshot) -> usize {
    let payload = serialize_affect_snapshot(snap);
    let event = Event {
        id: 0,
        topic: String::from(AFFECT_SNAPSHOT_TOPIC),
        payload,
        token: event_bus::CapabilityToken::Legacy(1),
    };
    EVENT_BUS.publish(event).unwrap_or(0)
}

/// Subscribe to AFFECT_SNAPSHOT updates.
/// Returns a Receiver that yields deserialized AffectSnapshot updates.
pub fn subscribe_affect_snapshot() -> AffectSnapshotReceiver {
    let receiver = EVENT_BUS.subscribe(AFFECT_SNAPSHOT_TOPIC);
    AffectSnapshotReceiver { inner: receiver }
}

/// Typed receiver for AFFECT_SNAPSHOT updates.
pub struct AffectSnapshotReceiver {
    inner: Receiver,
}

impl AffectSnapshotReceiver {
    /// Try to receive the next AffectSnapshot update.
    /// Returns None if no update available or deserialization failed.
    pub fn try_receive(&self) -> Option<AffectSnapshot> {
        self.inner.try_receive().and_then(|event| {
            deserialize_affect_snapshot(&event.payload)
        })
    }

    /// Check if there are pending updates.
    pub fn has_pending(&self) -> bool {
        self.inner.has_pending()
    }

    /// Get the number of pending updates.
    pub fn pending_len(&self) -> usize {
        self.inner.pending_len()
    }
}

/// Sync affect snapshot to both the legacy global and the new EventBus channel.
/// This maintains backward compatibility during transition.
pub fn sync_affect_snapshot_both(affect: &crate::affect::AffectVector, phase_deg: u32) {
    // Shim fino. S479: esta função publicava o EventBus e chamava
    // `crate::globals::sync_affect_snapshot` DE VOLTA — e globals delegava para
    // cá. Ciclo mútuo infinito (introduzido em 92d25242/s468d) que estourava a
    // stack de 8 MiB do scheduler do BSP em ~200k níveis, cravava o #PF no
    // limite do heap e esmagava 512 KiB do heap VA em silêncio.
    // O trabalho real (global AFFECT_SNAPSHOT + canal tipado) vive em globals.
    crate::globals::sync_affect_snapshot(affect, phase_deg);
}