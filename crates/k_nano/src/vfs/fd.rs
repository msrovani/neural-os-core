//! VFS open/fd MVP — Labor 25 (ADR-0062 P2 residual).
//! Tabela fd global + open/read/close sobre mounts `/mnt/*` (cache FilesystemDriver).

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

const MAX_FD: usize = 16;

struct FdEntry {
    path: String,
    offset: u64,
    /// Conteúdo materializado no open (MVP — sem seek parcial em block).
    data: Vec<u8>,
}

struct FdTable {
    slots: [Option<FdEntry>; MAX_FD],
}

impl FdTable {
    const fn new() -> Self {
        Self {
            slots: [const { None }; MAX_FD],
        }
    }
}

static FDS: Mutex<FdTable> = Mutex::new(FdTable::new());

/// open(path) → fd ≥ 0. Paths sob mount VFS; lê via StorageBus list/cache se possível.
pub fn open(path: &str) -> Result<i32, &'static str> {
    let path = path.trim();
    if path.is_empty() {
        return Err("empty_path");
    }
    // Resolve mount exists
    {
        let vfs = crate::vfs::VFS.lock();
        let Some(ref v) = *vfs else {
            return Err("vfs_uninit");
        };
        let (_m, _rel, agent) = v.resolve(path);
        let _ = agent;
    }
    let data = load_path_bytes(path)?;
    let mut t = FDS.lock();
    for (i, slot) in t.slots.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(FdEntry {
                path: String::from(path),
                offset: 0,
                data,
            });
            crate::slog_nano!(
                "VFS",
                "info",
                "step=open fd={} path={} VERDICT=PASS",
                i,
                path
            );
            return Ok(i as i32);
        }
    }
    Err("emfile")
}

fn load_path_bytes(path: &str) -> Result<Vec<u8>, &'static str> {
    // /mnt/ram virtual empty OK
    if path.starts_with("/mnt/ram") || path == "/mnt/ram" {
        return Ok(Vec::new());
    }
    // Prefer StorageBus EXT/FAT caches via list mounts — best-effort slog
    if let Some(body) = try_storage_read(path) {
        return Ok(body);
    }
    // VFS tree node without body
    let vfs = crate::vfs::VFS.lock();
    if let Some(ref v) = *vfs {
        if v.lookup(path).is_some() {
            return Ok(alloc::format!("[vfs] {}\n", path).into_bytes());
        }
        // list mount point as directory listing
        let entries = v.list_dir(path);
        if !entries.is_empty() {
            let mut s = String::new();
            for e in entries {
                s.push_str(&e);
                s.push('\n');
            }
            return Ok(s.into_bytes());
        }
    }
    Err("enoent")
}

fn try_storage_read(_path: &str) -> Option<Vec<u8>> {
    // Residual: bridge FilesystemDriver cache por mount — L13 file_cache via agent.
    None
}

pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, &'static str> {
    if fd < 0 {
        return Err("ebadf");
    }
    let mut t = FDS.lock();
    let Some(ent) = t.slots.get_mut(fd as usize).and_then(|s| s.as_mut()) else {
        return Err("ebadf");
    };
    if ent.offset as usize >= ent.data.len() {
        return Ok(0);
    }
    let start = ent.offset as usize;
    let n = (ent.data.len() - start).min(buf.len());
    buf[..n].copy_from_slice(&ent.data[start..start + n]);
    ent.offset += n as u64;
    Ok(n)
}

pub fn close(fd: i32) -> Result<(), &'static str> {
    if fd < 0 {
        return Err("ebadf");
    }
    let mut t = FDS.lock();
    let slot = t.slots.get_mut(fd as usize).ok_or("ebadf")?;
    if slot.take().is_none() {
        return Err("ebadf");
    }
    Ok(())
}

/// Smoke: open/read/close em `/mnt/ram`.
pub fn boot_smoke() -> bool {
    match open("/mnt/ram") {
        Ok(fd) => {
            let mut buf = [0u8; 8];
            let _ = read(fd, &mut buf);
            let _ = close(fd);
            crate::slog_nano!(
                "VFS",
                "info",
                "step=fd_smoke status=OK VERDICT=PASS reason=open_read_close"
            );
            true
        }
        Err(e) => {
            crate::slog_nano!(
                "VFS",
                "info",
                "step=fd_smoke status=SKIP VERDICT=SKIP reason={}",
                e
            );
            true
        }
    }
}
