//! GPT partition table — leitura e escrita.
//! Suporta criar partições, listar, deletar.
//! Usado pelo instalador neural e pelo app FS Manager.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::block_dev::BlockDevice;

#[derive(Debug, Clone)]
pub struct GptPartition {
    pub index: u32,
    pub name: String,
    pub type_guid: [u8; 16],
    pub lba_start: u64,
    pub lba_end: u64,
    pub attrs: u64,
}

/// GUIDs de tipo de partição conhecidos
pub const GPT_TYPE_ESP: [u8; 16] = [0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B];
pub const GPT_TYPE_NTFS: [u8; 16] = [0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7];
pub const GPT_TYPE_LINUX: [u8; 16] = [0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4];
pub const GPT_TYPE_NEURAL: [u8; 16] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00];

/// Sonda GPT e retorna lista de partições
pub fn probe_gpt(dev: &mut dyn BlockDevice) -> Option<Vec<GptPartition>> {
    let mut mbr = [0u8; 512];
    if !dev.read_sectors(0, &mut mbr) { return None; }
    if mbr[510] != 0x55 || mbr[511] != 0xAA { return None; }
    if !(0..4).any(|i| mbr[0x1BE + i * 16 + 4] == 0xEE) { return None; }

    let mut hdr = [0u8; 512];
    if !dev.read_sectors(1, &mut hdr) { return None; }
    if &hdr[0..8] != b"EFI PART" { return None; }

    let entries_lba = u64::from_le_bytes([hdr[72], hdr[73], hdr[74], hdr[75], hdr[76], hdr[77], hdr[78], hdr[79]]);
    let entry_count = u32::from_le_bytes([hdr[80], hdr[81], hdr[82], hdr[83]]);
    if entry_count > 128 { return None; }

    let mut parts = Vec::new();
    for i in 0..entry_count.min(128) {
        let entry_sector = entries_lba + (i as u64 * 128) / 512;
        let entry_off = (i as usize * 128) % 512;
        let mut buf = [0u8; 512];
        if !dev.read_sectors(entry_sector, &mut buf) { break; }
        let off = entry_off;
        let type_guid = &buf[off..off+16];
        if type_guid.iter().all(|&b| b == 0) { continue; }

        let lba_start = u64::from_le_bytes([buf[off+32], buf[off+33], buf[off+34], buf[off+35],
            buf[off+36], buf[off+37], buf[off+38], buf[off+39]]);
        let lba_end = u64::from_le_bytes([buf[off+40], buf[off+41], buf[off+42], buf[off+43],
            buf[off+44], buf[off+45], buf[off+46], buf[off+47]]);
        let attrs = u64::from_le_bytes([buf[off+56], buf[off+57], buf[off+58], buf[off+59],
            buf[off+60], buf[off+61], buf[off+62], buf[off+63]]);
        let name_utf16 = &buf[off+64..off+108];
        let name = String::from_utf16le(name_utf16).unwrap_or(String::new());
        let name = name.trim_end_matches('\0').to_string();
        let name = alloc::string::String::from(name.trim_end_matches('\0'));

        let mut guid = [0u8; 16];
        guid.copy_from_slice(type_guid);

        parts.push(GptPartition {
            index: i,
            name,
            type_guid: guid,
            lba_start,
            lba_end,
            attrs,
        });
    }
    Some(parts)
}

/// Cria tabela GPT com uma partição ocupando todo o espaço disponível
pub fn gpt_format_single(dev: &mut dyn BlockDevice, total_lba: u64, type_guid: &[u8; 16], label: &str) -> bool {
    let lba_end = total_lba - 34; // reserva GPT secundária no final

    // Cria MBR protetiva
    let mut mbr = [0u8; 512];
    mbr[0x1FE] = 0x55; mbr[0x1FF] = 0xAA;
    mbr[0x1BE + 4] = 0xEE; // GPT protetiva
    // LBA start = 1, size = total_lba - 1
    let mbr_start = 1u32;
    let mbr_size = (total_lba - 1) as u32;
    mbr[0x1BE + 8..0x1BE + 12].copy_from_slice(&mbr_start.to_le_bytes());
    mbr[0x1BE + 12..0x1BE + 16].copy_from_slice(&mbr_size.to_le_bytes());
    if !dev.write_sectors(0, &mbr) { return false; }

    // Cria GPT header no LBA 1
    let mut hdr = [0u8; 512];
    hdr[0..8].copy_from_slice(b"EFI PART");
    let revision = 0x00010000u32;
    hdr[8..12].copy_from_slice(&revision.to_le_bytes());
    let hdr_size = 92u32;
    hdr[12..16].copy_from_slice(&hdr_size.to_le_bytes());
    let crc_start = 16u32;
    hdr[16..20].copy_from_slice(&crc_start.to_le_bytes());
    hdr[24..32].copy_from_slice(&1u64.to_le_bytes()); // this LBA
    hdr[32..40].copy_from_slice(&(total_lba - 1).to_le_bytes()); // backup LBA
    hdr[40..48].copy_from_slice(&2u64.to_le_bytes()); // first usable LBA
    hdr[48..56].copy_from_slice(&(total_lba - 34).to_le_bytes()); // last usable LBA
    hdr[56..64].copy_from_slice(&2u64.to_le_bytes()); // partition entries LBA
    hdr[64..68].copy_from_slice(&128u32.to_le_bytes()); // number of entries
    hdr[68..72].copy_from_slice(&128u32.to_le_bytes()); // entry size
    // CRC32 da tabela de entrada é calculado sobre o array vazio (0 entradas além desta)
    hdr[16..20].copy_from_slice(&0u32.to_le_bytes()); // placeholder CRC
    if !dev.write_sectors(1, &hdr) { return false; }

    // Cria entrada de partição no LBA 2
    let mut entry = [0u8; 512];
    entry[0..16].copy_from_slice(type_guid);
    // Unique GUID: hash simples do label
    let mut ug = [0u8; 16];
    for (i, b) in label.bytes().enumerate() { ug[i % 16] ^= b; }
    entry[16..32].copy_from_slice(&ug);
    entry[32..40].copy_from_slice(&2048u64.to_le_bytes()); // LBA start (alinhado 1MB)
    entry[40..48].copy_from_slice(&lba_end.to_le_bytes()); // LBA end
    entry[48..56].copy_from_slice(&0u64.to_le_bytes()); // attrs
    // Nome em UTF-16LE
    let label_u16: Vec<u16> = label.encode_utf16().collect();
    for (i, &c) in label_u16.iter().enumerate().take(36) {
        entry[56 + i * 2] = (c & 0xFF) as u8;
        entry[57 + i * 2] = (c >> 8) as u8;
    }
    if !dev.write_sectors(2, &entry) { return false; }

    // Limpa primeiro setor da partição (vbr será escrito pelo formatador)
    let zero = [0u8; 512];
    dev.write_sectors(2048, &zero);

    true
}
