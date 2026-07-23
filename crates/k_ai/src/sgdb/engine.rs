//! ADR-0063 F3/D2 — AiosDatabaseEngine: MemoryDoc ↔ TickvLite + ART + BQ.
//! L0/L1: RAM-only por default (checkpoint explícito). ART guarda id lógico; key = md/...

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::art::ArtIndex;
use super::bq::BqFlatIndex;
use super::memory_doc::{MemoryDoc, MemoryDocView, MemoryLayer};

/// Contador monotônico de handles internos (ART / BQ ids).
static NEXT_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

fn is_ram_layer(layer: MemoryLayer) -> bool {
    matches!(layer, MemoryLayer::L0Sensory | MemoryLayer::L1Working)
}

pub struct AiosDatabaseEngine {
    pub art: ArtIndex,
    pub bq: BqFlatIndex,
    /// node_id local para vector clock
    pub node_id: u8,
    pub puts: u64,
    pub gets: u64,
    /// D2: blobs L0/L1 encoded (storage_key → NMD1); não toca Tickv até checkpoint.
    ram_l0l1: BTreeMap<String, Vec<u8>>,
    /// Puts L0/L1 que bypassaram Tickv (métrica honesty).
    pub ram_puts: u64,
    /// E2: id lógico → storage_key (recall BQ → doc).
    id_to_sk: BTreeMap<u64, String>,
}

impl AiosDatabaseEngine {
    pub fn new(node_id: u8) -> Self {
        AiosDatabaseEngine {
            art: ArtIndex::new(),
            bq: BqFlatIndex::new(),
            node_id,
            puts: 0,
            gets: 0,
            ram_l0l1: BTreeMap::new(),
            ram_puts: 0,
            id_to_sk: BTreeMap::new(),
        }
    }

    /// Persiste doc: L0/L1 → RAM; demais → TickvLite (`md/Lx/key`) + indexa.
    pub fn put(&mut self, mut doc: MemoryDoc) -> Result<u64, &'static str> {
        doc.clock.tick(self.node_id);
        let id = NEXT_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let sk = doc.storage_key();
        let blob = doc.encode();

        if is_ram_layer(doc.layer) {
            self.ram_l0l1.insert(sk.clone(), blob);
            self.ram_puts = self.ram_puts.saturating_add(1);
        } else if k_nano::storage::is_ready() {
            k_nano::storage::put_blob(&sk, &blob).map_err(|_| "tickv put")?;
        }

        self.index_doc(id, &doc, &sk);
        self.puts += 1;
        Ok(id)
    }

    /// HITL / SleepCycle: flush L0/L1 RAM → Tickv. Honesty: sem isto, reboot perde L0/L1.
    pub fn checkpoint_l0l1(&mut self) -> Result<usize, &'static str> {
        if !k_nano::storage::is_ready() {
            return Err("tickv not ready");
        }
        let mut n = 0usize;
        for (sk, blob) in self.ram_l0l1.iter() {
            k_nano::storage::put_blob(sk, blob).map_err(|_| "tickv put")?;
            n += 1;
        }
        Ok(n)
    }

    /// Pós-checkpoint: drop arena RAM (docs já no Tickv sob `md/L0|L1/…`).
    pub fn prune_ram_l0l1(&mut self) -> usize {
        let n = self.ram_l0l1.len();
        self.ram_l0l1.clear();
        n
    }

    fn index_doc(&mut self, id: u64, doc: &MemoryDoc, sk: &str) {
        self.id_to_sk.insert(id, String::from(sk));
        match doc.layer {
            MemoryLayer::L0Sensory
            | MemoryLayer::L1Working
            | MemoryLayer::L2EpisodicShort
            | MemoryLayer::L3EpisodicLong => {
                self.art.insert(sk, id);
            }
            MemoryLayer::L4Semantic | MemoryLayer::L5Procedural => {
                if let Some(ref bv) = doc.bitvec {
                    self.bq.insert(id, bv.clone());
                } else if !doc.payload.is_empty() {
                    let n = doc.payload.len() / 4;
                    if n > 0 {
                        let mut f = Vec::with_capacity(n);
                        for i in 0..n {
                            let o = i * 4;
                            let w = f32::from_le_bytes([
                                doc.payload[o],
                                doc.payload[o + 1],
                                doc.payload[o + 2],
                                doc.payload[o + 3],
                            ]);
                            f.push(w);
                        }
                        self.bq.insert_f32(id, &f);
                    }
                }
                self.art.insert(sk, id);
            }
            MemoryLayer::L6Reserved | MemoryLayer::L7Identity => {
                self.art.insert(sk, id);
            }
        }
    }

    /// Q2: reconstrói ART/BQ a partir de keys Tickv `md/*` (pós-remount).
    /// Não limpa ram_l0l1 (sessão viva).
    pub fn rebuild_indices_from_tickv(&mut self) -> usize {
        self.art.clear();
        self.bq.clear();
        self.id_to_sk.clear();
        // Reindex RAM L0/L1 first (logical ids fresh)
        let mut n = 0usize;
        let ram_keys: Vec<(String, Vec<u8>)> = self
            .ram_l0l1
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (sk, bytes) in ram_keys {
            if let Ok(doc) = MemoryDoc::decode(&bytes) {
                let id = NEXT_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                self.index_doc(id, &doc, &sk);
                n += 1;
            }
        }
        let keys = k_nano::storage::with_tickv(|kv| kv.keys_with_prefix("md/")).unwrap_or_default();
        for sk in keys {
            // skip if already in RAM (live session wins)
            if self.ram_l0l1.contains_key(&sk) {
                continue;
            }
            if let Ok(bytes) = k_nano::storage::get_blob(&sk) {
                if let Ok(doc) = MemoryDoc::decode(&bytes) {
                    let id = NEXT_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    self.index_doc(id, &doc, &sk);
                    n += 1;
                }
            }
        }
        n
    }

    pub fn get(&mut self, layer: MemoryLayer, key: &str) -> Result<Option<MemoryDoc>, &'static str> {
        let sk = alloc::format!("md/{}/{}", layer.as_str(), key);
        self.get_by_storage_key(&sk)
    }

    /// Load por storage key canônica `md/Lx/...` (RAM L0/L1 ou Tickv).
    pub fn get_by_storage_key(&mut self, sk: &str) -> Result<Option<MemoryDoc>, &'static str> {
        self.gets += 1;
        if let Some(bytes) = self.ram_l0l1.get(sk) {
            return Ok(Some(MemoryDoc::decode(bytes)?));
        }
        if !k_nano::storage::is_ready() {
            return Ok(None);
        }
        match k_nano::storage::get_blob(sk) {
            Ok(bytes) => Ok(Some(MemoryDoc::decode(&bytes)?)),
            Err("missing") => Ok(None),
            Err("corrupt") => Err("corrupt"),
            Err(e) => Err(e),
        }
    }

    pub fn get_view_bytes(
        &mut self,
        layer: MemoryLayer,
        key: &str,
    ) -> Result<Option<Vec<u8>>, &'static str> {
        self.gets += 1;
        let sk = alloc::format!("md/{}/{}", layer.as_str(), key);
        if let Some(bytes) = self.ram_l0l1.get(&sk) {
            let _ = MemoryDocView::parse(bytes)?;
            return Ok(Some(bytes.clone()));
        }
        if !k_nano::storage::is_ready() {
            return Ok(None);
        }
        match k_nano::storage::get_blob(&sk) {
            Ok(bytes) => {
                let _ = MemoryDocView::parse(&bytes)?;
                Ok(Some(bytes))
            }
            Err("missing") => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn art_lookup(&self, storage_key: &str) -> Option<u64> {
        self.art.get(storage_key)
    }

    pub fn bq_top_k_f32(&self, query: &[f32], k: usize) -> Vec<(u64, u32)> {
        self.bq.top_k_f32(query, k)
    }

    pub fn storage_key_of(&self, id: u64) -> Option<&str> {
        self.id_to_sk.get(&id).map(|s| s.as_str())
    }

    pub fn ram_l0l1_len(&self) -> usize {
        self.ram_l0l1.len()
    }

    pub fn bq_len(&self) -> usize {
        self.bq.len()
    }
}

pub static ENGINE: Mutex<Option<AiosDatabaseEngine>> = Mutex::new(None);

pub fn init_global(node_id: u8) {
    *ENGINE.lock() = Some(AiosDatabaseEngine::new(node_id));
}

pub fn with_engine<R>(f: impl FnOnce(&mut AiosDatabaseEngine) -> R) -> Option<R> {
    let mut g = ENGINE.lock();
    g.as_mut().map(f)
}

/// Put conveniência com texto (L1 working).
pub fn remember_text(layer: MemoryLayer, key: &str, text: &str) -> Result<u64, &'static str> {
    with_engine(|e| {
        let doc = MemoryDoc::new(layer, key, text.as_bytes().to_vec());
        e.put(doc)
    })
    .unwrap_or(Err("engine down"))
}

pub fn backend_note() -> String {
    if k_nano::storage::is_ready() {
        alloc::format!("tickv:{}", k_nano::storage::backend_name())
    } else {
        String::from("ram-index-only")
    }
}
