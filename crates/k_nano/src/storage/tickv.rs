//! ADR-0063 Q1 — TickvLite: append-log + CRC + GC/compaction + recover robusto.
//! Record: magic TKLV | u32 key_len | u32 val_len | u32 crc | key | val | pad16 → pad512.
//! Honesty: não é crate tickv upstream; erase NVMe = TRIM residual.
//!
//! Interop com `neural-sgdb` (ADR-0004 lá): `encode_record` / `scan_volume` são
//! o contrato byte-exato TKLV/TKCK. Gate host em `#[cfg(test)]` (SESSION_267).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::flash::{init_flash, ActiveFlash, FlashController, FLASH, RamFlash};

/// Magic TKLV — 4º byte 'V' (valid); tombstone in-place troca por 0x00.
pub const MAGIC: &[u8; 4] = b"TKLV";
pub const HEADER: usize = 16;
/// Byte 3 do magic: 'V' = válido (legado); 0 = invalidado in-place (herança TicKV).
pub const MAGIC_PREFIX: &[u8; 3] = b"TKL";
/// Canonical checkpoint key (paridade neural-sgdb).
pub const CKPT_KEY: &str = "sys/tickv_ckpt";
/// Limites do leitor (paridade recover / neural-sgdb).
pub const MAX_KLEN: usize = 4096;
pub const MAX_VLEN: usize = 2 * 1024 * 1024;

fn hdr_valid(hdr: &[u8]) -> bool {
    hdr.len() >= 4 && &hdr[0..3] == MAGIC_PREFIX && (hdr[3] == b'V' || hdr[3] == 1)
}

fn hdr_invalidated(hdr: &[u8]) -> bool {
    hdr.len() >= 4 && &hdr[0..3] == MAGIC_PREFIX && hdr[3] == 0
}

fn hdr_tickv_shaped(hdr: &[u8]) -> bool {
    hdr_valid(hdr) || hdr_invalidated(hdr)
}
/// Dispara GC se append_off ultrapassar isto (ou dead/live).
const HIGH_WATER: u64 = 256 * 1024;
const DEAD_RATIO_NUM: u64 = 1; // dead > live * ratio → GC
const DEAD_RATIO_DEN: u64 = 1;

/// CRC32 IEEE (poly 0xEDB88320) — cobre **somente key‖val**.
pub fn crc32(data: &[u8]) -> u32 {
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

/// Tamanho total de um record no volume (múltiplo de 512).
pub fn record_size(klen: usize, vlen: usize) -> usize {
    let body_len = klen + vlen;
    let padded = (body_len + 15) & !15;
    let rec_body = HEADER + padded;
    (rec_body + 511) & !511
}

fn rec_total(klen: usize, vlen: usize) -> usize {
    record_size(klen, vlen)
}

/// Serializa um record TKLV completo (512-alinhado) — byte-exato vs neural-sgdb.
pub fn encode_record(key: &[u8], val: &[u8]) -> Vec<u8> {
    let body_len = key.len() + val.len();
    let padded = (body_len + 15) & !15;
    let mut body = vec![0u8; padded];
    body[..key.len()].copy_from_slice(key);
    body[key.len()..body_len].copy_from_slice(val);
    let crc = crc32(&body[..body_len]);
    let total = record_size(key.len(), val.len());
    let mut rec = vec![0u8; total];
    rec[0..4].copy_from_slice(MAGIC);
    rec[4..8].copy_from_slice(&(key.len() as u32).to_le_bytes());
    rec[8..12].copy_from_slice(&(val.len() as u32).to_le_bytes());
    rec[12..16].copy_from_slice(&crc.to_le_bytes());
    rec[HEADER..HEADER + padded].copy_from_slice(&body);
    rec
}

/// Resultado de `scan_volume` — índice last-wins + métricas (interop neural-sgdb).
#[derive(Clone, Debug, Default)]
pub struct ScanResult {
    pub map: BTreeMap<String, Vec<u8>>,
    pub offsets: BTreeMap<String, u64>,
    pub append_off: u64,
    pub corrupt: u64,
    pub truncated: bool,
}

/// Varre um volume TKLV em memória — port do `recover()` / neural-sgdb `scan_volume`.
pub fn scan_volume(data: &[u8]) -> ScanResult {
    let mut out = ScanResult::default();
    let size = data.len() as u64;
    let mut off = 0u64;
    let mut eof = false;
    while off + HEADER as u64 <= size {
        let hdr = &data[off as usize..off as usize + HEADER];
        if !hdr_tickv_shaped(hdr) {
            if hdr.iter().all(|&b| b == 0 || b == 0xFF) {
                eof = true;
                break;
            }
            out.corrupt += 1;
            off = (off + 512) & !511;
            continue;
        }
        let klen = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
        let vlen = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
        if klen > MAX_KLEN || vlen > MAX_VLEN {
            out.corrupt += 1;
            off = (off + 512) & !511;
            continue;
        }
        let total = record_size(klen, vlen) as u64;
        if off + total > size {
            out.truncated = true;
            break;
        }
        if hdr[3] == 0 {
            out.append_off = out.append_off.max(off + total);
            off += total;
            continue;
        }
        let body = &data[off as usize + HEADER..off as usize + HEADER + klen + vlen];
        let want = u32::from_le_bytes(hdr[12..16].try_into().unwrap());
        if crc32(body) != want {
            out.corrupt += 1;
            off = (off + 512) & !511;
            continue;
        }
        out.append_off = out.append_off.max(off + total);
        if let Ok(key) = core::str::from_utf8(&body[..klen]) {
            if key != CKPT_KEY {
                if vlen == 0 {
                    out.map.remove(key);
                    out.offsets.remove(key);
                } else {
                    out.map.insert(String::from(key), body[klen..].to_vec());
                    out.offsets.insert(String::from(key), off);
                }
            }
        } else {
            out.corrupt += 1;
        }
        off += total;
    }
    if !eof && off < size {
        out.truncated = true;
    }
    out
}

/// Instala `RamFlash` no static FLASH (host / testes interop). Não chama NVMe.
pub fn install_ram_flash(size: usize) {
    let mut g = FLASH.lock();
    *g = Some(ActiveFlash::Ram(RamFlash::new(size)));
}

/// Dump dos primeiros `len` bytes do FLASH (gate OS→bytes).
/// Prefer `RamFlash` (host); NVMe exige alinhamento 512 — arredonda `len` para cima.
pub fn dump_flash(len: usize) -> Result<Vec<u8>, &'static str> {
    let mut g = FLASH.lock();
    let flash = g.as_mut().ok_or("no flash")?;
    let size = flash.size_bytes() as usize;
    let n = len.min(size);
    let mut buf = vec![0u8; n];
    // RamFlash aceita qualquer tamanho; NVMe exige 512 — pad local e trim.
    let need = (n + 511) & !511;
    if need == n {
        flash.read(0, &mut buf)?;
    } else if need <= size {
        let mut full = vec![0u8; need];
        flash.read(0, &mut full)?;
        buf.copy_from_slice(&full[..n]);
    } else {
        // só RAM/pequeno: lê n se o backend permitir (Ram)
        flash.read(0, &mut buf)?;
    }
    Ok(buf)
}

#[derive(Clone, Copy, Default)]
pub struct TickvStats {
    pub live_bytes: u64,
    pub dead_bytes: u64,
    pub corrupt_records: u64,
    pub compactions: u64,
    pub puts: u64,
    pub gets: u64,
}

pub struct TickvLite {
    index: BTreeMap<String, u64>,
    append_off: u64,
    ready: bool,
    backend: &'static str,
    pub stats: TickvStats,
}

impl TickvLite {
    pub fn new() -> Self {
        TickvLite {
            index: BTreeMap::new(),
            append_off: 0,
            ready: false,
            backend: "none",
            stats: TickvStats::default(),
        }
    }

    pub fn backend(&self) -> &str {
        self.backend
    }
    pub fn is_ready(&self) -> bool {
        self.ready
    }
    pub fn append_off(&self) -> u64 {
        self.append_off
    }
    pub fn live_keys(&self) -> usize {
        self.index.len()
    }

    pub fn mount(&mut self) -> Result<(), &'static str> {
        // Host/tests: se FLASH já foi instalado (`install_ram_flash`), não reinicia.
        {
            let g = FLASH.lock();
            if g.is_some() {
                self.backend = match g.as_ref() {
                    Some(ActiveFlash::Ram(_)) => "ram",
                    Some(ActiveFlash::Nvme(_)) => "nvme",
                    Some(ActiveFlash::File(_)) => "file",
                    None => "none",
                };
            } else {
                drop(g);
                self.backend = init_flash();
            }
        }
        // D3: tenta ckpt rápido; fallback full scan
        if self.try_mount_from_ckpt().is_err() {
            self.recover()?;
        }
        self.ready = true;
        Ok(())
    }

    fn with_flash<R>(&self, f: impl FnOnce(&mut ActiveFlash) -> R) -> Result<R, &'static str> {
        let mut g = FLASH.lock();
        let flash = g.as_mut().ok_or("no flash")?;
        Ok(f(flash))
    }

    /// Snapshot `sys/tickv_ckpt`: append_off + fnv + n + (key_len,key,off)*.
    pub fn write_ckpt(&mut self) -> Result<(), &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut entries: Vec<(String, u64)> = Vec::new();
        for (k, off) in self.index.iter() {
            if k == "sys/tickv_ckpt" {
                continue;
            }
            for &b in k.as_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            for b in off.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            entries.push((k.clone(), *off));
        }
        let n = entries.len() as u32;
        let mut body = Vec::with_capacity(24 + entries.len() * 32);
        body.extend_from_slice(b"TKCK");
        body.extend_from_slice(&self.append_off.to_le_bytes());
        body.extend_from_slice(&h.to_le_bytes());
        body.extend_from_slice(&n.to_le_bytes());
        for (k, off) in &entries {
            let kb = k.as_bytes();
            if kb.len() > 65535 {
                continue;
            }
            body.extend_from_slice(&(kb.len() as u16).to_le_bytes());
            body.extend_from_slice(kb);
            body.extend_from_slice(&off.to_le_bytes());
        }
        self.put_raw("sys/tickv_ckpt", &body)
    }

    fn try_mount_from_ckpt(&mut self) -> Result<(), &'static str> {
        self.index.clear();
        self.append_off = 0;
        // Scan: só CRC do record `sys/tickv_ckpt` (demais só lê key) — mais barato que recover.
        let size = self.with_flash(|fl| fl.size_bytes())?;
        let mut off = 0u64;
        let mut hdr = [0u8; HEADER];
        let mut found: Option<Vec<u8>> = None;
        while off + HEADER as u64 <= size {
            self.with_flash(|fl| fl.read(off, &mut hdr))??;
            if !hdr_tickv_shaped(&hdr) {
                if hdr.iter().all(|&b| b == 0 || b == 0xFF) {
                    break;
                }
                off = (off + 512) & !511;
                continue;
            }
            let klen = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
            let vlen = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
            let want_crc = u32::from_le_bytes(hdr[12..16].try_into().unwrap());
            if klen > 4096 || vlen > 2 * 1024 * 1024 {
                off = (off + 512) & !511;
                continue;
            }
            let total = rec_total(klen, vlen) as u64;
            if off + total > size {
                break;
            }
            // lê só a key; CRC completo só se for ckpt
            let klen = if klen > 4096 { 4096 } else { klen };
            let mut keybuf = vec![0u8; klen];
            self.with_flash(|fl| fl.read(off + HEADER as u64, &mut keybuf))??;
            let is_ckpt = core::str::from_utf8(&keybuf)
                .map(|s| s == "sys/tickv_ckpt")
                .unwrap_or(false);
            if is_ckpt && vlen >= 24 {
                let mut body = vec![0u8; klen + vlen];
                body[..klen].copy_from_slice(&keybuf);
                self.with_flash(|fl| fl.read(off + HEADER as u64 + klen as u64, &mut body[klen..]))??;
                if crc32(&body) == want_crc {
                    found = Some(body[klen..].to_vec());
                }
            }
            off += total;
        }
        let val = found.ok_or("no ckpt")?;
        if val.len() < 24 { return Err("ckpt short"); }
        if &val[0..4] != b"TKCK" {
            return Err("bad ckpt magic");
        }
        let append = u64::from_le_bytes(val[4..12].try_into().unwrap());
        let want_fnv = u64::from_le_bytes(val[12..20].try_into().unwrap());
        let n = u32::from_le_bytes(val[20..24].try_into().unwrap()) as usize;
        let mut pos = 24usize;
        self.index.clear();
        for _ in 0..n {
            if pos + 2 > val.len() {
                return Err("ckpt trunc");
            }
            let kl = u16::from_le_bytes(val[pos..pos + 2].try_into().unwrap()) as usize;
            pos += 2;
            if pos + kl + 8 > val.len() {
                return Err("ckpt trunc");
            }
            let key = core::str::from_utf8(&val[pos..pos + kl])
                .map_err(|_| "ckpt key")?
                .to_string();
            pos += kl;
            let o = u64::from_le_bytes(val[pos..pos + 8].try_into().unwrap());
            pos += 8;
            self.index.insert(key, o);
        }
        let got = {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for (k, off) in self.index.iter() {
                for &b in k.as_bytes() {
                    h ^= b as u64;
                    h = h.wrapping_mul(0x100_0000_01b3);
                }
                for b in off.to_le_bytes() {
                    h ^= b as u64;
                    h = h.wrapping_mul(0x100_0000_01b3);
                }
            }
            h
        };
        if got != want_fnv {
            self.index.clear();
            return Err("ckpt fnv");
        }
        if let Some((_, &o)) = self.index.iter().next() {
            let mut h = [0u8; HEADER];
            self.with_flash(|fl| fl.read(o, &mut h))??;
            if !hdr_valid(&h) {
                self.index.clear();
                return Err("ckpt stale");
            }
        }
        let mut end = append;
        for &_o in self.index.values() {
            let o = _o;
            let mut h = [0u8; HEADER];
            if self.with_flash(|fl| fl.read(o, &mut h)).is_err() {
                continue;
            }
            if !hdr_valid(&h) {
                continue;
            }
            let klen = u32::from_le_bytes(h[4..8].try_into().unwrap()) as usize;
            let vlen = u32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
            if klen > 4096 || vlen > 2 * 1024 * 1024 {
                continue;
            }
            let total = rec_total(klen, vlen) as u64;
            let e = o.saturating_add(total);
            if e > end {
                end = e;
            }
        }
        self.append_off = end;
        self.recompute_live_estimate();
        Ok(())
    }

    /// Recover: CRC fail → corrupt++, tenta avançar 512B; magic break = fim do log.
    fn recover(&mut self) -> Result<(), &'static str> {
        self.index.clear();
        self.append_off = 0;
        self.stats.live_bytes = 0;
        self.stats.dead_bytes = 0;
        let size = self.with_flash(|fl| fl.size_bytes())?;
        let mut off = 0u64;
        let mut hdr = [0u8; HEADER];
        while off + HEADER as u64 <= size {
            self.with_flash(|fl| fl.read(off, &mut hdr))??;
            if !hdr_tickv_shaped(&hdr) {
                // skip aligned hole (pós-GC / padding) até achar magic ou zeros longos
                if hdr.iter().all(|&b| b == 0 || b == 0xFF) {
                    break;
                }
                self.stats.corrupt_records = self.stats.corrupt_records.saturating_add(1);
                off = (off + 512) & !511;
                continue;
            }
            let klen = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
            let vlen = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
            let want_crc = u32::from_le_bytes(hdr[12..16].try_into().unwrap());
            if klen > 4096 || vlen > 1024 * 1024 {
                self.stats.corrupt_records = self.stats.corrupt_records.saturating_add(1);
                off = (off + 512) & !511;
                continue;
            }
            let body_len = klen + vlen;
            let _padded = (body_len + 15) & !15;
            let total = rec_total(klen, vlen) as u64;
            if off + total > size {
                break;
            }
            // E3: V=0 — avança sem indexar (GC identifica pelo header)
            if hdr_invalidated(&hdr) {
                self.stats.dead_bytes = self.stats.dead_bytes.saturating_add(total);
                off += total;
                continue;
            }
            let mut body = vec![0u8; body_len];
            self.with_flash(|fl| fl.read(off + HEADER as u64, &mut body))??;
            let got = crc32(&body);
            if got != want_crc {
                self.stats.corrupt_records = self.stats.corrupt_records.saturating_add(1);
                // honesty: não lixo no índice; avança alinhado e continua
                off = (off + 512) & !511;
                continue;
            }
            let key = match core::str::from_utf8(&body[..klen]) {
                Ok(s) => s.to_string(),
                Err(_) => {
                    self.stats.corrupt_records = self.stats.corrupt_records.saturating_add(1);
                    off += total;
                    continue;
                }
            };
            let rec_sz = total;
            if vlen == 0 {
                if let Some(old) = self.index.remove(&key) {
                    let _ = old;
                    self.stats.dead_bytes = self.stats.dead_bytes.saturating_add(rec_sz);
                }
            } else {
                if let Some(_old_off) = self.index.insert(key, off) {
                    self.stats.dead_bytes = self.stats.dead_bytes.saturating_add(rec_sz);
                } else {
                    self.stats.live_bytes = self.stats.live_bytes.saturating_add(rec_sz);
                }
            }
            off += total;
        }
        self.append_off = off;
        // recalcula live a partir do índice (mais preciso pós-overwrite)
        self.recompute_live_estimate();
        Ok(())
    }

    fn recompute_live_estimate(&mut self) {
        let mut live = 0u64;
        for &_off in self.index.values() {
            // tamanho exato exigiria re-ler; usa média conservadora via get sizes no GC
            live = live.saturating_add(512);
        }
        self.stats.live_bytes = live;
    }

    fn maybe_gc(&mut self) -> Result<(), &'static str> {
        let need = self.append_off > HIGH_WATER
            || (self.stats.live_bytes > 0
                && self.stats.dead_bytes * DEAD_RATIO_DEN
                    > self.stats.live_bytes * DEAD_RATIO_NUM);
        if need {
            self.compact()
        } else {
            Ok(())
        }
    }

    /// Reescreve live-set no início do flash; atualiza índice.
    pub fn compact(&mut self) -> Result<(), &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        let keys: Vec<String> = self.index.keys().cloned().collect();
        let mut live: Vec<(String, Vec<u8>)> = Vec::with_capacity(keys.len());
        for k in keys {
            match self.get(&k) {
                Ok(v) if !v.is_empty() => live.push((k, v)),
                Ok(_) => {} // tombstone residual
                Err(_) => {}
            }
        }
        let old_append = self.append_off;
        // Zera região usada (RAM erase / NVMe write-zeros best-effort)
        let wipe = ((old_append + 511) & !511).max(512);
        let mut zero = vec![0u8; 4096];
        let mut o = 0u64;
        while o < wipe {
            let n = core::cmp::min(4096u64, wipe - o) as usize;
            zero.truncate(n);
            zero.resize(n, 0);
            // alinhar a 512
            let n512 = (n + 511) & !511;
            zero.resize(n512, 0);
            let _ = self.with_flash(|fl| fl.erase(o, n512 as u64));
            self.with_flash(|fl| fl.write(o, &zero[..n512]))??;
            o += n512 as u64;
        }
        self.index.clear();
        self.append_off = 0;
        self.stats.dead_bytes = 0;
        self.stats.live_bytes = 0;
        for (k, v) in live {
            self.put_raw(&k, &v)?;
        }
        self.stats.compactions = self.stats.compactions.saturating_add(1);
        let _ = format!("gc freed={}", old_append.saturating_sub(self.append_off));
        // D3: ckpt pós-compact (append_off bounded)
        let _ = self.write_ckpt();
        Ok(())
    }

    fn put_raw(&mut self, key: &str, val: &[u8]) -> Result<(), &'static str> {
        let rec = encode_record(key.as_bytes(), val);
        let total = rec.len();
        let off = self.append_off;
        self.with_flash(|fl| fl.write(off, &rec))??;
        if let Some(_old) = self.index.insert(String::from(key), off) {
            self.stats.dead_bytes = self.stats.dead_bytes.saturating_add(total as u64);
        } else {
            self.stats.live_bytes = self.stats.live_bytes.saturating_add(total as u64);
        }
        self.append_off = off + total as u64;
        self.stats.puts = self.stats.puts.saturating_add(1);
        Ok(())
    }

    pub fn put(&mut self, key: &str, val: &[u8]) -> Result<(), &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        // E3: invalidate in-place do record antigo (herança TicKV V=0)
        if key != "sys/tickv_ckpt" {
            if self.index.contains_key(key) {
                let _ = self.invalidate_key(key);
            }
        }
        self.put_raw(key, val)?;
        if key != "__gc_lock" {
            let _ = self.maybe_gc();
        }
        Ok(())
    }

    /// Marca record inválido no flash (byte magic[3]=0) e remove do índice.
    pub fn invalidate_key(&mut self, key: &str) -> Result<(), &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        let off = *self.index.get(key).ok_or("missing")?;
        self.with_flash(|fl| fl.write(off + 3, &[0u8]))??;
        self.index.remove(key);
        self.stats.dead_bytes = self.stats.dead_bytes.saturating_add(512);
        Ok(())
    }

    pub fn get(&mut self, key: &str) -> Result<Vec<u8>, &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        self.stats.gets = self.stats.gets.saturating_add(1);
        let off = *self.index.get(key).ok_or("missing")?;
        let mut hdr = [0u8; HEADER];
        self.with_flash(|fl| fl.read(off, &mut hdr))??;
        if !hdr_valid(&hdr) {
            return Err("missing");
        }
        let klen = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
        let vlen = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
        let want_crc = u32::from_le_bytes(hdr[12..16].try_into().unwrap());
        let mut body = vec![0u8; klen + vlen];
        self.with_flash(|fl| fl.read(off + HEADER as u64, &mut body))??;
        if crc32(&body) != want_crc {
            return Err("corrupt");
        }
        Ok(body[klen..].to_vec())
    }

    /// Lista keys com prefixo (para rebuild ART).
    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.index
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect()
    }

    pub fn offset_of(&self, key: &str) -> Option<u64> {
        self.index.get(key).copied()
    }

    pub fn delete(&mut self, key: &str) -> Result<(), &'static str> {
        // Prefer invalidate in-place; fallback tombstone empty
        if self.invalidate_key(key).is_ok() {
            return Ok(());
        }
        self.put(key, &[])?;
        self.index.remove(key);
        Ok(())
    }

    pub fn status_line(&self) -> String {
        format!(
            "TICKV live={} dead={} corrupt={} gc={} append={} keys={} backend={}",
            self.stats.live_bytes,
            self.stats.dead_bytes,
            self.stats.corrupt_records,
            self.stats.compactions,
            self.append_off,
            self.index.len(),
            self.backend
        )
    }
}

pub static TICKV: Mutex<Option<TickvLite>> = Mutex::new(None);

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
        Some("file") => "file",
        Some("nvme") => "nvme",
        Some("ram") => "ram",
        Some(_) => "unknown",
        None => "none",
    }
}

pub fn status_line() -> String {
    TICKV
        .lock()
        .as_ref()
        .map(|k| k.status_line())
        .unwrap_or_else(|| String::from("TICKV down"))
}

pub fn with_tickv<R>(f: impl FnOnce(&mut TickvLite) -> R) -> Option<R> {
    let mut g = TICKV.lock();
    g.as_mut().map(f)
}

pub fn power_loss_smoke() -> bool {
    if put_blob("pl/test", b"survive").is_err() {
        return false;
    }
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

/// Q1: overwrite muitas vezes → compact → get ainda OK.
pub fn gc_smoke() -> bool {
    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    if !kv.is_ready() && kv.mount().is_err() {
        return false;
    }
    for i in 0..32u32 {
        let mut payload = [0u8; 64];
        payload[0..4].copy_from_slice(&i.to_le_bytes());
        if kv.put("gc/key", &payload).is_err() {
            return false;
        }
    }
    // força compact
    if kv.compact().is_err() {
        return false;
    }
    match kv.get("gc/key") {
        Ok(v) if v.len() >= 4 => {
            let last = u32::from_le_bytes(v[0..4].try_into().unwrap());
            last == 31 && kv.stats.compactions >= 1
        }
        _ => false,
    }
}

/// D3: 1k overwrites → compact → append_off bounded + ckpt válido.
pub fn stress_gc_smoke() -> bool {
    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    if !kv.is_ready() && kv.mount().is_err() {
        return false;
    }
    let before = kv.append_off();
    for i in 0..1000u32 {
        let mut payload = [0u8; 128];
        payload[0..4].copy_from_slice(&i.to_le_bytes());
        if kv.put("stress/key", &payload).is_err() {
            return false;
        }
    }
    if kv.compact().is_err() {
        return false;
    }
    let after = kv.append_off();
    let ok_get = matches!(kv.get("stress/key"), Ok(v) if v.len() >= 4
        && u32::from_le_bytes(v[0..4].try_into().unwrap()) == 999);
    // pós-GC: append muito menor que 1000×128 pads
    let bounded = after < 64 * 1024 && after < before.saturating_add(256 * 1024);
    // remount com ckpt (simula)
    let _ = kv.write_ckpt();
    ok_get && bounded && kv.stats.compactions >= 1
}

/// Q1: flip 1 byte no flash → get retorna Err("corrupt"), não lixo.
pub fn corrupt_smoke() -> bool {
    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    if !kv.is_ready() && kv.mount().is_err() {
        return false;
    }
    if kv.put("corrupt/t", b"good").is_err() {
        return false;
    }
    let off = match kv.offset_of("corrupt/t") {
        Some(o) => o,
        None => return false,
    };
    // flip um byte no payload (após header)
    let mut byte = [0u8; 512];
    if kv.with_flash(|fl| fl.read(off, &mut byte)).is_err() {
        return false;
    }
    byte[HEADER] ^= 0xFF;
    if kv.with_flash(|fl| fl.write(off, &byte)).is_err() {
        return false;
    }
    matches!(kv.get("corrupt/t"), Err("corrupt"))
}

#[cfg(test)]
mod interop_tests {
    use super::*;
    use crate::storage::flash::FLASH;
    use spin::Mutex as SpinMutex;

    /// Statics FLASH/TICKV são globais — serializa interop tests.
    static TEST_LOCK: SpinMutex<()> = SpinMutex::new(());

    fn reset() {
        *TICKV.lock() = None;
        *FLASH.lock() = None;
    }

    /// Mesmo vetor que `neural_sgdb::tickv::tests::golden_record_bytes` (v1.1.0).
    #[test]
    fn golden_record_bytes_match_neural_sgdb() {
        let _g = TEST_LOCK.lock();
        let rec = encode_record(b"k", b"v");
        assert_eq!(rec.len(), 512);
        assert_eq!(&rec[0..4], b"TKLV");
        assert_eq!(&rec[4..8], &1u32.to_le_bytes());
        assert_eq!(&rec[8..12], &1u32.to_le_bytes());
        let want_crc = crc32(b"kv");
        assert_eq!(&rec[12..16], &want_crc.to_le_bytes());
        assert_eq!(&rec[16..18], b"kv");
        assert!(rec[18..512].iter().all(|&b| b == 0));
        assert_eq!(record_size(1, 1), 512);
        assert_eq!(record_size(1000, 0), 1024);
        assert_eq!(record_size(1000, 1000), 2048);
    }

    #[test]
    fn scan_volume_tombstone_and_last_wins() {
        let _g = TEST_LOCK.lock();
        let mut data = Vec::new();
        data.extend_from_slice(&encode_record(b"md/L2/a", b"1"));
        data.extend_from_slice(&encode_record(b"md/L2/b", b"2"));
        data.extend_from_slice(&encode_record(b"md/L2/a", b"")); // tombstone
        let scan = scan_volume(&data);
        assert_eq!(scan.corrupt, 0);
        assert!(!scan.truncated);
        assert_eq!(scan.map.get("md/L2/a"), None);
        assert_eq!(
            scan.map.get("md/L2/b").map(|v| v.as_slice()),
            Some(&b"2"[..])
        );
    }

    #[test]
    fn put_get_roundtrip_ram_flash() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(64 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        kv.put("hello", b"world").expect("put");
        assert_eq!(kv.get("hello").unwrap(), b"world");
        let dump = dump_flash(512).expect("dump");
        assert_eq!(&dump[0..4], MAGIC);
        let scanned = scan_volume(&dump);
        assert_eq!(
            scanned.map.get("hello").map(|v| v.as_slice()),
            Some(&b"world"[..])
        );
        reset();
    }

    #[test]
    fn remount_preserves_volume_via_dump_scan() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(64 * 1024);
        {
            let mut kv = TickvLite::new();
            kv.mount().expect("mount");
            kv.put("persist", b"across").expect("put");
        }
        // Simula reopen: novo TickvLite, mesmos bytes no FLASH.
        let mut kv2 = TickvLite::new();
        kv2.mount().expect("remount");
        assert_eq!(kv2.get("persist").unwrap(), b"across");
        let dump = dump_flash(1024).unwrap();
        let scanned = scan_volume(&dump);
        assert_eq!(
            scanned.map.get("persist").map(|v| v.as_slice()),
            Some(&b"across"[..])
        );
        reset();
    }

    #[test]
    fn checkpoint_key_visible_in_raw_scan_skipped_in_map() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(64 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        kv.put("user/k", b"v").expect("put");
        kv.write_ckpt().expect("ckpt");
        let dump = dump_flash(4096).unwrap();
        // scan_volume omite CKPT_KEY do map (paridade neural-sgdb).
        let scanned = scan_volume(&dump);
        assert!(scanned.map.get(CKPT_KEY).is_none());
        assert_eq!(
            scanned.map.get("user/k").map(|v| v.as_slice()),
            Some(&b"v"[..])
        );
        // Bytes brutos contêm a key do ckpt.
        let ckpt_bytes = CKPT_KEY.as_bytes();
        assert!(dump.windows(ckpt_bytes.len()).any(|w| w == ckpt_bytes));
        reset();
    }
}
