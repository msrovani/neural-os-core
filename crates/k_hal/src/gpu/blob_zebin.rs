//! Intel zebin / ELF EM_INTELGT → `.text` + `.ze_info` (Wave 1).
//! Gen9 + Arc. Sem EU real → stub path no canário.

use alloc::vec::Vec;
use crate::gpu::compute_abi::IsaTag;
use crate::gpu::kernel_image::KernelImage;
use crate::gpu::kernel_pack::IrOrigin;

const EM_INTELGT: u16 = 205; // 0xCD

pub fn parse(blob: &[u8], isa: IsaTag) -> Option<KernelImage> {
    if blob.len() < 64 || &blob[0..4] != b"\x7fELF" {
        return None;
    }
    let e_shoff = u64::from_le_bytes(blob[40..48].try_into().ok()?) as usize;
    let e_shentsize = u16::from_le_bytes([blob[58], blob[59]]) as usize;
    let e_shnum = u16::from_le_bytes([blob[60], blob[61]]) as usize;
    let e_shstrndx = u16::from_le_bytes([blob[62], blob[63]]) as usize;
    if e_shentsize < 64 || e_shnum == 0 {
        return None;
    }
    let shstr = section(blob, e_shoff, e_shentsize, e_shstrndx)?;
    let str_off = u64::from_le_bytes(shstr[24..32].try_into().ok()?) as usize;
    let str_sz = u64::from_le_bytes(shstr[32..40].try_into().ok()?) as usize;
    if str_off + str_sz > blob.len() {
        return None;
    }
    let strtab = &blob[str_off..str_off + str_sz];

    let mut text: Option<Vec<u8>> = None;
    let mut regs = 128u32; // Gen9 GRF default honesto
    let mut shared = 0u32;
    let mut param_size = 256u32;

    for i in 0..e_shnum {
        let sh = section(blob, e_shoff, e_shentsize, i)?;
        let name_off = u32::from_le_bytes(sh[0..4].try_into().ok()?) as usize;
        let name = cstr(strtab, name_off).unwrap_or("");
        let sh_type = u32::from_le_bytes(sh[4..8].try_into().ok()?);
        let off = u64::from_le_bytes(sh[24..32].try_into().ok()?) as usize;
        let sz = u64::from_le_bytes(sh[32..40].try_into().ok()?) as usize;
        if off + sz > blob.len() || sz == 0 {
            continue;
        }
        let data = &blob[off..off + sz];
        if (name == ".text" || name.starts_with(".text")) && sh_type == 1 {
            text = Some(data.to_vec());
        }
        if name.contains("ze_info") || name.contains(".ze_info") {
            // YAML/ASCII hints — procura "grf_count:" / "slm_size:"
            if let Ok(s) = core::str::from_utf8(data) {
                if let Some(v) = find_u32_after(s, "grf_count:") {
                    regs = v.max(1);
                }
                if let Some(v) = find_u32_after(s, "slm_size:") {
                    shared = v;
                }
                if let Some(v) = find_u32_after(s, "arg_size:") {
                    param_size = v.max(16);
                }
            }
        }
    }
    let code = text?;
    let _ = EM_INTELGT;
    Some(KernelImage {
        isa,
        code,
        regs,
        shared,
        barriers: 1,
        param_base: 0,
        param_size,
        ir: IrOrigin::Zebin,
        is_stub: false,
    })
}

fn find_u32_after(s: &str, key: &str) -> Option<u32> {
    let i = s.find(key)?;
    let rest = s[i + key.len()..].trim_start();
    let num: u32 = rest
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some(num)
}

fn section(blob: &[u8], shoff: usize, entsize: usize, idx: usize) -> Option<&[u8]> {
    let off = shoff + idx * entsize;
    (off + entsize <= blob.len()).then(|| &blob[off..off + entsize])
}

fn cstr(tab: &[u8], off: usize) -> Option<&str> {
    if off >= tab.len() {
        return None;
    }
    let end = tab[off..].iter().position(|&b| b == 0)?;
    core::str::from_utf8(&tab[off..off + end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::blob_cubin::make_minimal_cubin_fixture;

    #[test]
    fn parse_elf_text_as_zebin_shape() {
        let blob = make_minimal_cubin_fixture();
        let img = parse(&blob, IsaTag::Gen9).expect("zebin-like");
        assert_eq!(img.ir, IrOrigin::Zebin);
        assert!(!img.code.is_empty());
    }
}
