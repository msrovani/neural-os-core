//! ADR-0063 F0 â€” FlashController sobre BlockDevice / NVMe real (SESSION_171).
//! RegiÃ£o LBA dedicada para TickvLite (nÃ£o usa stub storage/nvme.rs).

use crate::block_dev::BlockDevice;
use crate::disk_agent::nvme::NVME_DRIVER;
use spin::Mutex;

/// Trait de flash alinhado a setores 512B (TicKV / TickvLite).
pub trait FlashController: Send {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), &'static str>;
    fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), &'static str>;
    /// NVMe: TRIM best-effort; pode ser no-op.
    fn erase(&mut self, offset: u64, size: u64) -> Result<(), &'static str>;
    fn size_bytes(&self) -> u64;
}

/// Driver flash sobre NVME_DRIVER global (regiÃ£o [base_lba, base_lba+total_lbas)).
pub struct NvmeFlashRegion {
    pub base_lba: u64,
    pub total_lbas: u64,
}

impl NvmeFlashRegion {
    pub fn new(base_lba: u64, total_lbas: u64) -> Self {
        NvmeFlashRegion {
            base_lba,
            total_lbas,
        }
    }

    /// C1 (ora-1): regiÃ£o no FIM do disco â€” LBA 2048 (antigo default) colide com
    /// ESP@2048 e NeuralFS@4096 do GPT instalado (sys_installer). Brick no 1Âº boot
    /// NVMe real. Usa os Ãºltimos 32MB ANTES da backup GPT (Ãºltimos 34 setores).
    pub fn default_region() -> Self {
        // Via trait BlockDevice (NvmeDriver implementa total_sectors em 512B units)
        let total = NVME_DRIVER.lock()
            .as_mut()
            .map(|n| n.total_sectors())
            .unwrap_or(0);
        if total > 34 + 65_536 {
            NvmeFlashRegion::new(total - 34 - 65_536, 65_536)
        } else {
            // disco pequeno â€” ponytail: RAM fallback Ã© melhor que colidir com o GPT
            NvmeFlashRegion::new(2048, 65_536)
        }
    }
}

impl FlashController for NvmeFlashRegion {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        if offset % 512 != 0 || buf.len() % 512 != 0 {
            return Err("unaligned");
        }
        let lba0 = self.base_lba + offset / 512;
        let n = (buf.len() / 512) as u64;
        if lba0 + n > self.base_lba + self.total_lbas {
            return Err("oob");
        }
        let mut g = NVME_DRIVER.lock();
        let nvme = g.as_mut().ok_or("no nvme")?;
        if !unsafe { nvme.read_sectors_bounce(lba0, buf) } {
            return Err("nvme read fail");
        }
        Ok(())
    }

    fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), &'static str> {
        if offset % 512 != 0 || data.len() % 512 != 0 {
            return Err("unaligned");
        }
        let lba0 = self.base_lba + offset / 512;
        let n = (data.len() / 512) as u64;
        if lba0 + n > self.base_lba + self.total_lbas {
            return Err("oob");
        }
        let mut g = NVME_DRIVER.lock();
        let nvme = g.as_mut().ok_or("no nvme")?;
        if !unsafe { nvme.write_sectors_bounce(lba0, data) } {
            return Err("nvme write fail");
        }
        Ok(())
    }

    fn erase(&mut self, _offset: u64, _size: u64) -> Result<(), &'static str> {
        Ok(()) // TRIM residual
    }

    fn size_bytes(&self) -> u64 {
        self.total_lbas.saturating_mul(512)
    }
}

// â”€â”€ FileFlash â€” FlashController sobre arquivo prÃ©-alocado no FAT32 â”€â”€â”€â”€â”€â”€â”€
/// `/NSGDB.BIN` (8MB zeros, `tools/mkfat32.py`) na partiÃ§Ã£o de dados do stick
/// Ã© o volume TickvLite/SGDB persistente quando nÃ£o hÃ¡ NVMe. Precedente:
/// `overwrite_boot_log` (boot_logger) â€” walk FAT data-only sobre BlockDevice
/// genÃ©rico (MBR+GPT â†’ BPB â†’ root dir â†’ cluster chain).
/// ponytail: IO setor-a-setor 512B como o precedente; dispositivo 4Kn nativo
/// recusa o buffer e o caller cai pro prÃ³ximo backend (upgrade: traduÃ§Ã£o
/// bps>512 se algum HW real pedir).

/// Nome 8.3 do volume no FAT32.
pub const FILE_FLASH_NAME: &str = "NSGDB.BIN";
/// Teto honesto pro resolve (dirent pode mentir): 64MB.
const FILE_FLASH_MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FlashDev {
    Usb,
    Ata,
    Ahci,
    Nvme,
    VirtioBlk,
}

impl FlashDev {
    fn name(self) -> &'static str {
        match self {
            FlashDev::Usb => "usb",
            FlashDev::Ata => "ata",
            FlashDev::Ahci => "ahci",
            FlashDev::Nvme => "nvme",
            FlashDev::VirtioBlk => "virtio",
        }
    }
}

/// Despacha pro global do dispositivo por operaÃ§Ã£o (a chain Ã© resolvida uma
/// vez no probe; o dispositivo nÃ£o Ã© retido entre chamadas).
fn with_flash_dev<R>(
    dev: FlashDev,
    f: impl FnOnce(&mut dyn BlockDevice) -> R,
) -> Result<R, &'static str> {
    match dev {
        FlashDev::Usb => {
            let mut g = crate::globals::USB_MSC.lock();
            let d = g.as_mut().ok_or("no usb")?;
            Ok(f(d))
        }
        FlashDev::Ata => {
            let mut g = crate::globals::ATA_DRIVER.lock();
            let d = g.as_mut().ok_or("no ata")?;
            Ok(f(d))
        }
        FlashDev::Ahci => {
            let mut g = crate::globals::AHCI_DRIVER.lock();
            let d = g.as_mut().ok_or("no ahci")?;
            Ok(f(d))
        }
        FlashDev::Nvme => {
            let mut g = NVME_DRIVER.lock();
            let d = g.as_mut().ok_or("no nvme")?;
            Ok(f(d))
        }
        FlashDev::VirtioBlk => {
            let mut g = crate::virtio_blk::VIRTIO_BLK_DEV.lock();
            let d = g.as_mut().ok_or("no virtio_blk")?;
            Ok(f(d))
        }
    }
}

/// LÃª `[offset, offset+len)` mapeando pela chain de setores 512B. Ops parciais
/// (nÃ£o alinhadas) leem o setor inteiro via bounce.
fn chain_read(
    sectors: &[u32],
    offset: u64,
    buf: &mut [u8],
    mut rd: impl FnMut(u64, &mut [u8; 512]) -> bool,
) -> Result<(), &'static str> {
    let cap = sectors.len() as u64 * 512;
    if offset.saturating_add(buf.len() as u64) > cap {
        return Err("oob");
    }
    let mut done = 0usize;
    let mut pos = offset as usize;
    while done < buf.len() {
        let idx = pos / 512;
        let in_off = pos % 512;
        let take = core::cmp::min(512 - in_off, buf.len() - done);
        let lba = sectors[idx] as u64;
        if in_off == 0 && take == 512 {
            let sec: &mut [u8; 512] = (&mut buf[done..done + 512]).try_into().unwrap();
            if !rd(lba, sec) {
                return Err("io");
            }
        } else {
            let mut sec = [0u8; 512];
            if !rd(lba, &mut sec) {
                return Err("io");
            }
            buf[done..done + take].copy_from_slice(&sec[in_off..in_off + take]);
        }
        pos += take;
        done += take;
    }
    Ok(())
}

/// Escreve `data` em `[offset, offset+len)` pela chain. Ops parciais usam
/// bounce read-modify-write â€” o setor tem bytes fora do range do caller
/// (ex.: `TickvLite::invalidate_key` escreve 1 byte em off+3).
/// `rw(lba, setor, write=trueâ†’grava / falseâ†’lÃª)` â€” closure Ãºnica porque o
/// dispositivo sÃ³ dÃ¡ `&mut` de cada vez.
fn chain_write(
    sectors: &[u32],
    offset: u64,
    data: &[u8],
    mut rw: impl FnMut(u64, &mut [u8; 512], bool) -> bool,
) -> Result<(), &'static str> {
    let cap = sectors.len() as u64 * 512;
    if offset.saturating_add(data.len() as u64) > cap {
        return Err("oob");
    }
    let mut done = 0usize;
    let mut pos = offset as usize;
    while done < data.len() {
        let idx = pos / 512;
        let in_off = pos % 512;
        let take = core::cmp::min(512 - in_off, data.len() - done);
        let lba = sectors[idx] as u64;
        if in_off == 0 && take == 512 {
            let mut sec = [0u8; 512];
            sec.copy_from_slice(&data[done..done + 512]);
            if !rw(lba, &mut sec, true) {
                return Err("io");
            }
        } else {
            let mut sec = [0u8; 512];
            if !rw(lba, &mut sec, false) {
                return Err("io");
            }
            sec[in_off..in_off + take].copy_from_slice(&data[done..done + take]);
            if !rw(lba, &mut sec, true) {
                return Err("io");
            }
        }
        pos += take;
        done += take;
    }
    Ok(())
}

/// Resolve um arquivo 8.3 na raiz da partiÃ§Ã£o FAT32 de dados e retorna os LBAs
/// (512B) de todos os setores do cluster-chain, em ordem. Mesmo caminho do
/// `overwrite_boot_log`: MBR+GPT â†’ BPB â†’ root dir chain â†’ FAT chain.
pub(crate) fn resolve_fat_file_sectors(
    dev: &mut dyn BlockDevice,
    want: &[u8; 11],
) -> Option<alloc::vec::Vec<u32>> {
    let parts = crate::fat32::read_mbr_dev(dev);
    for part in &parts {
        // 0xEF = ESP GPT; dados costuma ser 0x0C (mesmo filtro do boot_logger).
        if !matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0x73 | 0xEF) {
            continue;
        }
        let part_lba = part.lba_start as u64;
        let mut bpb = [0u8; 512];
        if !dev.read_sectors(part_lba, &mut bpb) {
            continue;
        }
        if &bpb[3..11] == b"EXFAT   " {
            continue;
        }
        let bps = u16::from_le_bytes([bpb[0x0B], bpb[0x0C]]) as u64;
        let spc = bpb[0x0D] as u64;
        let reserved = u16::from_le_bytes([bpb[0x0E], bpb[0x0F]]) as u64;
        let fat_count = bpb[0x10] as u64;
        let root_entries = u16::from_le_bytes([bpb[0x11], bpb[0x12]]);
        if root_entries > 0 || bps < 512 || bps > 4096 || bps % 32 != 0 || spc == 0 {
            continue;
        }
        let spf = u32::from_le_bytes([bpb[0x24], bpb[0x25], bpb[0x26], bpb[0x27]]) as u64;
        let root_cluster = u32::from_le_bytes([bpb[0x2C], bpb[0x2D], bpb[0x2E], bpb[0x2F]]);
        let fat_lba = part_lba + reserved;
        let data_lba = fat_lba + fat_count * spf;
        let cluster_bytes = (spc * bps) as usize;

        let mut cluster = root_cluster;
        let mut walked = 0u32;
        while cluster >= 2 && cluster < 0x0FFF_FFF8 && walked < 256 {
            walked += 1;
            let clba = data_lba + (cluster as u64 - 2) * spc;
            let mut dir = alloc::vec![0u8; cluster_bytes];
            let mut io_ok = true;
            for s in 0..spc {
                let off = (s * bps) as usize;
                if !dev.read_sectors(clba + s, &mut dir[off..off + bps as usize]) {
                    io_ok = false;
                    break;
                }
            }
            if !io_ok {
                break;
            }
            for entry in (0..dir.len()).step_by(32) {
                let first = dir[entry];
                if first == 0 {
                    break;
                }
                if first == 0xE5 {
                    continue;
                }
                if dir[entry + 11] & 0x0F == 0x0F || dir[entry + 11] & 0x08 != 0 {
                    continue;
                }
                if &dir[entry..entry + 11] != want {
                    continue;
                }
                let fsize = u32::from_le_bytes([
                    dir[entry + 28],
                    dir[entry + 29],
                    dir[entry + 30],
                    dir[entry + 31],
                ]) as usize;
                if fsize == 0 || fsize > FILE_FLASH_MAX_BYTES {
                    return None; // dirent mente / fora de polÃ­tica
                }
                let fc_lo = u16::from_le_bytes([dir[entry + 26], dir[entry + 27]]) as u32;
                let fc_hi = u16::from_le_bytes([dir[entry + 20], dir[entry + 21]]) as u32;
                let expect = fsize.div_ceil(512);
                let mut out = alloc::vec::Vec::with_capacity(expect);
                let mut fc = (fc_hi << 16) | fc_lo;
                while fc >= 2 && fc < 0x0FFF_FFF8 && out.len() < expect {
                    let base = data_lba + (fc as u64 - 2) * spc;
                    for s in 0..spc {
                        out.push((base + s) as u32);
                    }
                    let fat_off = fc as usize * 4;
                    let fat_sec = fat_lba + fat_off as u64 / bps;
                    let mut fsec = [0u8; 512];
                    if !dev.read_sectors(fat_sec, &mut fsec) {
                        return None;
                    }
                    let boff = fat_off % bps as usize;
                    fc = u32::from_le_bytes([
                        fsec[boff],
                        fsec[boff + 1],
                        fsec[boff + 2],
                        fsec[boff + 3],
                    ]) & 0x0FFF_FFFF;
                }
                if out.len() < expect {
                    return None; // chain curta â€” dirent mente; fail-closed
                }
                return Some(out);
            }
            // PrÃ³ximo cluster do root dir
            let fat_off = cluster as usize * 4;
            let fat_sec = fat_lba + fat_off as u64 / bps;
            let mut fsec = [0u8; 512];
            if !dev.read_sectors(fat_sec, &mut fsec) {
                break;
            }
            let boff = fat_off % bps as usize;
            cluster = u32::from_le_bytes([
                fsec[boff],
                fsec[boff + 1],
                fsec[boff + 2],
                fsec[boff + 3],
            ]) & 0x0FFF_FFFF;
        }
    }
    None
}

/// FlashController sobre `/NSGDB.BIN` prÃ©-alocado no FAT32 do stick de dados.
pub struct FileFlash {
    /// LBAs 512B dos setores do arquivo, em ordem de chain.
    sectors: alloc::vec::Vec<u32>,
    dev: FlashDev,
}

impl FileFlash {
    /// SeleÃ§Ã£o de dispositivo = ordem do `persist_now` (USB-MSC â†’ ATA â†’ AHCI â†’
    /// NVMe); vence o primeiro que tiver `/NSGDB.BIN` montÃ¡vel.
    ///
    /// Boot ordering: USB-MSC enumera tarde (K18+). v1 tenta sÃ³ aqui na init;
    /// sem dispositivo â†’ None e o caller cai pro prÃ³ximo backend SEM retry.
    /// TODO(follow-up): re-probe FileFlash pÃ³s-enumeraÃ§Ã£o MSC (hoje um stick
    /// que aparece depois fica sem SGDB persistente atÃ© o prÃ³ximo boot).
    pub fn probe() -> Option<FileFlash> {
        const ORDER: [FlashDev; 5] = [
            FlashDev::Usb,
            FlashDev::Ata,
            FlashDev::Ahci,
            FlashDev::Nvme,
            FlashDev::VirtioBlk,
        ];
        let want = crate::fat32::encode_83(FILE_FLASH_NAME);
        // QEMU/TCG: ata0 = uefi.img boot — FAT walk PIO trava; virtio-blk data disk only.
        if crate::storage_bw::skip_measure() {
            if let Ok(Some(sectors)) =
                with_flash_dev(FlashDev::VirtioBlk, |d| resolve_fat_file_sectors(d, &want))
            {
                crate::slog_nano!(
                    "TICKV",
                    "ok",
                    "FileFlash probe virtio-blk NSGDB sectors={}",
                    sectors.len()
                );
                return Some(FileFlash {
                    sectors,
                    dev: FlashDev::VirtioBlk,
                });
            }
            return None;
        }
        for dev in ORDER {
            // Live USB sem MSC: só stick (USB); evita FAT walk no HD interno (hang HW).
            if crate::boot_logger::internal_disk_skipped()
                && matches!(dev, FlashDev::Ata | FlashDev::Ahci | FlashDev::Nvme)
            {
                continue;
            }
            // Mesmo gate do persist_now: ATA PIO pode hangar TCG fora do plano.
            if dev == FlashDev::Ata
                && !crate::boot_bind::storage_includes(crate::boot_bind::StorageKind::Ata)
            {
                continue;
            }
            match with_flash_dev(dev, |d| resolve_fat_file_sectors(d, &want)) {
                Ok(Some(sectors)) => return Some(FileFlash { sectors, dev }),
                Ok(None) | Err(_) => continue,
            }
        }
        None
    }

    /// Dispositivo onde o arquivo vive (`usb|ata|ahci|nvme`) â€” log honesto.
    pub fn dev_name(&self) -> &'static str {
        self.dev.name()
    }

    /// Primeiro LBA fÃ­sico do arquivo (diagnÃ³stico).
    pub fn first_lba(&self) -> u32 {
        self.sectors.first().copied().unwrap_or(0)
    }
}

impl FlashController for FileFlash {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        let sectors = &self.sectors;
        with_flash_dev(self.dev, |d| {
            chain_read(sectors, offset, buf, |lba, sec| d.read_sectors(lba, sec))
        })?
    }

    fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), &'static str> {
        let sectors = &self.sectors;
        with_flash_dev(self.dev, |d| {
            chain_write(sectors, offset, data, |lba, sec, is_wr| {
                if is_wr {
                    d.write_sectors(lba, sec)
                } else {
                    d.read_sectors(lba, sec)
                }
            })
        })?
    }

    /// Sem erase fÃ­sico em FAT â€” compact() do TickvLite sobrescreve com zeros.
    fn erase(&mut self, _offset: u64, _size: u64) -> Result<(), &'static str> {
        Ok(())
    }

    fn size_bytes(&self) -> u64 {
        self.sectors.len() as u64 * 512
    }
}

/// Flash em RAM (smoke sem NVMe / testes).
pub struct RamFlash {
    data: alloc::vec::Vec<u8>,
}

impl RamFlash {
    pub fn new(size: usize) -> Self {
        let mut data = alloc::vec![0u8; size];
        // magic empty marker
        data[0] = 0;
        RamFlash { data }
    }
}

impl FlashController for RamFlash {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        let o = offset as usize;
        if o + buf.len() > self.data.len() {
            return Err("oob");
        }
        buf.copy_from_slice(&self.data[o..o + buf.len()]);
        Ok(())
    }
    fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), &'static str> {
        let o = offset as usize;
        if o + data.len() > self.data.len() {
            return Err("oob");
        }
        self.data[o..o + data.len()].copy_from_slice(data);
        Ok(())
    }
    fn erase(&mut self, offset: u64, size: u64) -> Result<(), &'static str> {
        let o = offset as usize;
        let s = size as usize;
        if o + s > self.data.len() {
            return Err("oob");
        }
        for b in &mut self.data[o..o + s] {
            *b = 0xFF;
        }
        Ok(())
    }
    fn size_bytes(&self) -> u64 {
        self.data.len() as u64
    }
}

/// Backend ativo: prefer NVMe; fallback RAM 1MB para demo.
pub enum ActiveFlash {
    File(FileFlash),
    Nvme(NvmeFlashRegion),
    Ram(RamFlash),
}

impl FlashController for ActiveFlash {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        match self {
            ActiveFlash::File(f) => f.read(offset, buf),
            ActiveFlash::Nvme(f) => f.read(offset, buf),
            ActiveFlash::Ram(f) => f.read(offset, buf),
        }
    }
    fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), &'static str> {
        match self {
            ActiveFlash::File(f) => f.write(offset, data),
            ActiveFlash::Nvme(f) => f.write(offset, data),
            ActiveFlash::Ram(f) => f.write(offset, data),
        }
    }
    fn erase(&mut self, offset: u64, size: u64) -> Result<(), &'static str> {
        match self {
            ActiveFlash::File(f) => f.erase(offset, size),
            ActiveFlash::Nvme(f) => f.erase(offset, size),
            ActiveFlash::Ram(f) => f.erase(offset, size),
        }
    }
    fn size_bytes(&self) -> u64 {
        match self {
            ActiveFlash::File(f) => f.size_bytes(),
            ActiveFlash::Nvme(f) => f.size_bytes(),
            ActiveFlash::Ram(f) => f.size_bytes(),
        }
    }
}

pub static FLASH: Mutex<Option<ActiveFlash>> = Mutex::new(None);

/// Init: FileFlash (`/NSGDB.BIN` no FAT do stick) > NVMe > RAM (honesto no log).
/// C2: RAM Ã© VOLÃTIL â€” SELF.STATE, vida episÃ³dica, HANR e audit evaporam no
/// reboot sem erro. Log CRÃTICO deixa explÃ­cito (nÃ£o silencioso).
pub fn init_flash() -> &'static str {
    let mut g = FLASH.lock();
    // v1: tenta na init normal; USB-MSC enumera tarde (K18+) â€” se nÃ£o estiver
    // pronto AGORA cai pro prÃ³ximo backend sem retry complexo.
    // TODO(follow-up): re-probe pÃ³s-MSC (ver FileFlash::probe).
    if let Some(ff) = FileFlash::probe() {
        crate::slog_nano!(
            "TICKV",
            "ok",
            "backend=file lba={} dev={} cap={}KB",
            ff.first_lba(),
            ff.dev_name(),
            ff.size_bytes() / 1024
        );
        *g = Some(ActiveFlash::File(ff));
        return "file";
    }
    if crate::boot_logger::internal_disk_skipped() {
        *g = Some(ActiveFlash::Ram(RamFlash::new(1024 * 1024)));
        crate::slog_nano!(
            "TICKV",
            "info",
            "backend=RAM (live USB sem MSC — skip NVMe interno)"
        );
        return "ram";
    }
    if NVME_DRIVER.lock().is_some() {
        *g = Some(ActiveFlash::Nvme(NvmeFlashRegion::default_region()));
        "nvme"
    } else {
        *g = Some(ActiveFlash::Ram(RamFlash::new(1024 * 1024)));
        crate::slog_nano!(
            "TICKV",
            "error",
            "backend=RAM (VOLATIL) â€” memoria IA (SELF.STATE/episodica/HANR/audit) nao persiste entre boots. Sem NVMe, use NeuralFS write-through p/ dados criticos."
        );
        "ram"
    }
}

#[cfg(test)]
mod file_flash_tests {
    use super::*;

    /// Mock BlockDevice em memÃ³ria (512B/setor) â€” sem hardware.
    struct MockDev {
        data: alloc::vec::Vec<u8>,
    }

    impl BlockDevice for MockDev {
        fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
            let off = lba as usize * 512;
            if buf.is_empty() || buf.len() % 512 != 0 || off + buf.len() > self.data.len() {
                return false;
            }
            buf.copy_from_slice(&self.data[off..off + buf.len()]);
            true
        }
        fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
            let off = lba as usize * 512;
            if buf.is_empty() || buf.len() % 512 != 0 || off + buf.len() > self.data.len() {
                return false;
            }
            self.data[off..off + buf.len()].copy_from_slice(buf);
            true
        }
    }

    const TEST_CAP: usize = 16 * 1024; // 32 setores
    const PART_LBA: u64 = 2048;

    /// FAT32 sintÃ©tico com NSGDB.BIN: MBR(0x0C@2048) + BPB(reserved=8, fats=1,
    /// spf=1, spc=1, root=cluster2) + chain clusters 3..35 (TEST_CAP bytes).
    fn mock_image_with_nsgdb() -> MockDev {
        let reserved: u64 = 8;
        let fats: u64 = 1;
        let spf: u64 = 1;
        let data_lba = PART_LBA + reserved + fats * spf;
        let total = (data_lba + 64) as usize; // root + arquivo + slack
        let mut img = alloc::vec![0u8; total * 512];

        // MBR: uma partiÃ§Ã£o FAT32 0x0C
        img[510] = 0x55;
        img[511] = 0xAA;
        img[0x1BE + 4] = 0x0C;
        img[0x1BE + 8..0x1BE + 12].copy_from_slice(&(PART_LBA as u32).to_le_bytes());
        img[0x1BE + 12..0x1BE + 16]
            .copy_from_slice(&(total as u32 - PART_LBA as u32).to_le_bytes());

        // BPB
        let bp = PART_LBA as usize * 512;
        img[bp + 3..bp + 11].copy_from_slice(b"MSWIN4.1");
        img[bp + 0x0B..bp + 0x0D].copy_from_slice(&512u16.to_le_bytes());
        img[bp + 0x0D] = 1; // sectors per cluster
        img[bp + 0x0E..bp + 0x10].copy_from_slice(&(reserved as u16).to_le_bytes());
        img[bp + 0x10] = fats as u8;
        img[bp + 0x24..bp + 0x28].copy_from_slice(&(spf as u32).to_le_bytes());
        img[bp + 0x2C..bp + 0x30].copy_from_slice(&2u32.to_le_bytes()); // root cluster

        // FAT: root(2)=EOC; arquivo: 3â†’4â†’â€¦â†’EOC
        let fat_off = ((PART_LBA + reserved) as usize) * 512;
        let nsec = TEST_CAP / 512;
        img[fat_off + 8..fat_off + 12].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
        for i in 0..nsec {
            let cl = (3 + i) as u32;
            let next = if i + 1 < nsec { cl + 1 } else { 0x0FFF_FFF8 };
            let o = fat_off + cl as usize * 4;
            img[o..o + 4].copy_from_slice(&next.to_le_bytes());
        }

        // Dirent NSGDB.BIN no root (cluster 2 @ data_lba)
        let de = data_lba as usize * 512;
        img[de..de + 11].copy_from_slice(b"NSGDB   BIN");
        img[de + 11] = 0x20; // archive
        img[de + 26..de + 28].copy_from_slice(&3u16.to_le_bytes()); // first cluster lo
        img[de + 28..de + 32].copy_from_slice(&(TEST_CAP as u32).to_le_bytes());

        MockDev { data: img }
    }

    #[test]
    fn encode_83_nsgdb_matches_dirent() {
        assert_eq!(crate::fat32::encode_83(FILE_FLASH_NAME), *b"NSGDB   BIN");
    }

    #[test]
    fn resolve_maps_full_chain_in_order() {
        let mut dev = mock_image_with_nsgdb();
        let want = crate::fat32::encode_83(FILE_FLASH_NAME);
        let sectors = resolve_fat_file_sectors(&mut dev, &want).expect("resolve");
        assert_eq!(sectors.len(), TEST_CAP / 512);
        // data_lba = 2048+8+1 = 2057; cluster 3 â†’ LBA 2058, contÃ­gua.
        assert_eq!(sectors[0], 2058);
        assert_eq!(sectors[1], 2059);
        assert_eq!(sectors[31], 2058 + 31);
    }

    #[test]
    fn aligned_write_read_roundtrip() {
        let mut dev = mock_image_with_nsgdb();
        let want = crate::fat32::encode_83(FILE_FLASH_NAME);
        let sectors = resolve_fat_file_sectors(&mut dev, &want).expect("resolve");
        let payload: alloc::vec::Vec<u8> = (0..1024).map(|i| (i % 251) as u8).collect();
        chain_write(&sectors, 512, &payload, |l, s, w| {
            if w {
                dev.write_sectors(l, s)
            } else {
                dev.read_sectors(l, s)
            }
        })
        .expect("write");
        let mut out = [0u8; 1024];
        chain_read(&sectors, 512, &mut out, |l, s| dev.read_sectors(l, s)).expect("read");
        assert_eq!(out[..], payload[..]);
    }

    #[test]
    fn partial_write_is_read_modify_write() {
        let mut dev = mock_image_with_nsgdb();
        let want = crate::fat32::encode_83(FILE_FLASH_NAME);
        let sectors = resolve_fat_file_sectors(&mut dev, &want).expect("resolve");
        // Fundo conhecido
        chain_write(&sectors, 0, &[0xA5u8; 512], |l, s, w| {
            if w {
                dev.write_sectors(l, s)
            } else {
                dev.read_sectors(l, s)
            }
        })
        .unwrap();
        // PadrÃ£o invalidate_key: 3 bytes em offset nÃ£o alinhado
        chain_write(&sectors, 5, &[0, 1, 2], |l, s, w| {
            if w {
                dev.write_sectors(l, s)
            } else {
                dev.read_sectors(l, s)
            }
        })
        .unwrap();
        let mut out = [0u8; 512];
        chain_read(&sectors, 0, &mut out, |l, s| dev.read_sectors(l, s)).unwrap();
        assert_eq!(&out[5..8], &[0, 1, 2]);
        assert_eq!(out[4], 0xA5); // vizinhos preservados pelo RMW
        assert_eq!(out[8], 0xA5);
    }

    #[test]
    fn oob_rejected() {
        let mut dev = mock_image_with_nsgdb();
        let want = crate::fat32::encode_83(FILE_FLASH_NAME);
        let sectors = resolve_fat_file_sectors(&mut dev, &want).expect("resolve");
        let mut out = [0u8; 512];
        assert_eq!(
            chain_read(&sectors, TEST_CAP as u64, &mut out, |_, _| true),
            Err("oob")
        );
        assert_eq!(
            chain_write(&sectors, (TEST_CAP - 100) as u64, &[0u8; 512], |_, _, _| true),
            Err("oob")
        );
    }

    #[test]
    fn missing_file_returns_none() {
        let mut dev = mock_image_with_nsgdb();
        let want = crate::fat32::encode_83("NAOEXIS.BIN");
        assert!(resolve_fat_file_sectors(&mut dev, &want).is_none());
    }

    // FileFlash::probe()/with_flash_dev NÃƒO tÃªm teste host: ligam aos statics
    // globais de driver (USB_MSC/ATA_DRIVER/AHCI_DRIVER/NVME_DRIVER), que nÃ£o
    // sÃ£o injetÃ¡veis em testes. O glue Ã© despacho trivial de Mutex; a lÃ³gica
    // real (resolve + chain rw) estÃ¡ coberta acima sobre MockDev.
}
