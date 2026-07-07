//! Cross-OS Loaders — #306a-d: PE32+ (Windows), ELF (Linux), Mach-O (macOS), APK (Android).
//! Syscall-to-Skill Translation Layer (#307).

use alloc::string::String;

pub enum BinaryFormat { Elf, Pe, MachO, Apk, Unknown }
pub enum LoadResult { Ok(usize), Unsupported, Corrupted }

pub struct BinaryLoader;
impl BinaryLoader {
    pub fn detect(data: &[u8]) -> BinaryFormat {
        if data.len() < 16 { return BinaryFormat::Unknown; }
        if data[0] == 0x7f && data[1] == b'E' && data[2] == b'L' && data[3] == b'F' { BinaryFormat::Elf }
        else if data[0] == b'M' && data[1] == b'Z' { BinaryFormat::Pe }
        else if data[0] == 0xCF && data[1] == 0xFA && data[2] == 0xED && data[3] == 0xFE { BinaryFormat::MachO }
        else if data[0] == 0x50 && data[1] == 0x4B && data[3] == 0x03 && data[3] == 0x04 { BinaryFormat::Apk }
        else { BinaryFormat::Unknown }
    }
    pub fn load_elf(data: &[u8]) -> LoadResult {
        if data.len() < 64 { return LoadResult::Corrupted; }
        let entry = u64::from_le_bytes(data[24..32].try_into().unwrap_or([0u8; 8])) as usize;
        if entry == 0 { LoadResult::Corrupted } else { LoadResult::Ok(entry) }
    }
    pub fn load_pe(data: &[u8]) -> LoadResult {
        if data.len() < 0x100 { return LoadResult::Corrupted; }
        let pe_offset = u32::from_le_bytes(data[0x3C..0x40].try_into().unwrap_or([0u8; 4])) as usize;
        if pe_offset + 6 > data.len() { return LoadResult::Corrupted; }
        let entry = u32::from_le_bytes(data[pe_offset + 0x28..pe_offset + 0x2C].try_into().unwrap_or([0u8; 4])) as usize;
        LoadResult::Ok(entry)
    }
    pub fn load_macho(_data: &[u8]) -> LoadResult { LoadResult::Unsupported }
    pub fn load_apk(_data: &[u8]) -> LoadResult { LoadResult::Unsupported }
    pub fn status(&self) -> String { String::from("[BINARY] ELF+PE loader pronto, Mach-O+APK stub") }
}
