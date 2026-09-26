//! Full-text search MVP sobre mounts `/mnt` (Labor 44). Cache RAM only.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

struct Entry {
    path: String,
    blob: String,
}

static IDX: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

/// Runtime hygiene (s410d): índice FTS crescia sem teto — cada index_put
/// empilhava outra entrada do MESMO path (re-index duplicava). Dedup por
/// path (replace) + cap com evicção FIFO.
const FTS_IDX_CAP: usize = 256;

pub fn index_put(path: &str, text: &str) {
    let mut g = IDX.lock();
    if let Some(slot) = g.iter_mut().find(|e| e.path == path) {
        slot.blob = String::from(text);
        return;
    }
    if g.len() >= FTS_IDX_CAP {
        g.remove(0); // evicção FIFO
    }
    g.push(Entry {
        path: String::from(path),
        blob: String::from(text),
    });
}

pub fn search(needle: &str) -> Vec<String> {
    let g = IDX.lock();
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    for e in g.iter() {
        if e.blob.contains(needle) || e.path.contains(needle) {
            out.push(e.path.clone());
        }
    }
    out
}

pub fn boot_smoke() -> bool {
    index_put("/mnt/ram/readme", "neural os core search");
    let hits = search("neural");
    let ok = !hits.is_empty();
    crate::slog_nano!(
        "SEARCH",
        "info",
        "step=fts status={} hits={} VERDICT={}",
        if ok { "OK" } else { "FAIL" },
        hits.len(),
        if ok { "PARTIAL" } else { "FAIL" }
    );
    ok
}