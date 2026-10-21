//! Zero-copy-ish GGUF header parse (primeiros KB apenas).

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GgufError {
    #[error("buffer too short")]
    TooShort,
    #[error("bad magic")]
    BadMagic,
    #[error("unsupported version {0}")]
    BadVersion(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufHeaderInfo {
    pub version: u32,
    pub tensor_count: u64,
    pub kv_count: u64,
}

/// Lê magic + version + tensor_count + kv_count (layout GGUF v2/v3 prefix).
pub fn parse_gguf_header(buf: &[u8]) -> Result<GgufHeaderInfo, GgufError> {
    if buf.len() < 24 {
        return Err(GgufError::TooShort);
    }
    if &buf[0..4] != b"GGUF" {
        return Err(GgufError::BadMagic);
    }
    let version = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    if version < 1 || version > 3 {
        return Err(GgufError::BadVersion(version));
    }
    let tensor_count = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    let kv_count = u64::from_le_bytes(buf[16..24].try_into().unwrap());
    Ok(GgufHeaderInfo {
        version,
        tensor_count,
        kv_count,
    })
}

/// Safetensors: primeiros 8 bytes = u64 LE do tamanho do JSON header.
pub fn safetensors_header_len(buf: &[u8]) -> Result<u64, GgufError> {
    if buf.len() < 8 {
        return Err(GgufError::TooShort);
    }
    Ok(u64::from_le_bytes(buf[0..8].try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gguf_magic() {
        let mut b = vec![0u8; 24];
        b[0..4].copy_from_slice(b"GGUF");
        b[4..8].copy_from_slice(&3u32.to_le_bytes());
        b[8..16].copy_from_slice(&10u64.to_le_bytes());
        b[16..24].copy_from_slice(&5u64.to_le_bytes());
        let h = parse_gguf_header(&b).unwrap();
        assert_eq!(h.version, 3);
        assert_eq!(h.tensor_count, 10);
        assert_eq!(h.kv_count, 5);
    }

    #[test]
    fn reject_bad_magic() {
        assert_eq!(parse_gguf_header(&[0; 24]), Err(GgufError::BadMagic));
    }
}
