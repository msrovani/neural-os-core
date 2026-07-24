//! Full-text search MVP sobre mounts `/mnt` (Labor 44). Cache RAM only.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

struct Entry {
    path: String,
    blob: String,
}

static IDX: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

pub fn index_put(path: &str, text: &str) {
    let mut g = IDX.lock();
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