//! VirtIO-blk driver — bare-metal, transporte legacy PCI (OASIS VirtIO 1.1 §2.4/§5.2).
//! Espelha o padrão de `virtio_net.rs`, mas com layout de virtqueue COMPUTADO do
//! queue size reportado pelo device (blk=128 no QEMU ≠ net=256 — layout hardcoded
//! do net não serve aqui; spec §2.4: desc@0, avail alinhado 2, used alinhado 4096).
//!
//! QEMU `-drive if=virtio` cria virtio-blk-pci **transitional** (1AF4:1001) com
//! BAR0 I/O ports legacy — é o caminho suportado. Modern-only (1AF4:1042) é
//! detectado mas NÃO inicializado (protocolo common-cfg moderno é outro fio;
//! log honesto — sem hallucination).
//!
//! DMA: bounce buffer pré-alocado (2 páginas contíguas: header@0, status@16,
//! dados@4096). Buffers do caller vivem no heap (bump 0x_4000_0000_0000) — não
//! endereçáveis como phys via HHDM, então toda I/O passa pelo scratch. Ops são
//! serializadas pelo Mutex do global `VIRTIO_BLK_DEV` (1 request outstanding,
//! poll-to-completion, sem IRQ).

extern crate alloc;
use core::sync::atomic::Ordering;
use spin::Mutex;
use x86_64::instructions::port::Port;

use crate::block_dev::BlockDevice;
use crate::memory::PHYS_MEM_OFFSET;
use crate::pci::PciDevice;

pub const VIRTIO_VENDOR: u16 = 0x1AF4;
pub const VIRTIO_BLK_TRANSITIONAL: u16 = 0x1001; // legacy + modern (QEMU default)
pub const VIRTIO_BLK_MODERN: u16 = 0x1042;       // modern only — não suportado aqui

// Legacy I/O port offsets (mesmo layout do virtio_net.rs)
const REG_DEVICE_FEATURES: u16 = 0x00;
const REG_GUEST_FEATURES: u16 = 0x04;
const REG_QUEUE_ADDR: u16 = 0x08;  // PFN da virtqueue (phys >> 12)
const REG_QUEUE_SIZE: u16 = 0x0C;  // RO: max queue size do device
const REG_QUEUE_SEL: u16 = 0x0E;
const REG_QUEUE_NOTIFY: u16 = 0x10;
const REG_STATUS: u16 = 0x12;
const REG_CONFIG: u16 = 0x14;      // device config blk: capacity u64 @0, blk_size u32 @20

const STATUS_ACK: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;

// Features negociáveis (legado) — o resto é rejeitado (guest = features & MÁSCARA)
const F_ANY_LAYOUT: u64 = 1 << 27;
const F_RING_EVENT_IDX: u64 = 1 << 29;
const GUEST_FEATURES_MASK: u64 = F_ANY_LAYOUT | F_RING_EVENT_IDX;

// VIRTIO_BLK_F_RO (bit 5) — device somente-leitura
const F_BLK_RO: u64 = 1 << 5;

// Request types (VirtIO 1.1 §5.2.6)
pub const BLK_T_IN: u32 = 0;  // read (device escreve no buffer)
pub const BLK_T_OUT: u32 = 1; // write (device lê o buffer)

// Status byte do request (§5.2.7)
pub const BLK_S_OK: u8 = 0;
pub const BLK_S_IOERR: u8 = 1;
pub const BLK_S_UNSUPP: u8 = 2;

const DESC_F_WRITE: u16 = 2; // device-writable

const SECTOR: usize = 512;
/// Setores por request (página de dados do scratch = 4096B).
const SECTORS_PER_REQ: usize = 8;
/// Timeout de poll do used ring (TCG é lento; QEMU completa em µs de host).
const POLL_LIMIT: u32 = 1_000_000;

/// Global do driver — populado por `init_driver_virtio_blk()`. `.data` (SESSION_234).
#[link_section = ".data"]
pub static VIRTIO_BLK_DEV: Mutex<Option<VirtIoBlk>> = Mutex::new(None);

pub struct VirtIoBlk {
    io_base: u16,
    queue_pa: u64,   // phys da virtqueue (desc@0, avail@avail_off, used@used_off)
    scratch_pa: u64, // phys do bounce buffer (2 páginas: hdr+status | dados)
    qsize: u16,      // max queue size do device (define o layout)
    avail_off: u32,
    used_off: u32,
    avail_idx: u16,  // próximo slot avail a publicar
    used_last: u16,  // último used consumido
    capacity: u64,   // setores 512B
    blk_size: u32,   // bytes por setor lógico reportado (512 esperado)
    readonly: bool,
}

/// Header de request VirtIO-blk (16B little-endian): type u32, reserved u32, sector u64.
/// Pure p/ teste host — `transfer` escreve exatamente estes bytes no scratch.
pub fn blk_req_header(ty: u32, sector: u64) -> [u8; 16] {
    let mut h = [0u8; 16];
    h[0..4].copy_from_slice(&ty.to_le_bytes());
    // h[4..8] = reserved (priority, legado) — zero
    h[8..16].copy_from_slice(&sector.to_le_bytes());
    h
}

/// Inverso de `blk_req_header` — (type, sector).
pub fn parse_blk_req_header(h: &[u8; 16]) -> (u32, u64) {
    (
        u32::from_le_bytes(h[0..4].try_into().unwrap()),
        u64::from_le_bytes(h[8..16].try_into().unwrap()),
    )
}

/// Offsets do legacy virtqueue (spec §2.4) para um queue size `qsize`:
/// (avail_off, used_off, total_bytes). O device usa o MAX size DELE — o driver
/// não escolhe tamanho em legacy, só publica o PFN. Pure p/ teste host.
fn queue_layout(qsize: u16) -> (u32, u32, u32) {
    let desc_len = qsize as u32 * 16;
    let avail_off = desc_len; // já alinhado a 2 (16×n)
    let avail_len = 2 + 2 + 2 * qsize as u32 + 2; // flags+idx+ring+used_event
    let used_off = (avail_off + avail_len + 4095) & !4095; // alinhado 4096
    let used_len = 2 + 2 + 8 * qsize as u32 + 2; // flags+idx+ring+avail_event
    (avail_off, used_off, used_off + used_len)
}

/// Aloca N páginas físicas contíguas → (phys, virt). Mesmo padrão de virtio_net.rs.
unsafe fn alloc_pages(n: usize) -> Option<(u64, *mut u8)> {
    let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
    let alloc = (*guard).as_mut()?;
    let frame = alloc.allocate_contiguous(n)?;
    let pa = frame.start_address().as_u64();
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    Some((pa, (pa + offset) as *mut u8))
}

impl VirtIoBlk {
    /// Init a partir de um PciDevice transitional (1AF4:1001) com BAR0 I/O legacy.
    unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.bar0 & 1 != 1 {
            crate::slog_nano!("VBLK", "warn", "BAR0 nao e I/O ({:#x}) — legacy ausente", dev.bar0);
            return None;
        }
        let io_base = (dev.bar0 & !0x3) as u16;

        // Reset
        Port::new(io_base + REG_STATUS).write(0u8);
        let mut spins = 0u32;
        while Port::<u8>::new(io_base + REG_STATUS).read() != 0 {
            core::hint::spin_loop();
            spins += 1;
            if spins > 1_000_000 {
                crate::slog_nano!("VBLK", "fail", "reset nao completa");
                return None;
            }
        }
        Port::new(io_base + REG_STATUS).write(STATUS_ACK | STATUS_DRIVER);

        // Features: aceita só ANY_LAYOUT/EVENT_IDX (e observa RO); resto rejeitado.
        let features = Port::<u32>::new(io_base + REG_DEVICE_FEATURES).read() as u64;
        let readonly = features & F_BLK_RO != 0;
        let guest = features & GUEST_FEATURES_MASK;
        Port::new(io_base + REG_GUEST_FEATURES).write(guest as u32);
        Port::new(io_base + REG_STATUS).write(
            STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK,
        );
        let st = Port::<u8>::new(io_base + REG_STATUS).read();
        if st & STATUS_FEATURES_OK == 0 {
            crate::slog_nano!("VBLK", "fail", "FEATURES_OK rejeitado (guest={:#x})", guest);
            return None;
        }

        // Config: capacity u64 @0x14, blk_size u32 @0x14+20
        let cap_lo = Port::<u32>::new(io_base + REG_CONFIG).read() as u64;
        let cap_hi = Port::<u32>::new(io_base + REG_CONFIG + 4).read() as u64;
        let capacity = cap_lo | (cap_hi << 32);
        let blk_size_raw = Port::<u32>::new(io_base + REG_CONFIG + 20).read();
        let blk_size = if blk_size_raw == 0 || blk_size_raw % 512 != 0 { 512 } else { blk_size_raw };

        // Virtqueue 0 — layout do MAX size do device (spec §2.4)
        Port::new(io_base + REG_QUEUE_SEL).write(0u16);
        let qsize = Port::new(io_base + REG_QUEUE_SIZE).read();
        if qsize == 0 {
            crate::slog_nano!("VBLK", "fail", "queue size 0");
            return None;
        }
        let (avail_off, used_off, total) = queue_layout(qsize);
        let pages = total.div_ceil(4096) as usize;
        let (queue_pa, queue_va) = alloc_pages(pages)?;
        core::ptr::write_bytes(queue_va, 0, pages * 4096);
        Port::new(io_base + REG_QUEUE_ADDR).write((queue_pa >> 12) as u32);

        // Bounce buffer: 2 páginas — hdr(16B)@0, status(1B)@16, dados@4096
        let (scratch_pa, scratch_va) = alloc_pages(2)?;
        core::ptr::write_bytes(scratch_va, 0, 2 * 4096);

        // DRIVER_OK
        Port::new(io_base + REG_STATUS).write(
            STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );

        crate::slog_nano!(
            "VBLK", "ok",
            "io={:#x} cap={}MB qsize={} blk={} ro={}",
            io_base, capacity * 512 / (1024 * 1024), qsize, blk_size, readonly
        );
        Some(VirtIoBlk {
            io_base,
            queue_pa,
            scratch_pa,
            qsize,
            avail_off,
            used_off,
            avail_idx: 0,
            used_last: 0,
            capacity,
            blk_size,
            readonly,
        })
    }

    fn notify_queue(&self) {
        unsafe { Port::new(self.io_base + REG_QUEUE_NOTIFY).write(0u16); }
    }

    /// Um request síncrono (≤ 8 setores). Bounce via scratch; poll do used ring.
    /// `data` é lido (write) ou escrito (read) — raw ptr porque o caminho write
    /// recebe `&[u8]` do caller e só faz copy FROM dele.
    unsafe fn transfer(&mut self, ty: u32, lba: u64, data: *mut u8, len: usize, is_write: bool) -> bool {
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let scratch_va = (self.scratch_pa + offset) as *mut u8;
        let queue_va = (self.queue_pa + offset) as *mut u8;

        // Header (16B) + status byte @16
        let hdr = blk_req_header(ty, lba);
        core::ptr::copy_nonoverlapping(hdr.as_ptr(), scratch_va, 16);
        *(scratch_va.add(16)) = 0; // status limpo — device escreve OK/IOERR

        // Dados: bounce
        let data_va = scratch_va.add(4096);
        if is_write {
            core::ptr::copy_nonoverlapping(data as *const u8, data_va, len);
        }

        // Descritores 0,1,2 (chain fixa, 1 request outstanding)
        let desc = queue_va;
        let set_desc = |i: usize, addr: u64, dlen: u32, flags: u16, next: u16| {
            let p = desc.add(i * 16);
            (p as *mut u64).write_volatile(addr);
            (p.add(8) as *mut u32).write_volatile(dlen);
            (p.add(12) as *mut u16).write_volatile(flags);
            (p.add(14) as *mut u16).write_volatile(next);
        };
        // Chain: hdr → data → status (next encadeia; último = 0)
        set_desc(0, self.scratch_pa, 16, 0, 1); // header: device-read → next=1
        set_desc(1, self.scratch_pa + 4096, len as u32,
                 if is_write { 0 } else { DESC_F_WRITE }, 2); // dados → next=2
        set_desc(2, self.scratch_pa + 16, 1, DESC_F_WRITE, 0); // status: device-write, fim

        // Publica no avail ring (head = desc 0)
        let avail = queue_va.add(self.avail_off as usize);
        let ring_slot = (self.avail_idx % self.qsize) as usize;
        (avail.add(4 + ring_slot * 2) as *mut u16).write_volatile(0u16);
        core::sync::atomic::fence(Ordering::SeqCst);
        (avail.add(2) as *mut u16).write_volatile(self.avail_idx.wrapping_add(1));
        self.avail_idx = self.avail_idx.wrapping_add(1);

        self.notify_queue();

        // Poll used ring
        let used = queue_va.add(self.used_off as usize);
        let mut polls = 0u32;
        loop {
            core::sync::atomic::fence(Ordering::SeqCst);
            let used_idx = (used as *const u16).add(1).read_volatile();
            if used_idx != self.used_last {
                break;
            }
            polls += 1;
            if polls > POLL_LIMIT {
                // DIAGNÓSTICO #PF-storm: dump do estado da virtqueue no timeout
                let pfn = self.queue_pa >> 12;
                crate::slog_nano!("VBLK", "fail",
                    "timeout lba={} qpa={:#x} pfn={:#x} avail_off={} used_off={} qsize={}",
                    lba, self.queue_pa, pfn, self.avail_off, self.used_off, self.qsize);
                // Descritores 0-2 (o device deveria ler isto)
                for i in 0..3usize {
                    let p = desc.add(i * 16);
                    let daddr = (p as *const u64).read_volatile();
                    let dlen = (p.add(8) as *const u32).read_volatile();
                    let dflags = (p.add(12) as *const u16).read_volatile();
                    let dnext = (p.add(14) as *const u16).read_volatile();
                    crate::slog_nano!("VBLK", "fail",
                        "  desc[{}] addr={:#x} len={} flags={:#x} next={}",
                        i, daddr, dlen, dflags, dnext);
                }
                // Avail ring: flags, idx, ring[0]
                let aflags = (avail as *const u16).read_volatile();
                let aidx = (avail.add(2) as *const u16).read_volatile();
                let a0 = (avail.add(4) as *const u16).read_volatile();
                crate::slog_nano!("VBLK", "fail",
                    "  avail: flags={:#x} idx={} ring[0]={}", aflags, aidx, a0);
                // Used ring: flags, idx, ring[0]
                let uflags = (used as *const u16).read_volatile();
                let uidx = (used.add(2) as *const u16).read_volatile();
                let uid = (used.add(4) as *const u32).read_volatile();
                crate::slog_nano!("VBLK", "fail",
                    "  used: flags={:#x} idx={} ring[0].id={} (used_last={})",
                    uflags, uidx, uid, self.used_last);
                return false;
            }
            core::hint::spin_loop();
        }
        // elem = used.ring[used_last % qsize] (id u32 @+4+8i, len u32 @+8+8i)
        let elem_off = 4 + (self.used_last % self.qsize) as usize * 8;
        let _id = (used.add(elem_off) as *const u32).read_volatile();
        let _len = (used.add(elem_off + 4) as *const u32).read_volatile();
        self.used_last = self.used_last.wrapping_add(1);

        // Status byte
        let status = scratch_va.add(16).read_volatile();
        match status {
            BLK_S_OK => {
                if !is_write {
                    core::ptr::copy_nonoverlapping(data_va as *const u8, data, len);
                }
                true
            }
            BLK_S_IOERR => {
                crate::slog_nano!("VBLK", "fail", "IOERR lba={} len={}", lba, len);
                false
            }
            BLK_S_UNSUPP => {
                crate::slog_nano!("VBLK", "fail", "UNSUPP lba={} ty={}", lba, ty);
                false
            }
            s => {
                crate::slog_nano!("VBLK", "fail", "status desconhecido {} lba={}", s, lba);
                false
            }
        }
    }

    /// I/O em chunks de 8 setores (página de dados do scratch). `data` é lido
    /// (write) ou escrito (read) pelo driver — ver `transfer`.
    fn io_chunks(&mut self, lba: u64, data: *mut u8, len: usize, is_write: bool) -> bool {
        if self.blk_size as usize != SECTOR {
            // honesto: device 4Kn nativo recusa — caller cai pro próximo backend
            crate::slog_nano!("VBLK", "warn", "blk_size {} != 512 — I/O recusado", self.blk_size);
            return false;
        }
        if len == 0 || len % SECTOR != 0 {
            return false;
        }
        let n = len / SECTOR;
        if lba.saturating_add(n as u64) > self.capacity {
            crate::slog_nano!("VBLK", "warn", "oob lba={} n={} cap={}", lba, n, self.capacity);
            return false;
        }
        let mut done = 0usize;
        while done < len {
            let take = core::cmp::min(SECTORS_PER_REQ * SECTOR, len - done);
            let sec = lba + (done / SECTOR) as u64;
            let ty = if is_write { BLK_T_OUT } else { BLK_T_IN };
            if !unsafe { self.transfer(ty, sec, unsafe { data.add(done) }, take, is_write) } {
                return false;
            }
            done += take;
        }
        true
    }

    /// Self-test: lê o setor 0 e verifica assinatura MBR 0x55AA @510.
    /// Log honesto — disco blank não falha o init (device funciona igual).
    fn self_test(&mut self) {
        let mut sec = [0u8; SECTOR];
        if !self.read_sectors(0, &mut sec) {
            crate::slog_nano!("VBLK", "warn", "self-test: leitura do setor 0 falhou");
            return;
        }
        if sec[510] == 0x55 && sec[511] == 0xAA {
            crate::slog_nano!("VBLK", "ok", "self-test: MBR 0x55AA OK");
        } else {
            crate::slog_nano!("VBLK", "warn", "self-test: setor 0 sem MBR 0x55AA (disco blank?)");
        }
    }
}

impl BlockDevice for VirtIoBlk {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        self.io_chunks(lba, buf.as_mut_ptr(), buf.len(), false)
    }
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        if self.readonly {
            crate::slog_nano!("VBLK", "warn", "write em device RO recusado");
            return false;
        }
        // Cast p/ *mut só para uniformizar a assinatura — o driver só lê deste
        // ponteiro no caminho write (bounce para o scratch).
        self.io_chunks(lba, buf.as_ptr() as *mut u8, buf.len(), true)
    }
    fn total_sectors(&self) -> u64 {
        self.capacity
    }
    fn sector_size(&self) -> u16 {
        self.blk_size as u16
    }
    fn name(&self) -> &str {
        "vblk0"
    }
}

/// Varre PCI, detecta 1AF4:1001/1042, inicializa o primeiro blk e popula
/// `VIRTIO_BLK_DEV`. Idempotente. Não registra no StorageBus (caller faz).
pub unsafe fn init_driver_virtio_blk() -> bool {
    if VIRTIO_BLK_DEV.lock().is_some() {
        return true;
    }
    let devices = crate::pci::scan_pci();
    for dev in &devices {
        if dev.vendor_id != VIRTIO_VENDOR {
            continue;
        }
        if dev.device_id == VIRTIO_BLK_MODERN {
            crate::slog_nano!(
                "VBLK", "warn",
                "1AF4:1042 modern-only detectado — requer protocolo common-cfg moderno (nao suportado); use transitional"
            );
            continue;
        }
        if dev.device_id != VIRTIO_BLK_TRANSITIONAL {
            continue;
        }
        crate::slog_nano!(
            "VBLK", "info",
            "Detectado {:02x}:{:02x}.{:02x} (1AF4:1001)",
            dev.bus, dev.device, dev.function
        );
        // read_bar_value mascara o bit 0 de I/O BARs (pci.rs:249 `low & !0xFF`)
        // — o check `bar0 & 1` em VirtIoBlk::new nunca passaria. Re-lê o BAR
        // raw do PCI config (bit 0 intacto) para o device detectado.
        let bar0_raw = unsafe { crate::pci::read_config_dword(dev.bus, dev.device, dev.function, 0x10) };
        if bar0_raw & 1 == 1 {
            // I/O BAR: reconstrói o valor que o driver espera (bit 0 setado)
            let mut dev_io = *dev;
            dev_io.bar0 = (bar0_raw & !0xFF) as u64 | 1;
            if let Some(blk) = VirtIoBlk::new(&dev_io) {
                let mut b = blk;
                b.self_test();
                *VIRTIO_BLK_DEV.lock() = Some(b);
                return true;
            }
        } else {
            // MMIO BAR (virtio modern) — protocolo common-cfg não suportado ainda
            crate::slog_nano!(
                "VBLK", "warn",
                "BAR0 MMIO ({:#x}) — transporte modern nao suportado; use disable-modern=on no QEMU",
                dev.bar0
            );
        }
        if let Some(blk) = VirtIoBlk::new(dev) {
            let mut b = blk;
            b.self_test();
            *VIRTIO_BLK_DEV.lock() = Some(b);
            return true;
        }
    }
    crate::slog_nano!("VBLK", "info", "Nenhum VirtIO-blk encontrado.");
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn req_header_roundtrip() {
        for (ty, sector) in [(BLK_T_IN, 0u64), (BLK_T_OUT, 1), (BLK_T_IN, 2048), (7, u64::MAX)] {
            let h = blk_req_header(ty, sector);
            assert_eq!(parse_blk_req_header(&h), (ty, sector));
        }
    }

    #[test]
    fn req_header_layout() {
        // type u32 @0, reserved u32 @4 (zero), sector u64 @8 — little-endian
        let h = blk_req_header(BLK_T_OUT, 0x1122334455667788);
        assert_eq!(&h[0..4], &1u32.to_le_bytes());
        assert_eq!(&h[4..8], &[0, 0, 0, 0]);
        assert_eq!(&h[8..16], &0x1122334455667788u64.to_le_bytes());
        assert_eq!(h.len(), 16);
    }

    #[test]
    fn queue_layout_matches_spec_2_4() {
        // QEMU virtio-blk legacy: qsize=128 → desc@0(2048B), avail@2048, used@4096
        let (avail, used, total) = queue_layout(128);
        assert_eq!(avail, 2048);
        assert_eq!(used, 4096);
        assert_eq!(total, 4096 + 6 + 8 * 128); // used_len = flags+idx+ring+avail_event
        // qsize=256 (net default): desc 4096 → avail@4096, used@8192
        let (avail, used, _) = queue_layout(256);
        assert_eq!(avail, 4096);
        assert_eq!(used, 8192);
        // qsize=64: desc 1024, avail@1024(134B) → used alinhado 4096
        let (avail, used, total) = queue_layout(64);
        assert_eq!(avail, 1024);
        assert_eq!(used, 4096);
        assert_eq!(total, 4096 + 6 + 8 * 64);
        // used_off sempre alinhado a 4096
        for q in [1u16, 8, 64, 128, 256, 512] {
            let (_, used, total) = queue_layout(q);
            assert_eq!(used % 4096, 0);
            assert!(total >= used + 6 + 8 * q as u32);
        }
    }
}
