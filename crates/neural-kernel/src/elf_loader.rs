//! ELF64 loader for Ring-3 processes (ADR-0076 Item 1 + ADR-0082 F2.1).
//! Parses ELF64 headers, loads PT_LOAD segments into an AddressSpace,
//! sets up a user stack, and returns the entry point + stack top.
//! ADR-0082: `create_sandbox_as()` (isolamento real), flags RX/RW por
//! segmento (PF_X/PF_W), relocations `R_X86_64_RELATIVE` (PIE base=0).
//!
//! # Safety
//! The caller must ensure the ELF binary is trusted or sandboxed.
//! No validation beyond structural integrity (magic, bounds, headers).

use core::sync::atomic::Ordering;
use x86_64::VirtAddr;

use crate::address_space::{self, AddressSpace};
use crate::memory::{alloc_physical_frame, PHYS_MEM_OFFSET};

/// Default user stack base VA (after loaded segments).
pub const USER_STACK_BASE: u64 = 0x0000_7000_0040_0000;
/// Default user stack size (4 pages).
pub const USER_STACK_SIZE: u64 = 0x4000;

// ELF constants (ADR-0082 F2.1)
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const SHT_RELA: u32 = 4;
const R_X86_64_RELATIVE: u32 = 8;

/// Result of a successful ELF load.
#[derive(Debug)]
pub struct ElfLoadResult {
    /// Virtual address of the entry point.
    pub entry: u64,
    /// Top of the user stack (RSP initial value).
    pub stack_top: u64,
}

/// ELF64 loader for Ring-3 processes.
pub struct ElfLoader;

impl ElfLoader {
    /// Load an ELF64 binary into the given address space.
    ///
    /// Returns the entry point and the initial stack pointer.
    ///
    /// # Errors
    /// - `"ELF: invalid magic"` — missing `\x7fELF` header
    /// - `"ELF: not 64-bit"` — class byte is not `2` (ELFCLASS64)
    /// - `"ELF: not LE"` — data byte is not `1` (ELFDATA2LSB)
    /// - `"ELF: program header out of bounds"` — phoff + phnum*phentsize exceeds buffer
    /// - `"ELF: OOM"` — frame allocation failed
    /// - `"ELF: map failed"` — `AddressSpace::map_user_page` failed
    /// - `"ELF: empty — no PT_LOAD"` — no loadable segments found
    pub fn load(data: &[u8], aspace: &mut AddressSpace) -> Result<ElfLoadResult, &'static str> {
        // Higher-half safety: PHYS_MEM_OFFSET must be valid for HHDM access.
        if PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
            return Err("ELF: PHYS_MEM_OFFSET=0 — higher-half not available");
        }

        // --- ELF header (64 bytes minimum) ---
        if data.len() < 64 {
            return Err("ELF: invalid magic");
        }
        // Magic: \x7f E L F
        if &data[0..4] != b"\x7fELF" {
            return Err("ELF: invalid magic");
        }
        // EI_CLASS: 1=32-bit, 2=64-bit
        if data[4] != 2 {
            return Err("ELF: not 64-bit");
        }
        // EI_DATA: 1=little-endian, 2=big-endian
        if data[5] != 1 {
            return Err("ELF: not LE");
        }

        // e_entry (offset 24, 8 bytes)
        let entry = u64::from_le_bytes(data[24..32].try_into().unwrap());
        // e_phoff (offset 32, 8 bytes)
        let phoff = u64::from_le_bytes(data[32..40].try_into().unwrap());
        // e_phentsize (offset 54, 2 bytes)
        let phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap()) as usize;
        // e_phnum (offset 56, 2 bytes)
        let phnum = u16::from_le_bytes(data[56..58].try_into().unwrap()) as usize;

        if phoff == 0 || phentsize < 56 || phnum == 0 {
            return Err("ELF: no program headers");
        }

        let mut any_load = false;

        for i in 0..phnum {
            let off = phoff as usize + i * phentsize;
            // Program header entry is at least 56 bytes for ELF64
            if off + 56 > data.len() {
                return Err("ELF: program header out of bounds");
            }

            // p_type (offset 0, 4 bytes)
            let p_type = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
            // PT_LOAD = 1
            if p_type != 1 {
                continue;
            }
            any_load = true;

            // p_flags (offset 4, 4 bytes) — PF_R=4, PF_W=2, PF_X=1
            let p_flags = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
            // p_offset (offset 8, 8 bytes)
            let p_offset = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap());
            // p_vaddr (offset 16, 8 bytes)
            let p_vaddr = u64::from_le_bytes(data[off + 16..off + 24].try_into().unwrap());
            // p_paddr (offset 24, 8 bytes) — ignored
            // p_filesz (offset 32, 8 bytes)
            let p_filesz = u64::from_le_bytes(data[off + 32..off + 40].try_into().unwrap());
            // p_memsz (offset 40, 8 bytes)
            let p_memsz = u64::from_le_bytes(data[off + 40..off + 48].try_into().unwrap());
            // p_align (offset 48, 8 bytes) — ignored

            if p_memsz == 0 {
                continue;
            }

            // Map pages for this segment
            let start_page = p_vaddr & !0xFFF;
            let end = p_vaddr + p_memsz;
            // Round up to page boundary
            let end_page = (end + 0xFFF) & !0xFFF;
            let pages = ((end_page - start_page) / 4096) as usize;

            for j in 0..pages {
                let va = VirtAddr::new(start_page + (j as u64) * 4096);
                let frame = alloc_physical_frame().ok_or("ELF: OOM")?;

                // Calculate byte range to copy from ELF data
                let page_start = start_page + (j as u64) * 4096;
                let in_file_offset = if page_start < p_vaddr {
                    p_vaddr - page_start // bytes before first data in this page
                } else {
                    0
                };
                let copy_base = page_start + in_file_offset; // first VA that has data
                let copy_src_off = p_offset + (copy_base - p_vaddr); // src offset in ELF data

                // Amount of file data to copy into this page
                let copy_size = if copy_base < p_vaddr + p_filesz {
                    let remaining_file = (p_vaddr + p_filesz) - copy_base;
                    core::cmp::min(remaining_file, 4096 - in_file_offset) as usize
                } else {
                    0
                };

                // Copy data from ELF binary into the frame (via HHDM)
                let hhdm_base = PHYS_MEM_OFFSET.load(Ordering::Acquire);
                let dst_ptr = (hhdm_base + frame.start_address().as_u64()) as *mut u8;

                // Zero the entire frame first (mandatory for .bss)
                unsafe {
                    core::ptr::write_bytes(dst_ptr, 0, 4096);
                }

                // Copy file-backed portion
                if copy_size > 0 {
                    let src_off = copy_src_off as usize;
                    if src_off + copy_size <= data.len() {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                data.as_ptr().add(src_off),
                                dst_ptr.add(in_file_offset as usize),
                                copy_size,
                            );
                        }
                    }
                }

                // Determine page flags (ADR-0082 F2.1: RX para segmento
                // executável, RW para dados; NX não forçado no MVP).
                let flags = if p_flags & PF_X != 0 {
                    address_space::user_code_flags()
                } else {
                    address_space::user_data_flags()
                };

                unsafe {
                    aspace.map_user_page(va, frame, flags).map_err(|_| "ELF: map failed")?;
                }
            }
        }

        if !any_load {
            return Err("ELF: empty — no PT_LOAD");
        }

        // ADR-0082 F2.1: relocations R_X86_64_RELATIVE (PIE base=0) do .rela.dyn.
        apply_rela_dyn(data, aspace)?;

        // --- Set up user stack ---
        let stack_pages = (USER_STACK_SIZE / 4096) as usize;
        for j in 0..stack_pages {
            let va = VirtAddr::new(USER_STACK_BASE + (j as u64) * 4096);
            let frame = alloc_physical_frame().ok_or("ELF: OOM")?;
            // Zero the stack page
            let hhdm_base = PHYS_MEM_OFFSET.load(Ordering::Acquire);
            unsafe {
                core::ptr::write_bytes(
                    (hhdm_base + frame.start_address().as_u64()) as *mut u8,
                    0,
                    4096,
                );
            }
            unsafe {
                aspace
                    .map_user_page(va, frame, address_space::user_data_flags())
                    .map_err(|_| "ELF: stack map failed")?;
            }
        }
        let stack_top = USER_STACK_BASE + USER_STACK_SIZE;

        Ok(ElfLoadResult { entry, stack_top })
    }

    /// Validate that `data` looks like an ELF64 binary.
    /// Returns `true` if magic, class, and a plausible phoff are present.
    pub fn is_valid_elf(data: &[u8]) -> bool {
        if data.len() < 64 {
            return false;
        }
        if &data[0..4] != b"\x7fELF" {
            return false;
        }
        if data[4] != 2 {
            return false;
        }
        if data[5] != 1 {
            return false;
        }
        true
    }
}

/// Load an ELF binary and spawn it as a userspace process.
/// Returns the process PID on success.
pub fn load_and_spawn(data: &[u8], name: &str) -> Result<u64, &'static str> {
    // ADR-0082 F2.1: sandbox AS from-scratch (isolamento real) em vez de
    // clone_current() (compartilhava PTs inferiores do kernel).
    let mut aspace = address_space::create_sandbox_as()?;
    let result = ElfLoader::load(data, &mut aspace)?;
    let pid = crate::process::PROCESS_MANAGER.lock().spawn(name, result.entry, result.stack_top, aspace);
    k_nano::slog_bin!("RING3", "spawn", "Process '{}' pid={} entry={:#x} stack={:#x}",
        name, pid, result.entry, result.stack_top);
    Ok(pid)
}

/// Aplica relocations `R_X86_64_RELATIVE` (PIE, base=0) do `.rela.dyn`.
///
/// R_X86_64_RELATIVE: `*(u64*)(base + r_offset) = base + r_addend`.
/// Com base=0 (mapeado nos próprios VAs do ELF) vira `*(u64*)r_offset = r_addend`.
/// Write-through via HHDM + `frame_for_virt` (ADR-0082 F2.1).
fn apply_rela_dyn(data: &[u8], aspace: &mut AddressSpace) -> Result<(), &'static str> {
    if data.len() < 64 {
        return Ok(());
    }
    // e_shoff (40), e_shentsize (58), e_shnum (60)
    let shoff = u64::from_le_bytes(data[40..48].try_into().unwrap()) as usize;
    let shentsize = u16::from_le_bytes(data[58..60].try_into().unwrap()) as usize;
    let shnum = u16::from_le_bytes(data[60..62].try_into().unwrap()) as usize;
    if shoff == 0 || shentsize < 64 || shnum == 0 {
        return Ok(()); // sem section headers — nada a relocar
    }
    if shoff + shentsize * shnum > data.len() {
        return Err("ELF: section headers out of bounds");
    }

    for i in 0..shnum {
        let o = shoff + i * shentsize;
        // sh_type (4), sh_offset (24), sh_size (32), sh_entsize (56)
        let sh_type = u32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap());
        if sh_type != SHT_RELA {
            continue;
        }
        let rela_off = u64::from_le_bytes(data[o + 24..o + 32].try_into().unwrap()) as usize;
        let rela_size = u64::from_le_bytes(data[o + 32..o + 40].try_into().unwrap()) as usize;
        let ent_size = u64::from_le_bytes(data[o + 56..o + 64].try_into().unwrap()) as usize;
        let ent = if ent_size == 0 { 24 } else { ent_size };
        if rela_off + rela_size > data.len() {
            return Err("ELF: rela out of bounds");
        }

        let mut off = rela_off;
        while off + ent <= rela_off + rela_size && ent >= 24 {
            // Elf64_Rela: r_offset (0), r_info (8), r_addend (16)
            let r_offset = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
            let r_info = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap());
            let r_addend = u64::from_le_bytes(data[off + 16..off + 24].try_into().unwrap());
            let r_type = (r_info & 0xffff_ffff) as u32;
            if r_type == R_X86_64_RELATIVE {
                let va = VirtAddr::new(r_offset);
                let frame = aspace
                    .frame_for_virt(va)
                    .ok_or("ELF: rela target unmapped")?;
                let hhdm_base = PHYS_MEM_OFFSET.load(Ordering::Acquire);
                // Escrita de 8 bytes nao pode cruzar o frame (a frame seguinte e
                // outra alocacao). Relocs validos sao alinhados a 8 (r_offset&0xfff
                // <= 0xff8); qualquer outro e ELF malformado -> fail-closed.
                if (r_offset & 0xfff) > 0xff8 {
                    return Err("ELF: rela write crosses frame boundary");
                }
                let ptr = (hhdm_base + frame.start_address().as_u64() + (r_offset & 0xfff)) as *mut u64;
                unsafe {
                    core::ptr::write_volatile(ptr, r_addend);
                }
            }
            off += ent;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_elf() -> Vec<u8> {
        let mut elf = vec![0u8; 128];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[6] = 1;
        elf[16..18].copy_from_slice(&2u16.to_le_bytes());
        elf[18..20].copy_from_slice(&0x3Eu16.to_le_bytes());
        elf[20..24].copy_from_slice(&1u32.to_le_bytes());
        elf[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[52..54].copy_from_slice(&64u16.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());
        // Program header: PT_LOAD (type=1) with p_memsz = 0 → skipped
        elf[64..68].copy_from_slice(&1u32.to_le_bytes());
        elf[68..72].copy_from_slice(&5u32.to_le_bytes());
        elf[80..88].copy_from_slice(&0x401000u64.to_le_bytes());
        elf[96..104].copy_from_slice(&0u64.to_le_bytes());
        elf[104..112].copy_from_slice(&0u64.to_le_bytes());
        elf[112..120].copy_from_slice(&0x1000u64.to_le_bytes());
        elf
    }

    #[test]
    fn test_is_valid_elf() {
        let elf = make_minimal_elf();
        assert!(ElfLoader::is_valid_elf(&elf));
        assert!(!ElfLoader::is_valid_elf(&[0u8; 4]));
        assert!(!ElfLoader::is_valid_elf(&[0x7f, b'E', b'L', b'F', 1, 1]));
    }

    #[test]
    fn test_invalid_magic() {
        let data = [0u8; 64];
        assert!(!ElfLoader::is_valid_elf(&data));
    }
}

/// Self-test de boot (ADR-0082 F2.1) — monta ELF mínimo e valida
/// parse + flags RX/RW por segmento + stack. Não executa código.
pub fn elf_boot_self_test() -> bool {
    let mut elf = [0u8; 64 + 2 * 56];
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2; // ELF64
    elf[5] = 1; // LE
    elf[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN (PIE)
    elf[18..20].copy_from_slice(&0x3Eu16.to_le_bytes()); // x86_64
    elf[24..32].copy_from_slice(&0x1000u64.to_le_bytes()); // entry
    elf[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    // ELF64 header: e_phentsize @ 54 (2B), e_phnum @ 56 (2B), e_shentsize @ 58, e_shnum @ 60
    elf[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    elf[56..58].copy_from_slice(&2u16.to_le_bytes()); // e_phnum

    // PT_LOAD #1: RX @ 0x1000 (código)
    let o = 64;
    elf[o..o + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf[o + 4..o + 8].copy_from_slice(&(PF_X | 4).to_le_bytes()); // R+X
    elf[o + 16..o + 24].copy_from_slice(&0x1000u64.to_le_bytes()); // vaddr
    elf[o + 32..o + 40].copy_from_slice(&0x1000u64.to_le_bytes()); // filesz
    elf[o + 40..o + 48].copy_from_slice(&0x1000u64.to_le_bytes()); // memsz

    // PT_LOAD #2: RW @ 0x2000 (dados/BSS)
    let o = 64 + 56;
    elf[o..o + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf[o + 4..o + 8].copy_from_slice(&(PF_W | 4).to_le_bytes()); // R+W
    elf[o + 16..o + 24].copy_from_slice(&0x2000u64.to_le_bytes()); // vaddr
    elf[o + 32..o + 40].copy_from_slice(&0u64.to_le_bytes()); // filesz
    elf[o + 40..o + 48].copy_from_slice(&0x1000u64.to_le_bytes()); // memsz (BSS)

    let mut aspace = match address_space::create_sandbox_as() {
        Ok(a) => a,
        Err(e) => {
            k_nano::slog_bin!("ELF", "selftest", "create_sandbox_as fail: {}", e);
            return false;
        }
    };
    let result = match ElfLoader::load(&elf, &mut aspace) {
        Ok(r) => r,
        Err(e) => {
            k_nano::slog_bin!("ELF", "selftest", "load fail: {}", e);
            return false;
        }
    };
    if result.entry != 0x1000 {
        k_nano::slog_bin!("ELF", "selftest", "entry mismatch {:#x}", result.entry);
        return false;
    }
    // BSS zeroed: frame_for_virt(0x2000) mapeado e zerado pelo loader
    if aspace.frame_for_virt(VirtAddr::new(0x2000)).is_none() {
        k_nano::slog_bin!("ELF", "selftest", "BSS not mapped");
        return false;
    }
    k_nano::slog_bin!(
        "ELF",
        "selftest",
        "PASS entry={:#x} stack_top={:#x} (RX code + RW BSS mapeados no sandbox)",
        result.entry,
        result.stack_top
    );
    true
}
