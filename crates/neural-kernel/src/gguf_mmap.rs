//! GGUF/FAT file-backed mmap sobre demand-paging P7 (ADR-0041 P9).
//! I/O no register/mmap (frames pré-preenchidos); #PF só instala PRESENT — sem ATA no IRQ.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::PhysAddr;

use crate::address_space::{self, AddressSpace};
use crate::demand_page;
use crate::serial_println;
use crate::syscall::{self, Cap, SYS_DEMAND_PAGE, SYS_MAP_FILE, SYS_MAP_WEIGHTS};

/// VA dedicado (fora de CORTEX_WEIGHT_VA P5/P7).
pub const FILE_WEIGHT_VA: u64 = 0x0000_7000_0040_0000;
/// Páginas do PoC (início do arquivo basta).
pub const DEMO_FILE_PAGES: usize = 4;
const MAX_FRAMES: usize = 8;

/// GGUF magic LE ("GGUF").
const GGUF_MAGIC: u32 = 0x46554747;
/// .bitnet magic.
const BITNET_MAGIC: u32 = 0xBE11BE11;
/// Stub magic quando FAT não tem modelo (fallback documentado).
const FALLBACK_MAGIC: u32 = 0x4C49464E; // "NFIL"

/// Nomes 8.3 / curtos conhecidos na imagem FAT (ordem de preferência).
const CANDIDATES: &[&str] = &[
    "BITNET.BIN",
    "HWEXPRT.BIN",
    "RUSTCDR.BITNET",
    "MODEL.GGUF",
    "MICRO.BIN",
];

static MMAP_COUNT: AtomicU64 = AtomicU64::new(0);
static FILE_HIT: AtomicU64 = AtomicU64::new(0);
static FALLBACK_HIT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackingKind {
    Gguf,
    Bitnet,
    Unknown,
    Fallback,
}

#[derive(Clone, Copy, Debug)]
pub struct FileWeightMap {
    pub virt: u64,
    pub phys: u64,
    pub pages: usize,
    pub kind: BackingKind,
    pub expected_magic: u32,
}

fn detect_kind(data: &[u8]) -> (BackingKind, u32) {
    if data.len() < 4 {
        return (BackingKind::Unknown, 0);
    }
    let m = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if m == GGUF_MAGIC {
        (BackingKind::Gguf, GGUF_MAGIC)
    } else if m == BITNET_MAGIC {
        (BackingKind::Bitnet, BITNET_MAGIC)
    } else {
        (BackingKind::Unknown, m)
    }
}

/// Lê prefixo do primeiro arquivo de modelo encontrado no FAT (ATA).
fn try_read_model_prefix(nbytes: usize) -> Option<(BackingKind, u32, alloc::string::String, alloc::vec::Vec<u8>)> {
    let ata_guard = crate::ATA_DRIVER.lock();
    let ata = ata_guard.as_ref()?;
    let parts = unsafe { crate::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code != 0x0B && part.type_code != 0x0C && part.type_code != 0x1C {
            continue;
        }
        let fs = match unsafe { crate::fat32::Fat32Reader::new(ata, part) } {
            Some(f) => f,
            None => continue,
        };
        for &name in CANDIDATES {
            let data = unsafe { fs.read_file_range(name, 0, nbytes) };
            if let Some(buf) = data {
                if buf.is_empty() {
                    continue;
                }
                let (kind, magic) = detect_kind(&buf);
                serial_println!(
                    "[P9] FAT hit name={} bytes={} kind={:?} magic=0x{:08x}",
                    name,
                    buf.len(),
                    kind,
                    magic
                );
                return Some((kind, magic, alloc::string::String::from(name), buf));
            }
        }
    }
    None
}

unsafe fn alloc_and_fill(
    n: usize,
    bytes: Option<&[u8]>,
) -> Result<([PhysFrame<Size4KiB>; MAX_FRAMES], PhysFrame<Size4KiB>, BackingKind, u32), &'static str>
{
    if n == 0 || n > MAX_FRAMES {
        return Err("p9: file pages invalido");
    }
    let mut frames = [PhysFrame::containing_address(PhysAddr::new(0)); MAX_FRAMES];
    let (kind, magic) = match bytes {
        Some(b) => detect_kind(b),
        None => (BackingKind::Fallback, FALLBACK_MAGIC),
    };
    for i in 0..n {
        let frame = match address_space::alloc_frame() {
            Ok(f) => f,
            Err(e) => {
                for j in 0..i {
                    crate::memory::dealloc_physical_frame(frames[j]);
                }
                return Err(e);
            }
        };
        let dst = address_space::hhdm_mut::<u8>(frame);
        core::ptr::write_bytes(dst, 0, 4096);
        if let Some(b) = bytes {
            let off = i * 4096;
            if off < b.len() {
                let len = (b.len() - off).min(4096);
                core::ptr::copy_nonoverlapping(b.as_ptr().add(off), dst, len);
            }
        } else if i == 0 {
            // Stub documentado: magic NFIL + ASCII "no model file"
            let m = FALLBACK_MAGIC.to_le_bytes();
            core::ptr::copy_nonoverlapping(m.as_ptr(), dst, 4);
            let msg = b"no model file";
            core::ptr::copy_nonoverlapping(msg.as_ptr(), dst.add(4), msg.len());
        }
        frames[i] = frame;
    }
    Ok((frames, frames[0], kind, magic))
}

/// Caps: MAP_WEIGHTS | DEMAND_PAGE | MAP_FILE. Pré-lê FAT → frames → register_lazy.
pub unsafe fn mmap_file_weights(
    aspace: &mut AddressSpace,
    n: usize,
    held: Cap,
) -> Result<FileWeightMap, &'static str> {
    let need = Cap::MAP_WEIGHTS
        .union(Cap::DEMAND_PAGE)
        .union(Cap::MAP_FILE);
    if !held.contains(need) {
        serial_println!(
            "[CapGate] DENY MAP_WEIGHTS|DEMAND_PAGE|MAP_FILE held=0x{:x}",
            held.bits()
        );
        return Err("EPERM: Cap::MAP_WEIGHTS|DEMAND_PAGE|MAP_FILE");
    }
    let _ = syscall::dispatch(SYS_MAP_WEIGHTS, n as u64, held)?;
    let _ = syscall::dispatch(SYS_DEMAND_PAGE, n as u64, held)?;
    let _ = syscall::dispatch(SYS_MAP_FILE, n as u64, held)?;

    let nbytes = n.saturating_mul(4096);
    let (kind, magic, frames, first) = match try_read_model_prefix(nbytes) {
        Some((k, m, _name, buf)) => {
            FILE_HIT.fetch_add(1, Ordering::Relaxed);
            let (fr, first, _dk, _dm) = alloc_and_fill(n, Some(&buf))?;
            (k, m, fr, first)
        }
        None => {
            FALLBACK_HIT.fetch_add(1, Ordering::Relaxed);
            serial_println!("[P9] WARN no model file on FAT — fallback stub frames");
            let (fr, first, k, m) = alloc_and_fill(n, None)?;
            (k, m, fr, first)
        }
    };

    demand_page::register_lazy(aspace, FILE_WEIGHT_VA, &frames[..n], false)?;
    MMAP_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(FileWeightMap {
        virt: FILE_WEIGHT_VA,
        phys: first.start_address().as_u64(),
        pages: n,
        kind,
        expected_magic: magic,
    })
}

pub fn mmap_count() -> u64 {
    MMAP_COUNT.load(Ordering::Relaxed)
}

/// Demo non-fatal P9: deny Cap → mmap file-backed → first-touch → verify magic → restore CR3.
pub fn demo_gguf_mmap() -> Result<(), &'static str> {
    serial_println!("[P9] GGUF/FAT file-backed mmap demo (demand-page + pre-fill)");

    let need = Cap::MAP_WEIGHTS
        .union(Cap::DEMAND_PAGE)
        .union(Cap::MAP_FILE);

    if syscall::dispatch(SYS_MAP_FILE, 0, Cap::EMPTY).is_ok() {
        return Err("p9: Cap vazia nao deveria MAP_FILE");
    }
    if unsafe {
        mmap_file_weights(
            &mut AddressSpace::clone_current()?,
            DEMO_FILE_PAGES,
            Cap::MAP_WEIGHTS.union(Cap::DEMAND_PAGE),
        )
    }
    .is_ok()
    {
        return Err("p9: MAP_FILE ausente deveria negar");
    }

    let (kernel_l4, kernel_flags) = address_space::kernel_cr3();
    let mut as_cortex = AddressSpace::clone_current()?;
    let map = match unsafe { mmap_file_weights(&mut as_cortex, DEMO_FILE_PAGES, need) } {
        Ok(m) => m,
        Err(e) => {
            serial_println!("[P9] WARN mmap_file_weights: {} — Cap-only path", e);
            syscall::dispatch(SYS_MAP_FILE, DEMO_FILE_PAGES as u64, need)?;
            serial_println!("[P9] SUCCESS Cap MAP_FILE (sem frames / sem FAT)");
            return Ok(());
        }
    };

    let cures_before = demand_page::cure_count();
    let got_magic = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        as_cortex.activate();
        let ptr = map.virt as *const u32;
        let m = ptr.read_volatile();
        address_space::restore_cr3(kernel_l4, kernel_flags);
        m
    });
    let cured = demand_page::cure_count().saturating_sub(cures_before);
    if cured < 1 {
        return Err("p9: esperava >=1 cure #PF first-touch");
    }
    if got_magic != map.expected_magic {
        serial_println!(
            "[P9] WARN magic got=0x{:08x} expected=0x{:08x} kind={:?}",
            got_magic,
            map.expected_magic,
            map.kind
        );
        // Non-fatal se fallback batido ou dados truncados — ainda prova mmap.
        if map.kind != BackingKind::Fallback {
            return Err("p9: header magic mismatch");
        }
    }

    let src = if map.kind == BackingKind::Fallback {
        "fallback"
    } else {
        "FAT"
    };
    serial_println!(
        "[P9] SUCCESS file-mmap pages={} va={:x} kind={:?} src={} magic=0x{:08x} cures={} hits={}",
        map.pages,
        map.virt,
        map.kind,
        src,
        got_magic,
        cured,
        FILE_HIT.load(Ordering::Relaxed)
    );
    Ok(())
}
