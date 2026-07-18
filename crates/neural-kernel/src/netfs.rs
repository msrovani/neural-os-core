//! Network Filesystem (#418) — remote FS via TCP peer on gateway:4446.
//! Protocol: cmd(1) + len(4 LE) + payload. cmd=0 READ, 1 WRITE, 2 LIST.
//! Guest → NET_CONFIG.gateway_ip (QEMU slirp 10.0.2.2), not dns_ip.
//! Peer: `python tools/netfs_peer.py` (LIST = newline-separated; dirs end with /).

use alloc::string::String;
use alloc::vec::Vec;
use crate::fs_driver::{FilesystemDriver, FsInfo};
use core::sync::atomic::{AtomicBool, Ordering};

const NETFS_PORT: u16 = 4446;
static SMOKE_DONE: AtomicBool = AtomicBool::new(false);

fn gateway_ip() -> [u8; 4] {
    let cfg = crate::net::NET_CONFIG.lock();
    let gw = cfg.gateway_ip;
    if gw != [0; 4] {
        gw
    } else {
        [10, 0, 2, 2]
    }
}

fn netfs_send(cmd: u8, payload: &[u8]) -> Option<Vec<u8>> {
    let host = gateway_ip();
    let mut msg = Vec::with_capacity(5 + payload.len());
    msg.push(cmd);
    msg.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    msg.extend_from_slice(payload);
    unsafe { crate::net::tcp_exchange(host, NETFS_PORT, &msg) }
}

/// Parse LIST body: newline-separated names; trailing `/` ⇒ directory.
pub fn parse_list_entries(data: &[u8]) -> Vec<(String, bool)> {
    let text = core::str::from_utf8(data).unwrap_or("");
    let mut out = Vec::new();
    for line in text.split(['\n', '\r']) {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(dir) = name.strip_suffix('/') {
            if !dir.is_empty() {
                out.push((String::from(dir), true));
            }
        } else {
            out.push((String::from(name), false));
        }
    }
    out
}

/// Best-effort LIST/WRITE/READ roundtrip after L5. Non-fatal.
pub fn smoke_if_online() {
    if SMOKE_DONE.swap(true, Ordering::Relaxed) {
        return;
    }
    if crate::network_agent::early_smoke_status() != "L5_OK" {
        k_nano::slog_bin!("NETFS", "info", "smoke skip (net not L5_OK)");
        return;
    }
    let list = match netfs_send(2, b".") {
        Some(d) => d,
        None => {
            k_nano::slog_bin!("NETFS", "info", "VERDICT=FAIL reason=list");
            return;
        }
    };
    let entries = parse_list_entries(&list);
    if entries.is_empty() {
        k_nano::slog_bin!("NETFS", "info", "VERDICT=FAIL reason=empty_list");
        return;
    }
    let marker = b"neural-netfs-smoke";
    let mut wpay = Vec::with_capacity(20 + marker.len());
    wpay.extend_from_slice(b"smoke.tmp");
    wpay.push(0);
    wpay.extend_from_slice(marker);
    if netfs_send(1, &wpay).is_none() {
        k_nano::slog_bin!("NETFS", "info", "VERDICT=FAIL reason=write");
        return;
    }
    match netfs_send(0, b"smoke.tmp") {
        Some(body) if body.as_slice() == marker => {
            k_nano::slog_bin!(
                "NETFS",
                "info",
                "VERDICT=PASS list={} write/read ok",
                entries.len()
            );
        }
        Some(_) => k_nano::slog_bin!("NETFS", "info", "VERDICT=FAIL reason=read_mismatch"),
        None => k_nano::slog_bin!("NETFS", "info", "VERDICT=FAIL reason=read"),
    }
}

pub struct NetFs {
    pub name: String,
    pub mount_point: String,
    pub backend: String, // "webdav", "nfs", "s3"
}

impl NetFs {
    pub fn new(name: &str, mount: &str, backend: &str) -> Self {
        NetFs {
            name: String::from(name),
            mount_point: String::from(mount),
            backend: String::from(backend),
        }
    }
}

impl FilesystemDriver for NetFs {
    fn name(&self) -> &str {
        &self.name
    }

    fn detect(_dev: &mut dyn crate::block_dev::BlockDevice, _lba: u64) -> Option<Self> {
        None
    }

    fn mount(
        &mut self,
        _dev: &mut dyn crate::block_dev::BlockDevice,
        _start_lba: u64,
    ) -> Result<FsInfo, &'static str> {
        Ok(FsInfo {
            fs_type: "netfs",
            label: self.name.clone(),
            total_bytes: 0,
            free_bytes: None,
            block_size: 512,
            writable: true,
        })
    }

    fn read(&self, path: &str, _offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        let data = netfs_send(0, path.as_bytes()).ok_or("netfs read failed")?;
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    fn write(&mut self, path: &str, _offset: u64, data: &[u8]) -> Result<(), &'static str> {
        let mut payload = Vec::with_capacity(path.len() + 1 + data.len());
        payload.extend_from_slice(path.as_bytes());
        payload.push(0);
        payload.extend_from_slice(data);
        netfs_send(1, &payload).ok_or("netfs write failed")?;
        Ok(())
    }

    fn list(&self, path: &str) -> Result<Vec<(String, bool)>, &'static str> {
        let data = netfs_send(2, path.as_bytes()).ok_or("netfs list failed")?;
        Ok(parse_list_entries(&data))
    }

    fn free_space(&self) -> u64 {
        0
    }
    fn total_space(&self) -> u64 {
        0
    }
}
