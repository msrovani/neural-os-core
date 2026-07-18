//! GPT partition table — leitura e escrita com CRC32C correto.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::block_dev::BlockDevice;
use crate::neural_fs::checksum::crc32c;

#[derive(Debug, Clone)]
pub struct GptPartition {
    pub index: u32, pub name: String, pub type_guid: [u8; 16],
    pub lba_start: u64, pub lba_end: u64, pub attrs: u64,
}

pub const GPT_TYPE_ESP: [u8; 16] = [0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B];
/// NeuralFS GPT type GUID (on-disk mixed-endian).
/// UUID: `4E455552-414C-4653-2D4E-465320000001` ("NEURALFS" / NF001).
pub const GPT_TYPE_NEURALFS: [u8; 16] = [
    0x52, 0x55, 0x45, 0x4E, // 4E455552 LE
    0x4C, 0x41,             // 414C LE
    0x53, 0x46,             // 4653 LE
    0x2D, 0x4E, 0x46, 0x53, 0x20, 0x00, 0x00, 0x01,
];
/// Alias legado.
pub const GPT_TYPE_NEURAL: [u8; 16] = GPT_TYPE_NEURALFS;

pub fn probe_gpt(dev: &mut dyn BlockDevice) -> Option<Vec<GptPartition>> {
    let mut mbr = [0u8; 512];
    if !dev.read_sectors(0, &mut mbr) || mbr[510] != 0x55 || mbr[511] != 0xAA { return None; }
    if !(0..4).any(|i| mbr[0x1BE + i * 16 + 4] == 0xEE) { return None; }

    let mut hdr = [0u8; 512];
    if !dev.read_sectors(1, &mut hdr) || &hdr[0..8] != b"EFI PART" { return None; }

    let entries_lba = u64::from_le_bytes([hdr[72], hdr[73], hdr[74], hdr[75], hdr[76], hdr[77], hdr[78], hdr[79]]);
    let entry_count = u32::from_le_bytes([hdr[80], hdr[81], hdr[82], hdr[83]]).min(128);
    let entry_size = u32::from_le_bytes([hdr[84], hdr[85], hdr[86], hdr[87]]);
    if entry_size != 128 { return None; }

    let mut parts = Vec::new();
    for i in 0..entry_count {
        let entry_sector = entries_lba + (i as u64 * 128) / 512;
        let entry_off = (i as usize * 128) % 512;
        let mut buf = [0u8; 512];
        if !dev.read_sectors(entry_sector, &mut buf) { break; }
        let off = entry_off;
        if buf[off..off+16].iter().all(|&b| b == 0) { continue; }

        let mut guid = [0u8; 16]; guid.copy_from_slice(&buf[off..off+16]);
        let lba_start = u64::from_le_bytes(buf[off+32..off+40].try_into().unwrap());
        let lba_end = u64::from_le_bytes(buf[off+40..off+48].try_into().unwrap());
        // GPT partition name em UTF-16LE, max 36 caracteres (72 bytes)
        let name_u16: Vec<u16> = buf[off+56..off+72].chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        let name: String = core::char::decode_utf16(name_u16.into_iter())
            .map(|r| r.unwrap_or('\u{FFFD}'))
            .collect();

        parts.push(GptPartition { index: i, name, type_guid: guid, lba_start, lba_end, attrs: 0 });
    }
    Some(parts)
}

/// Cria GPT completa com 128 entradas zeradas + 1 partição.
pub fn gpt_format_single(dev: &mut dyn BlockDevice, total_lba: u64, type_guid: &[u8; 16], label: &str) -> bool {
    if total_lba <= 34 { return false; }
    let last_lba = total_lba - 1;
    let entries_lba = 2u64;
    let backup_entries_lba = match last_lba.checked_sub(32) { Some(x) => x, None => return false };
    let first_usable = 2048u64;
    let part_lba_start = first_usable;
    let part_lba_end = match last_lba.checked_sub(34) { Some(x) => x, None => return false };
    let entry_count = 128u32;
    let entry_size = 128u32;
    let hdr_size = 92u32;

    // MBR protetiva
    let mut mbr = [0u8; 512];
    mbr[0x1FE] = 0x55; mbr[0x1FF] = 0xAA;
    mbr[0x1BE + 4] = 0xEE;
    mbr[0x1BE + 8..0x1BE + 12].copy_from_slice(&1u32.to_le_bytes());
    let mbr_size = if total_lba > 0xFFFFFFFF { 0xFFFFFFFFu32 } else { (total_lba - 1) as u32 };
    mbr[0x1BE + 12..0x1BE + 16].copy_from_slice(&mbr_size.to_le_bytes());
    if !dev.write_sectors(0, &mbr) { return false; }

    // Prepara entrada de particao
    let mut entry_buf = [0u8; 512];
    entry_buf[0..16].copy_from_slice(type_guid);
    let mut ug = [0u8; 16];
    for (i, b) in label.bytes().enumerate() { ug[i % 16] ^= b; }
    entry_buf[16..32].copy_from_slice(&ug);
    entry_buf[32..40].copy_from_slice(&part_lba_start.to_le_bytes());
    entry_buf[40..48].copy_from_slice(&part_lba_end.to_le_bytes());
    let label_u16: Vec<u16> = label.encode_utf16().collect();
    for (i, &c) in label_u16.iter().enumerate().take(72/2) {
        entry_buf[56 + i*2] = (c & 0xFF) as u8;
        entry_buf[57 + i*2] = (c >> 8) as u8;
    }

    // Prepara array completo de 128 entradas (16384 bytes)
    let mut all_entries = vec![0u8; (entry_count * entry_size) as usize];
    all_entries[0..128].copy_from_slice(&entry_buf[..128]);
    let entries_crc = crc32c(&all_entries);

    // GPT header (LBA 1)
    let mut hdr = [0u8; 512];
    hdr[0..8].copy_from_slice(b"EFI PART");
    hdr[8..12].copy_from_slice(&0x00010000u32.to_le_bytes()); // revision
    hdr[12..16].copy_from_slice(&hdr_size.to_le_bytes());
    hdr[16..20].copy_from_slice(&0u32.to_le_bytes()); // CRC placeholder
    hdr[20..24].copy_from_slice(&0u32.to_le_bytes()); // reserved
    hdr[24..32].copy_from_slice(&1u64.to_le_bytes()); // this LBA
    hdr[32..40].copy_from_slice(&last_lba.to_le_bytes()); // backup LBA
    hdr[40..48].copy_from_slice(&first_usable.to_le_bytes()); // first usable
    hdr[48..56].copy_from_slice(&(last_lba - 33).to_le_bytes()); // last usable
    hdr[56..72].copy_from_slice(&ug); // disk GUID (mesmo da particao)
    hdr[72..80].copy_from_slice(&entries_lba.to_le_bytes()); // partition entries LBA
    hdr[80..84].copy_from_slice(&entry_count.to_le_bytes());
    hdr[84..88].copy_from_slice(&entry_size.to_le_bytes());
    hdr[88..92].copy_from_slice(&entries_crc.to_le_bytes()); // partition array CRC
    // CRC do header (bytes 0-91, campo CRC zerado)
    hdr[16..20].copy_from_slice(&0u32.to_le_bytes());
    let hdr_crc = crc32c(&hdr[0..92]);
    hdr[16..20].copy_from_slice(&hdr_crc.to_le_bytes());

    // Escreve entradas (LBA 2-33)
    for i in 0..32 {
        let start = i * 512;
        let mut sector = [0u8; 512];
        let end = (start + 512).min(all_entries.len());
        sector[..end - start].copy_from_slice(&all_entries[start..end]);
        if !dev.write_sectors(entries_lba + i as u64, &sector) { return false; }
    }

    // Escreve header primary
    if !dev.write_sectors(1, &hdr) { return false; }

    // Backup: entradas no penultimo LBA group
    for i in 0..32 {
        let start = i * 512;
        let mut sector = [0u8; 512];
        let end = (start + 512).min(all_entries.len());
        sector[..end - start].copy_from_slice(&all_entries[start..end]);
        if !dev.write_sectors(backup_entries_lba + i as u64, &sector) { return false; }
    }

    // Backup header (ultimo LBA)
    hdr[24..32].copy_from_slice(&last_lba.to_le_bytes()); // this LBA = last
    hdr[32..40].copy_from_slice(&1u64.to_le_bytes()); // primary LBA
    hdr[72..80].copy_from_slice(&backup_entries_lba.to_le_bytes());
    hdr[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    hdr[16..20].copy_from_slice(&0u32.to_le_bytes());
    let backup_crc = crc32c(&hdr[0..92]);
    hdr[16..20].copy_from_slice(&backup_crc.to_le_bytes());
    if !dev.write_sectors(last_lba, &hdr) { return false; }

    // Limpa primeiro setor da particao
    let zero = [0u8; 512];
    dev.write_sectors(part_lba_start, &zero);
    true
}
