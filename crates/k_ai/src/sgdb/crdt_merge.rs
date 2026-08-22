//! Merge CRDT no_std — LWW + multi-value visível (ADR-0100 T-048/T-049).
//! Conflito na mesma versão não sobrescreve: junta os dois lados com marcador.

use alloc::vec::Vec;

/// Magic + versão u64 LE; resto opcional (blob).
pub const HDR_LEN: usize = 5 + 8;
const CONFLICT_SEP: &[u8] = b"\n<CRDT-CONFLICT>\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeKind {
    KeepLocal,
    AdoptRemote,
    /// Mesma versão, bytes diferentes — ambos visíveis.
    ConflictBoth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    pub kind: MergeKind,
    pub version: u64,
    pub payload: Vec<u8>,
}

/// Extrai `(version, blob)` de `CRDT\0` + u64 LE [+ blob].
pub fn parse_crdt_body(payload: &[u8]) -> Option<(u64, &[u8])> {
    if !payload.starts_with(b"CRDT\0") || payload.len() < HDR_LEN {
        return None;
    }
    let mut ver = [0u8; 8];
    ver.copy_from_slice(&payload[5..13]);
    let v = u64::from_le_bytes(ver);
    let rest = if payload.len() > HDR_LEN {
        &payload[HDR_LEN..]
    } else {
        &[]
    };
    Some((v, rest))
}

/// LWW por versão; empate com conteúdo distinto → multi-value (não silent overwrite).
pub fn merge_lww(local_v: u64, local: &[u8], remote_v: u64, remote: &[u8]) -> MergeResult {
    if remote_v > local_v {
        MergeResult {
            kind: MergeKind::AdoptRemote,
            version: remote_v,
            payload: remote.to_vec(),
        }
    } else if remote_v < local_v {
        MergeResult {
            kind: MergeKind::KeepLocal,
            version: local_v,
            payload: local.to_vec(),
        }
    } else if local == remote {
        MergeResult {
            kind: MergeKind::KeepLocal,
            version: local_v,
            payload: local.to_vec(),
        }
    } else {
        let mut p = Vec::with_capacity(local.len() + CONFLICT_SEP.len() + remote.len());
        p.extend_from_slice(local);
        p.extend_from_slice(CONFLICT_SEP);
        p.extend_from_slice(remote);
        MergeResult {
            kind: MergeKind::ConflictBoth,
            version: local_v,
            payload: p,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_only() {
        let mut b = b"CRDT\0".to_vec();
        b.extend_from_slice(&7u64.to_le_bytes());
        let (v, rest) = parse_crdt_body(&b).unwrap();
        assert_eq!(v, 7);
        assert!(rest.is_empty());
    }

    #[test]
    fn lww_adopts_higher() {
        let r = merge_lww(1, b"a", 3, b"b");
        assert_eq!(r.kind, MergeKind::AdoptRemote);
        assert_eq!(r.version, 3);
        assert_eq!(r.payload, b"b");
    }

    #[test]
    fn lww_keeps_local() {
        let r = merge_lww(5, b"a", 2, b"b");
        assert_eq!(r.kind, MergeKind::KeepLocal);
        assert_eq!(r.payload, b"a");
    }

    #[test]
    fn equal_version_conflict_keeps_both() {
        let r = merge_lww(4, b"left", 4, b"right");
        assert_eq!(r.kind, MergeKind::ConflictBoth);
        assert_eq!(r.version, 4);
        assert!(r.payload.windows(CONFLICT_SEP.len()).any(|w| w == CONFLICT_SEP));
        assert!(r.payload.starts_with(b"left"));
        assert!(r.payload.ends_with(b"right"));
    }

    #[test]
    fn equal_identical_not_conflict() {
        let r = merge_lww(1, b"x", 1, b"x");
        assert_eq!(r.kind, MergeKind::KeepLocal);
        assert_eq!(r.payload, b"x");
    }
}
