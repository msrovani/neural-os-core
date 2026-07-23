//! ADR-0063 F1a — TickvLite: append-log KV mínimo (honesty: não é crate tickv upstream).
//! Record: magic TKLV | u32 key_len | u32 val_len | u32 crc | key | val | pad to 16.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::flash::{init_flash, ActiveFlash, FlashController, FLASH};

const MAGIC: &[u8; 4] = b"TKLV";
const HEADER: usize = 16; // magic4 + klen4 + vlen4 + crc4

fn crc32(data: &[u8]) -> u32 {
    // IEEE CRC32 simples
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = if crc & 1 != 0 { 0xEDB8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

pub struct TickvLite {
    /// Índice em RAM: key → offset no flash
    index: BTreeMap<String, u64>,
    /// Próximo offset livre (append)
    append_off: u64,
    ready: bool,
    backend: &'static str,
}

impl TickvLite {
    pub fn new() -> Self {
        TickvLite {
            index: BTreeMap::new(),
            append_off: 0,
            ready: false,
            backend: "none",
        }
    }

    pub fn backend(&self) -> &str {
        self.backend
    }
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn mount(&mut self) -> Result<(), &'static str> {
        self.backend = init_flash();
        self.recover()?;
        self.ready = true;
        Ok(())
    }

    fn with_flash<R>(&self, f: impl FnOnce(&mut ActiveFlash) -> R) -> Result<R, &'static str> {
        let mut g = FLASH.lock();
        let flash = g.as_mut().ok_or("no flash")?;
        Ok(f(flash))
    }

    fn recover(&mut self) -> Result<(), &'static str> {
        self.index.clear();
        self.append_off = 0;
        let size = self.with_flash(|fl| fl.size_bytes())?;
        let mut off = 0u64;
        let mut hdr = [0u8; HEADER];
        while off + HEADER as u64 <= size {
            self.with_flash(|fl| fl.read(off, &mut hdr))??;
            if &hdr[0..4] != MAGIC {
                break;
            }
            let klen = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
            let vlen = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
            let want_crc = u32::from_le_bytes(hdr[12..16].try_into().unwrap());
            let body_len = klen + vlen;
            let padded = (body_len + 15) & !15;
            if off + HEADER as u64 + padded as u64 > size {
                break;
            }
            let mut body = vec![0u8; body_len];
            self.with_flash(|fl| fl.read(off + HEADER as u64, &mut body))??;
            let got = crc32(&body);
            if got != want_crc {
                break; // corrupt → stop append chain
            }
            let key = core::str::from_utf8(&body[..klen])
                .map_err(|_| "utf8")?
                .to_string();
            // tombstone: vlen==0 means delete
            if vlen == 0 {
                self.index.remove(&key);
            } else {
                self.index.insert(key, off);
            }
            let rec_body = HEADER + padded;
            let total = (rec_body + 511) & !511;
            off += total as u64;
        }
        self.append_off = off;
        Ok(())
    }

    pub fn put(&mut self, key: &str, val: &[u8]) -> Result<(), &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        let k = key.as_bytes();
        let body_len = k.len() + val.len();
        let padded = (body_len + 15) & !15;
        let mut body = vec![0u8; padded];
        body[..k.len()].copy_from_slice(k);
        body[k.len()..k.len() + val.len()].copy_from_slice(val);
        let crc = crc32(&body[..body_len]);
        let mut rec = vec![0u8; HEADER + padded];
        rec[0..4].copy_from_slice(MAGIC);
        rec[4..8].copy_from_slice(&(k.len() as u32).to_le_bytes());
        rec[8..12].copy_from_slice(&(val.len() as u32).to_le_bytes());
        rec[12..16].copy_from_slice(&crc.to_le_bytes());
        rec[HEADER..HEADER + padded].copy_from_slice(&body);
        // pad whole record to 512
        let total = (rec.len() + 511) & !511;
        rec.resize(total, 0);
        let off = self.append_off;
        self.with_flash(|fl| fl.write(off, &rec))??;
        self.index.insert(String::from(key), off);
        self.append_off = off + total as u64;
        Ok(())
    }

    pub fn get(&mut self, key: &str) -> Result<Vec<u8>, &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        let off = *self.index.get(key).ok_or("missing")?;
        let mut hdr = [0u8; HEADER];
        self.with_flash(|fl| fl.read(off, &mut hdr))??;
        let klen = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
        let vlen = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
        let mut body = vec![0u8; klen + vlen];
        self.with_flash(|fl| fl.read(off + HEADER as u64, &mut body))??;
        Ok(body[klen..].to_vec())
    }

    pub fn delete(&mut self, key: &str) -> Result<(), &'static str> {
        self.put(key, &[])?;
        self.index.remove(key);
        Ok(())
    }
}

pub static TICKV: Mutex<Option<TickvLite>> = Mutex::new(None);

/// Boot smoke: mount + put/get. Retorna true se PASS.
pub fn smoke() -> bool {
    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    if kv.mount().is_err() {
        return false;
    }
    if kv.put("smoke", b"ok").is_err() {
        return false;
    }
    match kv.get("smoke") {
        Ok(v) => v.as_slice() == b"ok",
        Err(_) => false,
    }
}

/// Persist blob sob key (ex. vdb/blob).
pub fn put_blob(key: &str, data: &[u8]) -> Result<(), &'static str> {
    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    if !kv.is_ready() {
        kv.mount()?;
    }
    kv.put(key, data)
}

pub fn get_blob(key: &str) -> Result<Vec<u8>, &'static str> {
    let mut g = TICKV.lock();
    let kv = g.as_mut().ok_or("no tickv")?;
    if !kv.is_ready() {
        return Err("not mounted");
    }
    kv.get(key)
}

pub fn is_ready() -> bool {
    TICKV.lock().as_ref().map(|k| k.is_ready()).unwrap_or(false)
}

pub fn backend_name() -> &'static str {
    let g = TICKV.lock();
    match g.as_ref().map(|k| k.backend) {
        Some("nvme") => "nvme",
        Some("ram") => "ram",
        Some(_) => "unknown",
        None => "none",
    }
}

/// F8 lite: put → drop índice RAM → remount/recover → get deve sobreviver (CRC).
pub fn power_loss_smoke() -> bool {
    if put_blob("pl/test", b"survive").is_err() {
        return false;
    }
    // Simula crash: perde TickvLite em RAM; flash mantém append-log.
    *TICKV.lock() = None;
    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    if kv.mount().is_err() {
        return false;
    }
    match kv.get("pl/test") {
        Ok(v) => v.as_slice() == b"survive",
        Err(_) => false,
    }
}
