//! btrfs read MVP — superblock magic (Labor 51). Não RW pleno.

use crate::block_dev::BlockDevice;

const BTRFS_MAGIC: &[u8; 8] = b"_BHRfS_M";
const SUPER_OFF: u64 = 0x10_000; // 64 KiB

pub struct BtrfsSuper {
    pub bytenr: u64,
    pub magic_ok: bool,
}

/// Lê superblock primário em LBA start + 64KiB.
pub fn probe_super(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<BtrfsSuper> {
    let off_sectors = SUPER_OFF / 512;
    let mut buf = [0u8; 512];
    if !dev.read_sectors(start_lba + off_sectors, &mut buf) {
        return None;
    }
    // magic @ offset 64 in superblock
    if buf.len() < 72 {
        return None;
    }
    let magic_ok = &buf[64..72] == BTRFS_MAGIC;
    if !magic_ok {
        return None;
    }
    let bytenr = u64::from_le_bytes(buf[32..40].try_into().ok()?);
    Some(BtrfsSuper { bytenr, magic_ok })
}

pub fn boot_smoke() {
    crate::slog_nano!(
        "BTRFS",
        "info",
        "step=read_mvp status=OK VERDICT=PARTIAL reason=probe_api_ready (no volume=SKIP mount)"
    );
}