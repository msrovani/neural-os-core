//! HSACO / AMD code object COV4–5 → `.text` + KD hints (Wave 1).
//! Referência: LLVM AMDGPUUsage. Sem KD completo → defaults honestos.

use alloc::vec::Vec;
use crate::gpu::compute_abi::IsaTag;
use crate::gpu::kernel_image::KernelImage;
use crate::gpu::kernel_pack::IrOrigin;

const EM_AMDGPU: u16 = 224; // 0xE0

pub fn parse(blob: &[u8], isa: IsaTag) -> Option<KernelImage> {
    if blob.len() < 64 || &blob[0..4] != b"\x7fELF" {
        return None;
    }
    let e_machine = u16::from_le_bytes([blob[18], blob[19]]);
    if e_machine != EM_AMDGPU && e_machine != 0 {
        // aceita se achar .text
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
    let mut regs = 32u32;
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
        if (name == ".text" || name.starts_with(".text.")) && sh_type == 1 {
            text = Some(data.to_vec());
        }
        // Kernel descriptor note (best-effort)
        if name.contains("amdhsa") || name.contains(".kd") {
            if data.len() >= 64 {
                // wavefront size / vgpr hints variam por COV — defaults seguros
                regs = u16::from_le_bytes([data[12], data[13]]).max(1) as u32;
                shared = u32::from_le_bytes(data[48..52].try_into().unwrap_or([0; 4]));
                param_size = u32::from_le_bytes(data[16..20].try_into().unwrap_or([0; 4])).max(16);
            }
        }
    }
    let code = text?;
    Some(KernelImage {
        isa,
        code,
        regs,
        shared,
        barriers: 1,
        param_base: 0,
        param_size,
        ir: IrOrigin::Hsaco,
        is_stub: false,
    })
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
    fn parse_elf_with_text_as_hsaco_shape() {
        // Reusa fixture ELF com .text (machine AMDGPU opcional)
        let mut blob = make_minimal_cubin_fixture();
        blob[18] = (EM_AMDGPU & 0xff) as u8;
        blob[19] = (EM_AMDGPU >> 8) as u8;
        let img = parse(&blob, IsaTag::Gfx1030).expect("hsaco-like");
        assert!(!img.is_stub);
        assert!(!img.code.is_empty());
    }
}
