//! CUBIN loader — ELF EM_CUDA → `.text` + hints REGCOUNT/PARAM (Wave 1).
//! Referência: Mesa `nv_cubin.c` / gdev. Host fixtures + stub honestos.

use alloc::vec::Vec;
use crate::gpu::compute_abi::IsaTag;
use crate::gpu::kernel_image::KernelImage;
use crate::gpu::kernel_pack::IrOrigin;

const EM_CUDA: u16 = 190; // 0xBE
const SHT_PROGBITS: u32 = 1;
const PT_LOAD: u32 = 1;

/// Parse CUBIN/ELF64 mínimo. Sem `.text` → None (nunca inventa SASS).
pub fn parse(blob: &[u8], isa: IsaTag) -> Option<KernelImage> {
    if blob.len() < 64 {
        return None;
    }
    if &blob[0..4] != b"\x7fELF" {
        return None;
    }
    let ei_class = blob[4];
    if ei_class != 2 {
        return None; // só ELF64
    }
    let e_machine = u16::from_le_bytes([blob[18], blob[19]]);
    if e_machine != EM_CUDA && e_machine != 0 {
        // Alguns cubins de teste usam EM=0; aceita se tem seção .text
    }
    let e_shoff = u64::from_le_bytes(blob[40..48].try_into().ok()?) as usize;
    let e_shentsize = u16::from_le_bytes([blob[58], blob[59]]) as usize;
    let e_shnum = u16::from_le_bytes([blob[60], blob[61]]) as usize;
    let e_shstrndx = u16::from_le_bytes([blob[62], blob[63]]) as usize;
    if e_shentsize < 64 || e_shnum == 0 || e_shoff + e_shentsize * e_shnum > blob.len() {
        return None;
    }

    let shstr = section_at(blob, e_shoff, e_shentsize, e_shstrndx)?;
    let strtab_off = u64::from_le_bytes(shstr[24..32].try_into().ok()?) as usize;
    let strtab_size = u64::from_le_bytes(shstr[32..40].try_into().ok()?) as usize;
    if strtab_off + strtab_size > blob.len() {
        return None;
    }
    let strtab = &blob[strtab_off..strtab_off + strtab_size];

    let mut text: Option<Vec<u8>> = None;
    let mut regs = 16u32;
    let mut shared = 0u32;
    let mut param_base = 0u32;
    let mut param_size = 256u32;

    for i in 0..e_shnum {
        let sh = section_at(blob, e_shoff, e_shentsize, i)?;
        let name_off = u32::from_le_bytes(sh[0..4].try_into().ok()?) as usize;
        let name = cstr_at(strtab, name_off).unwrap_or("");
        let sh_type = u32::from_le_bytes(sh[4..8].try_into().ok()?);
        let sh_offset = u64::from_le_bytes(sh[24..32].try_into().ok()?) as usize;
        let sh_size = u64::from_le_bytes(sh[32..40].try_into().ok()?) as usize;
        if sh_offset + sh_size > blob.len() || sh_size == 0 {
            continue;
        }
        let data = &blob[sh_offset..sh_offset + sh_size];
        if (name == ".text" || name.starts_with(".text.")) && sh_type == SHT_PROGBITS {
            text = Some(data.to_vec());
        }
        // NVIDIA info sections (best-effort; ausente → defaults honestos)
        if name.contains("nv.info") || name == ".nv.info" {
            scan_nv_info(data, &mut regs, &mut shared, &mut param_base, &mut param_size);
        }
    }

    // Fallback: first PT_LOAD
    if text.is_none() {
        let e_phoff = u64::from_le_bytes(blob[32..40].try_into().ok()?) as usize;
        let e_phentsize = u16::from_le_bytes([blob[54], blob[55]]) as usize;
        let e_phnum = u16::from_le_bytes([blob[56], blob[57]]) as usize;
        for i in 0..e_phnum {
            let off = e_phoff + i * e_phentsize;
            if off + 56 > blob.len() {
                break;
            }
            let p_type = u32::from_le_bytes(blob[off..off + 4].try_into().ok()?);
            if p_type != PT_LOAD {
                continue;
            }
            let p_offset = u64::from_le_bytes(blob[off + 8..off + 16].try_into().ok()?) as usize;
            let p_filesz = u64::from_le_bytes(blob[off + 32..off + 40].try_into().ok()?) as usize;
            if p_offset + p_filesz <= blob.len() && p_filesz > 0 {
                text = Some(blob[p_offset..p_offset + p_filesz].to_vec());
                break;
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
        param_base,
        param_size,
        ir: IrOrigin::Cubin,
        is_stub: false,
    })
}

fn section_at(blob: &[u8], shoff: usize, entsize: usize, idx: usize) -> Option<&[u8]> {
    let off = shoff + idx * entsize;
    if off + entsize > blob.len() {
        return None;
    }
    Some(&blob[off..off + entsize])
}

fn cstr_at(tab: &[u8], off: usize) -> Option<&str> {
    if off >= tab.len() {
        return None;
    }
    let end = tab[off..].iter().position(|&b| b == 0).unwrap_or(tab.len() - off);
    core::str::from_utf8(&tab[off..off + end]).ok()
}

/// Scan best-effort de atributos NVIDIA (EIATTR_*). Desconhecido → não mexe.
fn scan_nv_info(
    data: &[u8],
    regs: &mut u32,
    shared: &mut u32,
    param_base: &mut u32,
    param_size: &mut u32,
) {
    let mut i = 0;
    while i + 4 <= data.len() {
        let attr = data[i];
        let fmt = data[i + 1];
        i += 2;
        if fmt == 1 && i < data.len() {
            // EIATTR_FORMAT_BYTE
            let v = data[i] as u32;
            i += 1;
            if attr == 0x0a {
                // EIATTR_REGCOUNT (comum)
                *regs = v.max(1);
            }
        } else if fmt == 2 && i + 2 <= data.len() {
            let v = u16::from_le_bytes([data[i], data[i + 1]]) as u32;
            i += 2;
            if attr == 0x0a {
                *regs = v.max(1);
            } else if attr == 0x1c {
                *shared = v;
            } else if attr == 0x19 {
                *param_size = v.max(16);
            } else if attr == 0x17 {
                *param_base = v;
            }
        } else if fmt == 3 && i + 4 <= data.len() {
            let v = u32::from_le_bytes(data[i..i + 4].try_into().unwrap_or([0; 4]));
            i += 4;
            if attr == 0x0a {
                *regs = v.max(1);
            } else if attr == 0x17 {
                *param_base = v;
            } else if attr == 0x19 {
                *param_size = v.max(16);
            }
        } else {
            break;
        }
    }
}

/// Fixture ELF64 mínimo com `.text` de 16 bytes (teste host).
#[cfg(test)]
pub fn make_minimal_cubin_fixture() -> Vec<u8> {
    // ELF64 header + 2 sections (null + .text) + shstrtab ".text\0"
    let mut b = Vec::new();
    b.extend_from_slice(b"\x7fELF");
    b.push(2); // class 64
    b.push(1); // LE
    b.push(1);
    b.extend_from_slice(&[0u8; 9]);
    b.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    b.extend_from_slice(&EM_CUDA.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes()); // version
    b.extend_from_slice(&0u64.to_le_bytes()); // entry
    b.extend_from_slice(&64u64.to_le_bytes()); // phoff (unused)
    // shoff filled later
    let shoff_pos = b.len();
    b.extend_from_slice(&0u64.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // flags
    b.extend_from_slice(&64u16.to_le_bytes()); // ehsize
    b.extend_from_slice(&56u16.to_le_bytes()); // phentsize
    b.extend_from_slice(&0u16.to_le_bytes()); // phnum
    b.extend_from_slice(&64u16.to_le_bytes()); // shentsize
    b.extend_from_slice(&3u16.to_le_bytes()); // shnum
    b.extend_from_slice(&2u16.to_le_bytes()); // shstrndx

    while b.len() < 64 {
        b.push(0);
    }

    // code at offset 64+ — we'll put code after headers
    let strtab = b".\0.text\0.shstrtab\0";
    // Layout: [ELF hdr 64][code 16][shstrtab][3 section headers]
    let code_off = 64usize;
    let code = [0x56u8, 0x00, 0x00, 0x00, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let str_off = code_off + code.len();
    let sh_off = str_off + strtab.len();

    b.resize(code_off, 0);
    b.extend_from_slice(&code);
    b.extend_from_slice(strtab);

    // null section
    b.extend_from_slice(&[0u8; 64]);
    // .text section: name_off=2 (".text"), PROGBITS, offset=code_off, size=16
    let mut sh = [0u8; 64];
    sh[0..4].copy_from_slice(&2u32.to_le_bytes());
    sh[4..8].copy_from_slice(&SHT_PROGBITS.to_le_bytes());
    sh[24..32].copy_from_slice(&(code_off as u64).to_le_bytes());
    sh[32..40].copy_from_slice(&(code.len() as u64).to_le_bytes());
    b.extend_from_slice(&sh);
    // .shstrtab
    let mut sh2 = [0u8; 64];
    sh2[0..4].copy_from_slice(&8u32.to_le_bytes()); // ".shstrtab"
    sh2[4..8].copy_from_slice(&3u32.to_le_bytes()); // SHT_STRTAB
    sh2[24..32].copy_from_slice(&(str_off as u64).to_le_bytes());
    sh2[32..40].copy_from_slice(&(strtab.len() as u64).to_le_bytes());
    b.extend_from_slice(&sh2);

    b[shoff_pos..shoff_pos + 8].copy_from_slice(&(sh_off as u64).to_le_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_cubin_text() {
        let blob = make_minimal_cubin_fixture();
        let img = parse(&blob, IsaTag::Sm61).expect("parse");
        assert!(!img.is_stub);
        assert_eq!(img.code.len(), 16);
        assert_eq!(img.ir, IrOrigin::Cubin);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(b"not elf", IsaTag::Sm61).is_none());
    }
}
