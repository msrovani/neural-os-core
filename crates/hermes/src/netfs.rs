//! Network Filesystem — monta sistemas de arquivos remotos via serial tunnel.
//! Usa o mesmo padrao do serial_bridge.py: TCP ao host na porta 4446.
//! Protocolo: cmd(1) + len(4) + payload.
//! cmd=0 READ, cmd=1 WRITE, cmd=2 LIST.

use alloc::vec::Vec;
use alloc::string::String;
use k_nano::fs_driver::{FilesystemDriver, FsInfo};

const NETFS_PORT: u16 = 4446;

fn netfs_send(cmd: u8, payload: &[u8]) -> Option<Vec<u8>> {
    let cfg = crate::net::NET_CONFIG.lock();
    let dns = cfg.dns_ip;
    drop(cfg);
    let mut msg = Vec::with_capacity(5 + payload.len());
    msg.push(cmd);
    msg.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    msg.extend_from_slice(payload);
    let raw = unsafe { crate::net::http_get_raw(dns, NETFS_PORT, &msg) };
    raw
}

pub struct NetFs {
    pub name: String,
    pub mount_point: String,
    pub backend: String, // "webdav", "nfs", "s3"
}

impl NetFs {
    pub fn new(name: &str, mount: &str, backend: &str) -> Self {
        NetFs { name: String::from(name), mount_point: String::from(mount), backend: String::from(backend) }
    }
}

impl FilesystemDriver for NetFs {
    fn name(&self) -> &str { &self.name }

    fn detect(_dev: &mut dyn k_nano::block_dev::BlockDevice, _lba: u64) -> Option<Self> { None }

    fn mount(&mut self, _dev: &mut dyn k_nano::block_dev::BlockDevice, _start_lba: u64) -> Result<FsInfo, &'static str> {
        Ok(FsInfo { fs_type: "netfs", label: self.name.clone(), total_bytes: 0,
            free_bytes: None, block_size: 512, writable: true })
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
        let _text = core::str::from_utf8(&data).unwrap_or("[]");
        let entries: Vec<String> = alloc::vec!["netfs_remote".into()];
        Ok(entries.into_iter().map(|n| (n, false)).collect())
    }

    fn free_space(&self) -> u64 { 0 }
    fn total_space(&self) -> u64 { 0 }
}
