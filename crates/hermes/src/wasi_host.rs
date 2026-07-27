//! ADR-0076 F5 — WASI Preview 2 host function stubs (wasi_snapshot_preview1).
//!
//! Implements the `wasi_snapshot_preview1` flat namespace — the most widely
//! supported ABI for wasm32-wasi binaries. Sufficient for "hello world" WASI
//! modules and basic file-less workloads.
//!
//! # ponytail
//! - Single preopen entry: fd=3 → "/"
//! - Empty environment and args
//! - `clock_time_get` returns TIMER_TICKS (coarse, not wall-clock)
//! - `random_get` returns zeroed memory (NOT cryptographically random)
//! - No real I/O beyond stderr/stdout logging
//! - Per-instance WASI state when multi-sandbox is needed

use super::wasmi_rt::HostState;
use wasmi::Linker;

// ─── WASI Preview 1 ABI constants ──────────────────────────────────────────

/// WASI errno values (subset: common ones for stub behavior).
pub mod errno {
    pub const SUCCESS: u32 = 0;
    pub const BADF: u32 = 8;
    pub const NOSYS: u32 = 52;
    pub const NOTSUP: u32 = 58;
}

/// WASI open flags.
pub mod oflags {
    pub const CREAT: u16 = 1;
    pub const DIRECTORY: u16 = 2;
    pub const EXCL: u16 = 4;
    pub const TRUNC: u16 = 8;
}

/// WASI descriptor flags.
pub mod fdflags {
    pub const APPEND: u16 = 1;
}

/// WASI lookup flags.
pub mod lookupflags {
    pub const SYMLINK_FOLLOW: u32 = 1;
}

/// WASI rights bitmask (simplified).
pub mod rights {
    pub const FD_READ: u64 = 1 << 1;
    pub const FD_WRITE: u64 = 1 << 5;
    pub const FD_SEEK: u64 = 1 << 2;
    pub const PATH_OPEN: u64 = 1 << 8;
    pub const FD_READDIR: u64 = 1 << 12;
}

// ─── registration ──────────────────────────────────────────────────────────

/// Register `wasi_snapshot_preview1` host function stubs on `linker`.
///
/// All stubs log via `slog_hermes!` and return sensible defaults.
/// `fd_write` (stdout/stderr) and `fd_prestat_get` (fd=3) are functional
/// enough for wasi-libc startup and basic output.
///
/// # Note — parameter order
/// wasmi 0.47 requires `Caller` as the **first** parameter of `func_wrap`
/// closures. All WASI params follow after `caller`.
pub fn register_wasi_host_functions(
    linker: &mut Linker<HostState>,
) -> Result<(), wasmi::Error> {
    use wasmi::Caller;

    // ── fd_write (stdout/stderr) ───────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_write",
        |mut caller: Caller<'_, HostState>,
         fd: u32,
         iovs: u32,
         iovs_len: u32,
         nwritten: u32|
         -> u32 {
            if fd != 1 && fd != 2 {
                return errno::BADF;
            }
            let mem = match caller.get_export("memory") {
                Some(wasmi::Extern::Memory(m)) => m,
                _ => return errno::NOSYS,
            };
            // Read iov array (each iov = {buf:u32, len:u32} = 8 bytes)
            let total = {
                let d = mem.data(&caller);
                let base = iovs as usize;
                let n = iovs_len as usize;
                let mut acc = 0u32;
                for i in 0..n {
                    let off = base + i * 8;
                    if off + 8 <= d.len() {
                        let len = u32::from_le_bytes(
                            d[off + 4..off + 8].try_into().unwrap_or([0; 4]),
                        );
                        acc = acc.saturating_add(len);
                    }
                }
                acc
            };
            // Write nwritten to guest memory
            let nw = nwritten as usize;
            if nw + 4 <= mem.data(&caller).len() {
                mem.data_mut(&mut caller)[nw..nw + 4]
                    .copy_from_slice(&total.to_le_bytes());
            }
            k_nano::slog_hermes!(
                "WASI", "info",
                "fd_write fd={} iovs={} total={}", fd, iovs_len, total
            );
            errno::SUCCESS
        },
    )?;

    // ── fd_close ───────────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_close",
        |fd: u32| -> u32 {
            // ponytail: real close would free the fd
            if fd <= 2 {
                // Don't close stdin/stdout/stderr — pretend it's fine
                return errno::SUCCESS;
            }
            errno::SUCCESS
        },
    )?;

    // ── fd_seek ────────────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_seek",
        |mut caller: Caller<'_, HostState>,
         fd: u32,
         _offset: i64,
         _whence: u32,
         newoffset: u32|
         -> u32 {
            // ponytail: always "success" at position 0
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let off = newoffset as usize;
                if off + 8 <= mem.data(&caller).len() {
                    mem.data_mut(&mut caller)[off..off + 8]
                        .copy_from_slice(&0u64.to_le_bytes());
                }
            }
            k_nano::slog_hermes!("WASI", "info", "fd_seek fd={} (stub→0)", fd);
            errno::SUCCESS
        },
    )?;

    // ── fd_prestat_get ─────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_prestat_get",
        |mut caller: Caller<'_, HostState>, fd: u32, buf: u32| -> u32 {
            if fd != 3 {
                return errno::BADF;
            }
            // Write prestat = {tag:u8=0(PREOPENTYPE_DIR), pad:[u8;3], name_len:u32}
            // Total 8 bytes: [0,0,0,0, 1,0,0,0] (name_len=1 → "/")
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let b = buf as usize;
                if b + 8 <= mem.data(&caller).len() {
                    let dst = &mut mem.data_mut(&mut caller)[b..b + 8];
                    dst.copy_from_slice(&[0u8, 0, 0, 0, 1, 0, 0, 0]);
                }
            }
            k_nano::slog_hermes!("WASI", "info", "fd_prestat_get fd=3 → SUCCESS (preopen /)");
            errno::SUCCESS
        },
    )?;

    // ── fd_prestat_dir_name ────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_prestat_dir_name",
        |mut caller: Caller<'_, HostState>, fd: u32, path: u32, path_len: u32| -> u32 {
            if fd != 3 {
                return errno::BADF;
            }
            // Write "/" (1 byte) to guest memory
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let p = path as usize;
                let len = path_len as usize;
                let max = core::cmp::min(1, len);
                if p + max <= mem.data(&caller).len() {
                    mem.data_mut(&mut caller)[p..p + max].copy_from_slice(b"/");
                }
            }
            k_nano::slog_hermes!("WASI", "info", "fd_prestat_dir_name fd=3 path=/ (stub)");
            errno::SUCCESS
        },
    )?;

    // ── environ_sizes_get ──────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        |mut caller: Caller<'_, HostState>, count: u32, buf_size: u32| -> u32 {
            // Write count=0, buf_size=0 → empty environment
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let max = mem.data(&caller).len();
                let c = count as usize;
                let b = buf_size as usize;
                if c + 4 <= max {
                    mem.data_mut(&mut caller)[c..c + 4]
                        .copy_from_slice(&0u32.to_le_bytes());
                }
                if b + 4 <= max {
                    mem.data_mut(&mut caller)[b..b + 4]
                        .copy_from_slice(&0u32.to_le_bytes());
                }
            }
            errno::SUCCESS
        },
    )?;

    // ── environ_get ────────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_get",
        |_environ: u32, _environ_buf: u32| -> u32 {
            // Empty env — nothing to write
            errno::SUCCESS
        },
    )?;

    // ── args_sizes_get ─────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "args_sizes_get",
        |mut caller: Caller<'_, HostState>, count: u32, buf_size: u32| -> u32 {
            // Write count=0, buf_size=0 → no args
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let max = mem.data(&caller).len();
                let c = count as usize;
                let b = buf_size as usize;
                if c + 4 <= max {
                    mem.data_mut(&mut caller)[c..c + 4]
                        .copy_from_slice(&0u32.to_le_bytes());
                }
                if b + 4 <= max {
                    mem.data_mut(&mut caller)[b..b + 4]
                        .copy_from_slice(&0u32.to_le_bytes());
                }
            }
            errno::SUCCESS
        },
    )?;

    // ── args_get ───────────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "args_get",
        |_argv: u32, _argv_buf: u32| -> u32 {
            // No args — nothing to write
            errno::SUCCESS
        },
    )?;

    // ── proc_exit ──────────────────────────────────────────────────────────
    linker.func_wrap("wasi_snapshot_preview1", "proc_exit", |code: u32| {
        k_nano::slog_hermes!(
            "WASI", "info",
            "proc_exit code={} (stub — host does not exit)", code
        );
        // ponytail: host doesn't actually exit — just logs.
        // The wasm module continues; harmless if called as last instruction.
    })?;

    // ── random_get ─────────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "random_get",
        |mut caller: Caller<'_, HostState>, buf: u32, buf_len: u32| -> u32 {
            // Zero-fill the requested buffer (NOT cryptographically random)
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let start = buf as usize;
                let len = buf_len as usize;
                let max = mem.data(&caller).len();
                let end = core::cmp::min(start.saturating_add(len), max);
                if start < end {
                    mem.data_mut(&mut caller)[start..end].fill(0);
                }
            }
            k_nano::slog_hermes!("WASI", "info", "random_get len={} (zero stub)", buf_len);
            errno::SUCCESS
        },
    )?;

    // ── clock_time_get ─────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "clock_time_get",
        |mut caller: Caller<'_, HostState>,
         id: u32,
         _precision: u64,
         time: u32|
         -> u32 {
            let tick = k_nano::interrupts::TIMER_TICKS
                .load(core::sync::atomic::Ordering::Relaxed) as u64;
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let t = time as usize;
                if t + 8 <= mem.data(&caller).len() {
                    mem.data_mut(&mut caller)[t..t + 8]
                        .copy_from_slice(&tick.to_le_bytes());
                }
            }
            k_nano::slog_hermes!(
                "WASI", "info",
                "clock_time_get id={} tick={} (stub)", id, tick
            );
            errno::SUCCESS
        },
    )?;

    // ── path_open ──────────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "path_open",
        |mut caller: Caller<'_, HostState>,
         fd: u32,
         dirflags: u32,
         _path: u32,
         _path_len: u32,
         oflags: u32,
         _fs_rights_base: u64,
         _fs_rights_inheriting: u64,
         _fdflags: u32,
         opened_fd: u32|
         -> u32 {
            // ponytail: always "open" a new fake fd (4)
            let fake_fd = 4u32;
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let o = opened_fd as usize;
                if o + 4 <= mem.data(&caller).len() {
                    mem.data_mut(&mut caller)[o..o + 4]
                        .copy_from_slice(&fake_fd.to_le_bytes());
                }
            }
            k_nano::slog_hermes!(
                "WASI", "info",
                "path_open fd={} dirflags={} oflags={} → fake_fd={} (stub)",
                fd, dirflags, oflags, fake_fd
            );
            errno::SUCCESS
        },
    )?;

    // ── fd_read (stdin stub → EOF) ─────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_read",
        |mut caller: Caller<'_, HostState>,
         fd: u32,
         _iovs: u32,
         iovs_len: u32,
         nread: u32|
         -> u32 {
            if fd != 0 {
                return errno::BADF;
            }
            // Return "read 0 bytes" (EOF)
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let nr = nread as usize;
                if nr + 4 <= mem.data(&caller).len() {
                    mem.data_mut(&mut caller)[nr..nr + 4]
                        .copy_from_slice(&0u32.to_le_bytes());
                }
            }
            k_nano::slog_hermes!(
                "WASI", "info",
                "fd_read fd=0 iovs={} → 0 bytes (EOF stub)", iovs_len
            );
            errno::SUCCESS
        },
    )?;

    // ── fd_fdstat_get ──────────────────────────────────────────────────────
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_fdstat_get",
        |mut caller: Caller<'_, HostState>, fd: u32, buf: u32| -> u32 {
            // Write minimal fdstat (24 bytes): fs_filetype=2 (CHARACTER_DEVICE),
            // fs_flags=0, fs_rights_base=all, fs_rights_inheriting=0
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let b = buf as usize;
                let max = mem.data(&caller).len();
                if b + 24 <= max {
                    let dst = &mut mem.data_mut(&mut caller)[b..b + 24];
                    dst[0] = 2; // filetype = CHARACTER_DEVICE
                    dst[1..4].fill(0); // padding
                    dst[4..6].fill(0); // fs_flags = 0
                    dst[6..8].fill(0); // padding
                    dst[8..16].fill(0xFF); // fs_rights_base = all
                    dst[16..24].fill(0); // fs_rights_inheriting = 0
                }
            }
            k_nano::slog_hermes!("WASI", "info", "fd_fdstat_get fd={} (stub)", fd);
            errno::SUCCESS
        },
    )?;

    Ok(())
}
