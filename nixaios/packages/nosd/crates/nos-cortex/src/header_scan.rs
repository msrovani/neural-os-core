//! Header scanner — GGUF / Safetensors sem carregar tensores.

use nos_core::gguf::{parse_gguf_header, safetensors_header_len};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMeta {
    pub path: PathBuf,
    pub kind: String,
    pub detail: String,
}

pub fn scan_path(root: &Path, max_files: usize) -> Vec<ModelMeta> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            if out.len() >= max_files {
                return out;
            }
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            let ext = ext.to_ascii_lowercase();
            if ext == "gguf" {
                if let Some(m) = probe_gguf(&p) {
                    out.push(m);
                }
            } else if ext == "safetensors" {
                if let Some(m) = probe_safetensors(&p) {
                    out.push(m);
                }
            }
        }
    }
    out
}

fn probe_gguf(path: &Path) -> Option<ModelMeta> {
    let mut f = File::open(path).ok()?;
    let mut buf = [0u8; 64];
    let n = f.read(&mut buf).ok()?;
    if n < 24 {
        return None;
    }
    let h = parse_gguf_header(&buf[..n.min(64)]).ok()?;
    Some(ModelMeta {
        path: path.to_path_buf(),
        kind: "gguf".into(),
        detail: format!(
            "v{} tensors={} kv={}",
            h.version, h.tensor_count, h.kv_count
        ),
    })
}

fn probe_safetensors(path: &Path) -> Option<ModelMeta> {
    let mut f = File::open(path).ok()?;
    let mut buf = [0u8; 8];
    f.read_exact(&mut buf).ok()?;
    let n = safetensors_header_len(&buf).ok()?;
    Some(ModelMeta {
        path: path.to_path_buf(),
        kind: "safetensors".into(),
        detail: format!("header_json_bytes={}", n),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn scans_gguf() {
        let d = tempdir().unwrap();
        let p = d.path().join("t.gguf");
        let mut f = File::create(&p).unwrap();
        let mut b = vec![0u8; 24];
        b[0..4].copy_from_slice(b"GGUF");
        b[4..8].copy_from_slice(&3u32.to_le_bytes());
        f.write_all(&b).unwrap();
        let metas = scan_path(d.path(), 10);
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].kind, "gguf");
    }
}
