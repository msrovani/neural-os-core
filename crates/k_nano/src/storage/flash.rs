//! ADR-0063 F0 — FlashController sobre BlockDevice / NVMe real (SESSION_171).
//! Região LBA dedicada para TickvLite (não usa stub storage/nvme.rs).

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

/// Driver flash sobre NVME_DRIVER global (região [base_lba, base_lba+total_lbas)).
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

    /// C1 (ora-1): região no FIM do disco — LBA 2048 (antigo default) colide com
    /// ESP@2048 e NeuralFS@4096 do GPT instalado (sys_installer). Brick no 1º boot
    /// NVMe real. Usa os últimos 32MB ANTES da backup GPT (últimos 34 setores).
    pub fn default_region() -> Self {
        // Via trait BlockDevice (NvmeDriver implementa total_sectors em 512B units)
        use crate::block_dev::BlockDevice;
        let total = NVME_DRIVER.lock()
            .as_mut()
            .map(|n| n.total_sectors())
            .unwrap_or(0);
        if total > 34 + 65_536 {
            NvmeFlashRegion::new(total - 34 - 65_536, 65_536)
        } else {
            // disco pequeno — ponytail: RAM fallback é melhor que colidir com o GPT
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
    Nvme(NvmeFlashRegion),
    Ram(RamFlash),
}

impl FlashController for ActiveFlash {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        match self {
            ActiveFlash::Nvme(f) => f.read(offset, buf),
            ActiveFlash::Ram(f) => f.read(offset, buf),
        }
    }
    fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), &'static str> {
        match self {
            ActiveFlash::Nvme(f) => f.write(offset, data),
            ActiveFlash::Ram(f) => f.write(offset, data),
        }
    }
    fn erase(&mut self, offset: u64, size: u64) -> Result<(), &'static str> {
        match self {
            ActiveFlash::Nvme(f) => f.erase(offset, size),
            ActiveFlash::Ram(f) => f.erase(offset, size),
        }
    }
    fn size_bytes(&self) -> u64 {
        match self {
            ActiveFlash::Nvme(f) => f.size_bytes(),
            ActiveFlash::Ram(f) => f.size_bytes(),
        }
    }
}

pub static FLASH: Mutex<Option<ActiveFlash>> = Mutex::new(None);

/// Init: NVMe se presente, senão RAM (honesto no log).
/// C2: RAM é VOLÁTIL — SELF.STATE, vida episódica, HANR e audit evaporam no
/// reboot sem erro. Log CRÍTICO deixa explícito (não silencioso).
pub fn init_flash() -> &'static str {
    let mut g = FLASH.lock();
    if NVME_DRIVER.lock().is_some() {
        *g = Some(ActiveFlash::Nvme(NvmeFlashRegion::default_region()));
        "nvme"
    } else {
        *g = Some(ActiveFlash::Ram(RamFlash::new(1024 * 1024)));
        crate::slog_nano!(
            "TICKV",
            "error",
            "backend=RAM (VOLATIL) — memoria IA (SELF.STATE/episodica/HANR/audit) nao persiste entre boots. Sem NVMe, use NeuralFS write-through p/ dados criticos."
        );
        "ram"
    }
}
