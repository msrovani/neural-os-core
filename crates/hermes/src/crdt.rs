//! CRDT merge no_std LWW/multi-value (ADR-0100 T-048/T-049, ADR-0081 C4 #315.26).
//! LWW per-key com tie-break timestamp + node_id, conflito visível (não silent overwrite).
//! Reusa wire `CRDT\0` de k_nano/k_ai (k_ai::sgdb::crdt_merge) — não duplica parser.
//! no_std via `alloc` BTreeMap, sem alloc desnecessário (ponytail: BTreeMap direto).

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Separador visível para conflito same-ts (reusa k_ai).
pub const CONFLICT_SEP: &[u8] = b"\n<CRDT-CONFLICT>\n";

/// Entrada CRDT LWW por chave: valor + timestamp lógico + node_id tie-break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrdtEntry {
    pub value: Vec<u8>,
    /// timestamp lógico (ex: TIMER_TICKS ou version). Maior vence.
    pub ts: u64,
    /// tie-break determinístico: menor node_id vence quando ts igual.
    pub node: u8,
}

impl CrdtEntry {
    pub fn new(value: Vec<u8>, ts: u64, node: u8) -> Self {
        Self { value, ts, node }
    }
}

/// Mapa CRDT LWW: chave → entrada com ts+node.
pub type CrdtMap = BTreeMap<Vec<u8>, CrdtEntry>;

/// Re-export parser do wire `CRDT\0` (k_ai) — não duplica.
pub use k_ai::sgdb::crdt_merge::{parse_crdt_body, HDR_LEN, MergeKind, MergeResult, merge_lww};

/// Merge LWW com timestamp + node_id tie-break e conflito visível.
///
/// Regra por chave:
/// - ts maior vence
/// - ts menor mantém local
/// - ts igual:
///     - valor idêntico → KeepLocal
///     - valor distinto → multi-value visível: winner (menor node) + SEP + loser
///       (não silent overwrite). Version do resultado = ts.
/// Winner determinístico por node_id evita flapping em partições.
pub fn merge_entry(local: &CrdtEntry, remote: &CrdtEntry) -> (CrdtEntry, MergeKind) {
    if remote.ts > local.ts {
        return (remote.clone(), MergeKind::AdoptRemote);
    }
    if remote.ts < local.ts {
        return (local.clone(), MergeKind::KeepLocal);
    }
    // ts igual
    if local.value == remote.value {
        return (local.clone(), MergeKind::KeepLocal);
    }
    // payloads distintos no mesmo ts → conflito visível
    // tie-break: menor node vence como prefixo, mas ambos visíveis
    let (winner, loser) = if local.node <= remote.node {
        (local, remote)
    } else {
        (remote, local)
    };
    let mut v = Vec::with_capacity(winner.value.len() + CONFLICT_SEP.len() + loser.value.len());
    v.extend_from_slice(&winner.value);
    v.extend_from_slice(CONFLICT_SEP);
    v.extend_from_slice(&loser.value);
    let merged = CrdtEntry {
        value: v,
        ts: local.ts,
        node: winner.node,
    };
    (merged, MergeKind::ConflictBoth)
}

/// Merge de dois mapas LWW (BTreeMap no_std via alloc).
/// Sem alloc desnecessário: single-pass sobre chaves union.
pub fn merge(a: &CrdtMap, b: &CrdtMap) -> CrdtMap {
    let mut out = CrdtMap::new();
    // ponytail: BTreeMap iteration é O(n log n) mas map ≤16-64 keys no mesh — trivial
    for (k, va) in a.iter() {
        if let Some(vb) = b.get(k) {
            let (merged, _kind) = merge_entry(va, vb);
            out.insert(k.clone(), merged);
        } else {
            out.insert(k.clone(), va.clone());
        }
    }
    for (k, vb) in b.iter() {
        if !a.contains_key(k) {
            out.insert(k.clone(), vb.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn lww_higher_ts_wins() {
        let mut a = CrdtMap::new();
        a.insert(b"k".to_vec(), CrdtEntry::new(b"old".to_vec(), 1, 1));
        let mut b = CrdtMap::new();
        b.insert(b"k".to_vec(), CrdtEntry::new(b"new".to_vec(), 2, 2));
        let m = merge(&a, &b);
        let e = m.get(&b"k".to_vec()).unwrap();
        assert_eq!(e.value, b"new");
        assert_eq!(e.ts, 2);
        assert_eq!(e.node, 2);
    }

    #[test]
    fn conflict_visible_not_silent_overwrite() {
        // mesmo ts, nodes diferentes, valores distintos → ambos visíveis
        let mut a = CrdtMap::new();
        a.insert(b"k".to_vec(), CrdtEntry::new(b"left".to_vec(), 5, 1));
        let mut b = CrdtMap::new();
        b.insert(b"k".to_vec(), CrdtEntry::new(b"right".to_vec(), 5, 2));
        let m = merge(&a, &b);
        let v = &m.get(&b"k".to_vec()).unwrap().value;
        // não pode ser só "left" nem só "right" → silent overwrite detectado
        assert_ne!(v, b"left");
        assert_ne!(v, b"right");
        // deve conter ambos e separador
        assert!(v.windows(CONFLICT_SEP.len()).any(|w| w == CONFLICT_SEP), "missing CONFLICT_SEP");
        assert!(v.windows(b"left".len()).any(|w| w == b"left"));
        assert!(v.windows(b"right".len()).any(|w| w == b"right"));
        // tie-break menor node vence como prefixo
        assert!(v.starts_with(b"left"), "menor node (1) deveria vencer prefixo");
    }

    #[test]
    fn tie_break_node_id_deterministico() {
        // ts igual, node 2 vs 1 → node 1 vence prefixo
        let mut a = CrdtMap::new();
        a.insert(b"k".to_vec(), CrdtEntry::new(b"from_2".to_vec(), 5, 2));
        let mut b = CrdtMap::new();
        b.insert(b"k".to_vec(), CrdtEntry::new(b"from_1".to_vec(), 5, 1));
        let m = merge(&a, &b);
        let v = &m.get(&b"k".to_vec()).unwrap().value;
        assert!(v.starts_with(b"from_1"), "tie-break menor node_id deve vencer");
        assert!(v.windows(b"from_2".len()).any(|w| w == b"from_2"), "loser ainda visível");
    }

    #[test]
    fn disjoint_keys_union() {
        let mut a = CrdtMap::new();
        a.insert(b"k1".to_vec(), CrdtEntry::new(b"v1".to_vec(), 1, 1));
        let mut b = CrdtMap::new();
        b.insert(b"k2".to_vec(), CrdtEntry::new(b"v2".to_vec(), 1, 2));
        let m = merge(&a, &b);
        assert_eq!(m.len(), 2);
        assert_eq!(m[&b"k1".to_vec()].value, b"v1");
        assert_eq!(m[&b"k2".to_vec()].value, b"v2");
    }

    #[test]
    fn identical_same_ts_same_value_no_dup() {
        let mut a = CrdtMap::new();
        a.insert(b"k".to_vec(), CrdtEntry::new(b"same".to_vec(), 5, 1));
        let mut b = CrdtMap::new();
        b.insert(b"k".to_vec(), CrdtEntry::new(b"same".to_vec(), 5, 1));
        let m = merge(&a, &b);
        assert_eq!(m[&b"k".to_vec()].value, b"same");
        assert!(!m[&b"k".to_vec()].value.windows(CONFLICT_SEP.len()).any(|w| w == CONFLICT_SEP));
    }

    #[test]
    fn merge_entry_keep_local_on_lower_ts() {
        let local = CrdtEntry::new(b"local".to_vec(), 10, 1);
        let remote = CrdtEntry::new(b"remote".to_vec(), 2, 2);
        let (e, kind) = merge_entry(&local, &remote);
        assert_eq!(kind, MergeKind::KeepLocal);
        assert_eq!(e.value, b"local");
    }

    #[test]
    fn parse_reuse_not_duplicated() {
        // garante que parse_crdt_body reexport funciona (reuso k_nano mesh tipos)
        let mut buf = vec![];
        buf.extend_from_slice(b"CRDT\0");
        buf.extend_from_slice(&42u64.to_le_bytes());
        buf.extend_from_slice(b"hello");
        let (v, rest) = parse_crdt_body(&buf).unwrap();
        assert_eq!(v, 42);
        assert_eq!(rest, b"hello");
    }
}
