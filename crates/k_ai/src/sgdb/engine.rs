//! ADR-0063 F3 — AiosDatabaseEngine: MemoryDoc ↔ TickvLite + ART (L0–L3) + BQ (L4–L5).

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::art::ArtIndex;
use super::bq::BqFlatIndex;
use super::memory_doc::{MemoryDoc, MemoryLayer};

/// Contador monotônico de handles internos (ART / BQ ids).
static NEXT_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

pub struct AiosDatabaseEngine {
    pub art: ArtIndex,
    pub bq: BqFlatIndex,
    /// node_id local para vector clock
    pub node_id: u8,
    pub puts: u64,
    pub gets: u64,
}

impl AiosDatabaseEngine {
    pub fn new(node_id: u8) -> Self {
        AiosDatabaseEngine {
            art: ArtIndex::new(),
            bq: BqFlatIndex::new(),
            node_id,
            puts: 0,
            gets: 0,
        }
    }

    /// Persiste doc em TickvLite (`md/Lx/key`) e indexa.
    pub fn put(&mut self, mut doc: MemoryDoc) -> Result<u64, &'static str> {
        doc.clock.tick(self.node_id);
        let id = NEXT_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let sk = doc.storage_key();
        let blob = doc.encode();

        // Persistência: TickvLite se pronto; senão só índice em RAM (honesty).
        if k_nano::storage::is_ready() {
            k_nano::storage::put_blob(&sk, &blob).map_err(|_| "tickv put")?;
        }

        match doc.layer {
            MemoryLayer::L0Sensory
            | MemoryLayer::L1Working
            | MemoryLayer::L2EpisodicShort
            | MemoryLayer::L3EpisodicLong => {
                self.art.insert(&sk, id);
            }
            MemoryLayer::L4Semantic | MemoryLayer::L5Procedural => {
                if let Some(ref bv) = doc.bitvec {
                    self.bq.insert(id, bv.clone());
                } else if !doc.payload.is_empty() {
                    // heurística: primeiros bytes → f32 LE se múltiplo de 4
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
                // também indexa chave no ART para lookup por id textual
                self.art.insert(&sk, id);
            }
            MemoryLayer::L6Reserved | MemoryLayer::L7Identity => {
                self.art.insert(&sk, id);
            }
        }

        self.puts += 1;
        let _ = id;
        Ok(id)
    }

    pub fn get(&mut self, layer: MemoryLayer, key: &str) -> Result<Option<MemoryDoc>, &'static str> {
        self.gets += 1;
        let sk = alloc::format!("md/{}/{}", layer.as_str(), key);
        if !k_nano::storage::is_ready() {
            return Ok(None);
        }
        match k_nano::storage::get_blob(&sk) {
            Ok(bytes) => Ok(Some(MemoryDoc::decode(&bytes)?)),
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
