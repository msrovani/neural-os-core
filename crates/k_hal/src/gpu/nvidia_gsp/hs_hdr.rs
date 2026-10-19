//! Parse HS / bin headers (Nouveau `nvfw_bin_hdr` + `nvfw_hs_header_v2`).
//!
//! Layout mínimo para validar blobs `booter_load-*` / `bootloader-*` antes
//! de tocar Falcon/SEC2. Sem silício = só parse host-testável.

/// `nvfw_bin_hdr` (Nouveau include/nvfw/bin.h) — LE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinHdr {
    pub magic: u32,
    pub header_offset: u32,
    pub data_offset: u32,
    pub data_size: u32,
}

/// Subconjunto útil de `nvfw_hs_header_v2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HsHdrV2 {
    pub sig_prod_offset: u32,
    pub sig_prod_size: u32,
    pub patch_loc: u32,
    pub patch_sig: u32,
    pub num_sig: u32,
    pub header_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HsBinInfo {
    pub bin: BinHdr,
    pub hs: Option<HsHdrV2>,
    pub total_len: usize,
}

const BIN_MAGIC: u32 = 0x3b1d_2f00; // Nouveau NVFW_BIN_MAGIC (comum)

fn u32_le(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Parse best-effort: aceita magic Nouveau ou layout genérico offset-based.
pub fn parse_hs_bin(blob: &[u8]) -> Option<HsBinInfo> {
    if blob.len() < 16 {
        return None;
    }
    let magic = u32_le(blob, 0)?;
    // Alguns blobs OpenRM usam magic diferente — se header_offset parece
    // plausível, seguimos; senão rejeita.
    let header_offset = u32_le(blob, 4)?;
    let data_offset = u32_le(blob, 8)?;
    let data_size = u32_le(blob, 12)?;
    if data_offset as usize >= blob.len() {
        return None;
    }
    if (data_offset as u64) + (data_size as u64) > blob.len() as u64 {
        return None;
    }
    let bin = BinHdr {
        magic,
        header_offset,
        data_offset,
        data_size,
    };
    let hs = parse_hs_v2(blob, header_offset as usize);
    if magic != BIN_MAGIC && hs.is_none() && data_size == 0 {
        return None;
    }
    Some(HsBinInfo {
        bin,
        hs,
        total_len: blob.len(),
    })
}

fn parse_hs_v2(blob: &[u8], hoff: usize) -> Option<HsHdrV2> {
    // Campos mínimos alinhados a Nouveau hs_header_v2 (offsets relativos).
    // Sem struct completa: lê sig/patch/num se couber.
    if hoff == 0 || hoff.saturating_add(32) > blob.len() {
        return None;
    }
    let sig_prod_offset = u32_le(blob, hoff)?;
    let sig_prod_size = u32_le(blob, hoff + 4)?;
    let patch_loc = u32_le(blob, hoff + 8)?;
    let patch_sig = u32_le(blob, hoff + 12)?;
    let num_sig = u32_le(blob, hoff + 16)?;
    let header_offset = u32_le(blob, hoff + 20)?;
    if num_sig == 0 || num_sig > 64 {
        return None;
    }
    Some(HsHdrV2 {
        sig_prod_offset,
        sig_prod_size,
        patch_loc,
        patch_sig,
        num_sig,
        header_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_tiny() {
        assert!(parse_hs_bin(&[0u8; 8]).is_none());
    }

    #[test]
    fn parse_minimal_bin() {
        let mut b = alloc::vec![0u8; 64];
        b[0..4].copy_from_slice(&BIN_MAGIC.to_le_bytes());
        b[4..8].copy_from_slice(&16u32.to_le_bytes()); // header @16
        b[8..12].copy_from_slice(&48u32.to_le_bytes()); // data @48
        b[12..16].copy_from_slice(&16u32.to_le_bytes()); // data_size 16
        // hs @16
        b[16..20].copy_from_slice(&0u32.to_le_bytes());
        b[20..24].copy_from_slice(&64u32.to_le_bytes());
        b[24..28].copy_from_slice(&0u32.to_le_bytes());
        b[28..32].copy_from_slice(&0u32.to_le_bytes());
        b[32..36].copy_from_slice(&1u32.to_le_bytes()); // num_sig
        b[36..40].copy_from_slice(&0u32.to_le_bytes());
        let info = parse_hs_bin(&b).expect("parse");
        assert_eq!(info.bin.data_size, 16);
        assert!(info.hs.is_some());
        assert_eq!(info.hs.unwrap().num_sig, 1);
    }
}
