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
use core::sync::atomic::{AtomicBool, Ordering};
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

/// Razões do flush oportunista (log honesto `tickv flush reason=..`).
/// - `idle`: scheduler sem trabalho / entre slices (sem timer dedicado).
/// - `high_water`: o append log cruzou [`HIGH_WATER`] ao fim de uma escrita.
pub const FLUSH_IDLE: &str = "idle";
pub const FLUSH_HIGH_WATER: &str = "high_water";

/// s410j: guard de recursão — `compact()` regrava via `put_batch_impl` que
/// chama `maybe_gc` no fim; se o live-set pós-compact ainda > HIGH_WATER
/// (volume de dados real maior que o gatilho), o maybe_gc dispararia compact
/// de novo → recursão infinita de GC. True = estamos DENTRO de um compact;
/// `maybe_gc` skipa.
static COMPACTING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// s410j: telemetria de recursão evitada (observável em testes/UI sem mudar
/// o contrato de erro de `compact()`). Incrementado a cada maybe_gc skipado
/// pelo guard; decrementado no fim do compact.
static COMPACT_SKIPPED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// s410j: total monotônico de skips (NUNCA drenado) — observável por testes
/// e UI; `COMPACT_SKIPPED` é a janela do compact corrente (drenada no log).
static COMPACT_SKIPPED_TOTAL: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Boot/FileFlash: `compact()` reescreve o log via ATA PIO (wipe+rewrite) —
/// minutos de silêncio se append_off ≫ HIGH_WATER (K33[28] soft-hang).
/// Suspenso do mount file/nvme até `set_gc_suspended(false)` no Runtime.
static GC_SUSPENDED: AtomicBool = AtomicBool::new(false);

/// Teto wall-clock p/ scan de mount (ckpt + recover). Sem TSC = sem teto (host tests).
const MOUNT_SCAN_BUDGET_US: u64 = 3_000_000;

fn mount_scan_deadline() -> u64 {
    let now = crate::tsc::now_us();
    if now == 0 {
        return u64::MAX; // TSC não calibrado — não aborta (testes host)
    }
    now.saturating_add(MOUNT_SCAN_BUDGET_US)
}

fn mount_scan_expired(deadline: u64) -> bool {
    #[cfg(test)]
    if FORCE_SCAN_TIMEOUT.load(Ordering::Relaxed) {
        return true; // host nao reproduz o PIO lento do backend=file
    }
    deadline != u64::MAX && crate::tsc::now_us() >= deadline
}

/// Sessao AION: hook de teste — o RamFlash host nao tem PIO lento, entao o
/// caminho de timeout do scan precisa ser forcado para o teste ser deterministico.
#[cfg(test)]
static FORCE_SCAN_TIMEOUT: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
fn force_scan_timeout(v: bool) {
    FORCE_SCAN_TIMEOUT.store(v, Ordering::Relaxed);
}

/// s410j: log honesto do guard de recursão — skipped>0 significa "live-set
/// real > HIGH_WATER, GC suprimido dentro do compact" (visível ao operador).
fn k_nano_slog_gc_skipped(old_append: u64) {
    let skipped = COMPACT_SKIPPED.swap(0, Ordering::Relaxed);
    if skipped > 0 {
        crate::slog_nano!(
            "TICKV",
            "warn",
            "compact batch: maybe_gc skipado {}x (live-set > HIGH_WATER, append_old={}) — GC suprimido p/ evitar recursão",
            skipped,
            old_append
        );
    }
}

/// Suspende / retoma `maybe_gc` (boot alive). Idempotente.
pub fn set_gc_suspended(suspended: bool) {
    GC_SUSPENDED.store(suspended, Ordering::Release);
    if suspended {
        crate::slog_nano!("TICKV", "warn", "GC compact SUSPENDED (boot/FileFlash ATA)");
    } else {
        crate::slog_nano!("TICKV", "ok", "GC compact RESUMED");
    }
}

pub fn gc_is_suspended() -> bool {
    GC_SUSPENDED.load(Ordering::Acquire)
}
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

/// Sessao AION: avanca `off` por um record de header TKL valido mas com
/// klen/vlen acima do teto (ex.: `sys/checkpoint` ~2MB > MAX_VLEN). Pula pelo
/// TOTAL (bounded por `size`) em vez de 512-a-512 — o passo de 512 caia no corpo
/// do record e fazia `break` no primeiro trecho zerado, perdendo todo o log
/// depois dele (bug F1.5). Se o total nao couber no volume, avanca 512 (corrupto).
fn advance_oversized(off: u64, klen: usize, vlen: usize, size: u64) -> u64 {
    let big = rec_total(klen, vlen) as u64;
    if big >= 512 && off.saturating_add(big) <= size {
        off + big
    } else {
        (off + 512) & !511
    }
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
    /// recover TIMEOUT / índice parcial — puts OK mas honesty: mount degradado.
    degraded: bool,
    backend: &'static str,
    /// Latched ao cruzar HIGH_WATER: impede re-disparo do flush oportunista a
    /// cada put enquanto o GC não baixar o append (ou o backend não permitir GC).
    hw_latched: bool,
    /// Sessao AION: o índice mudou desde o último ckpt. Dispara o ckpt no flush
    /// idle/high-water. Para backend=file/nvme o GC é proibido e o full-scan PIO
    /// estoura o budget do mount — o ckpt é o ÚNICO mount rápido desses backends.
    ckpt_dirty: bool,
    pub stats: TickvStats,
}

impl TickvLite {
    pub fn new() -> Self {
        TickvLite {
            index: BTreeMap::new(),
            append_off: 0,
            ready: false,
            degraded: false,
            backend: "none",
            hw_latched: false,
            ckpt_dirty: false,
            stats: TickvStats::default(),
        }
    }

    pub fn backend(&self) -> &str {
        self.backend
    }
    pub fn is_ready(&self) -> bool {
        self.ready
    }
    pub fn is_degraded(&self) -> bool {
        self.degraded
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
        // D3: tenta ckpt rápido; fallback full scan (um deadline TSC p/ os dois)
        let deadline = mount_scan_deadline();
        self.degraded = false;
        if self.try_mount_from_ckpt(deadline).is_err() {
            self.recover(deadline)?;
        }
        self.ready = true;
        if self.degraded {
            crate::slog_nano!(
                "TICKV",
                "warn",
                "mount READY but DEGRADED — índice parcial; compact/HITL remount recomendado"
            );
        }
        // File/NVMe: nunca compact automático no boot — wipe de MB via PIO
        // congela K33 (SESSION_354 deep). Runtime chama set_gc_suspended(false).
        if self.backend == "file" || self.backend == "nvme" {
            set_gc_suspended(true);
        }
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
        let r = self.put_raw("sys/tickv_ckpt", &body);
        if r.is_ok() {
            self.ckpt_dirty = false;
        }
        r
    }

    fn try_mount_from_ckpt(&mut self, deadline: u64) -> Result<(), &'static str> {
        self.index.clear();
        self.append_off = 0;
        // Scan: só CRC do record `sys/tickv_ckpt` (demais só lê key) — mais barato que recover.
        let size = self.with_flash(|fl| fl.size_bytes())?;
        let mut off = 0u64;
        let mut hdr = [0u8; HEADER];
        let mut found: Option<Vec<u8>> = None;
        while off + HEADER as u64 <= size {
            if mount_scan_expired(deadline) {
                crate::slog_nano!(
                    "TICKV",
                    "warn",
                    "ckpt scan TIMEOUT off={}/{} — fallback recover",
                    off,
                    size
                );
                return Err("ckpt scan timeout");
            }
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
            if klen > MAX_KLEN || vlen > MAX_VLEN {
                off = advance_oversized(off, klen, vlen, size);
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
        // Sessao AION: replay da cauda pos-ckpt (ver scan_range). O record de
        // ckpt ocupa [end, end+ckpt_total); a cauda comeca depois dele. Mesma
        // validacao do recover; `keep=true` anexa sem limpar o indice do ckpt.
        let ckpt_total = rec_total(CKPT_KEY.len(), val.len()) as u64;
        self.scan_range(end + ckpt_total, true, deadline)?;
        Ok(())
    }

    /// Recover: CRC fail → corrupt++, tenta avançar 512B; magic break = fim do log.
    /// Timeout TSC → mount degradado (índice parcial + append_off = off atual).
    fn recover(&mut self, deadline: u64) -> Result<(), &'static str> {
        self.scan_range(0, false, deadline)
    }

    /// Varre [start..size]. `keep=false` = recover (limpa índice/stats);
    /// `keep=true` = replay da cauda pós-ckpt (anexa, last-wins).
    ///
    /// Sessao AION: o ckpt so cobre [0, append) — o estado no instante do ckpt.
    /// Records appendados DEPOIS do ultimo ckpt (hard-kill antes do proximo
    /// flush) ficavam invisiveis, e `append_off < fim real` fazia a proxima
    /// escrita sobrescrever a cauda. Este range scan fecha o buraco.
    fn scan_range(&mut self, start: u64, keep: bool, deadline: u64) -> Result<(), &'static str> {
        if !keep {
            self.index.clear();
            self.append_off = 0;
            self.stats.live_bytes = 0;
            self.stats.dead_bytes = 0;
        }
        let size = self.with_flash(|fl| fl.size_bytes())?;
        let mut off = start;
        let mut hdr = [0u8; HEADER];
        while off + HEADER as u64 <= size {
            if mount_scan_expired(deadline) {
                self.degraded = true;
                crate::slog_nano!(
                    "TICKV",
                    "warn",
                    "recover TIMEOUT off={}/{} keys={} — mount DEGRADED (índice parcial)",
                    off,
                    size,
                    self.index.len()
                );
                break;
            }
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
            if klen > MAX_KLEN || vlen > MAX_VLEN {
                self.stats.corrupt_records = self.stats.corrupt_records.saturating_add(1);
                off = advance_oversized(off, klen, vlen, size);
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
        // Sessao AION: `off` é o fim REAL só se o scan completou. Em timeout o
        // índice é parcial e gravar em `off` sobrescreve a cauda não-varrida
        // (foi o que apagou o skill no boot 2). Fail-closed: aponta para o fim
        // do volume; put_raw passa a devolver Err("oob") em vez de clobber.
        self.append_off = if self.degraded { size } else { off };
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

    /// Política de GC: append passou o HIGH_WATER ou há mais dead que live.
    fn gc_due(&self) -> bool {
        self.append_off > HIGH_WATER
            || (self.stats.live_bytes > 0
                && self.stats.dead_bytes * DEAD_RATIO_DEN
                    > self.stats.live_bytes * DEAD_RATIO_NUM)
    }

    /// GC automático só é permitido onde ele é barato e seguro: RAM (e qualquer
    /// backend não-file/nvme). file/nvme = wipe+rewrite via PIO (hang TCG/HW) —
    /// compact só explícito (SleepCycle/HITL). GC_SUSPENDED suspende no boot.
    fn gc_allowed(&self) -> bool {
        !(self.backend == "file" || self.backend == "nvme")
            && !GC_SUSPENDED.load(Ordering::Acquire)
    }

    /// Flush oportunista (ADR-ora-2 item 4 adaptado): SEM timer dedicado.
    /// Chamado quando o scheduler está idle/entre slices (`reason=idle`) ou
    /// quando o append cruza o HIGH_WATER ao fim de uma escrita
    /// (`reason=high_water`). Compacta **somente se devido e permitido** —
    /// nunca no hot path de cada put. Loga `tickv flush reason=.. bytes=..`
    /// (ok) apenas quando um GC real correu.
    fn flush_opportunistic(&mut self, reason: &'static str) -> bool {
        if !self.ready || COMPACTING.load(Ordering::Acquire) {
            return false;
        }
        // Sessao AION: file/nvme proíbem GC (wipe+rewrite PIO), mas o ckpt é o
        // ÚNICO mount rápido desses backends — sem ele o boot paga um full-scan
        // PIO que estoura o budget e perde registros recentes. Desacopla o ckpt
        // do gc_allowed(); grava o snapshot do índice quando sujo.
        if self.backend == "file" || self.backend == "nvme" {
            if !self.ckpt_dirty {
                return false;
            }
            if self.write_ckpt().is_ok() {
                crate::slog_nano!(
                    "TICKV",
                    "ok",
                    "tickv flush reason={} bytes={} (ckpt)",
                    reason,
                    self.append_off
                );
                return true;
            }
            return false;
        }
        if !self.gc_due() || !self.gc_allowed() {
            return false;
        }
        let before = self.stats.compactions;
        let _ = self.maybe_gc();
        if self.stats.compactions == before {
            return false; // gated (file/nvme/suspend) — nada a logar
        }
        // Re-arma o latch conforme o append pós-GC (RAM baixa < HIGH_WATER).
        self.hw_latched = self.append_off >= HIGH_WATER;
        crate::slog_nano!(
            "TICKV",
            "ok",
            "tickv flush reason={} bytes={}",
            reason,
            self.append_off
        );
        true
    }

    /// Dispara o flush de HIGH_WATER no fim de put/put_batch (1× por travessia).
    fn maybe_high_water_flush(&mut self) {
        if self.append_off <= HIGH_WATER || self.hw_latched {
            return;
        }
        // Dentro de compact(): preserva a telemetria do guard anti-recursão.
        if COMPACTING.load(Ordering::Acquire) {
            let _ = self.maybe_gc();
            return;
        }
        if !self.gc_allowed() {
            // file/nvme/suspenso: não compacta no write path; latch p/ não
            // re-avaliar a cada put. O idle flush (RAM) continua disponível.
            self.hw_latched = true;
            return;
        }
        let _ = self.flush_opportunistic(FLUSH_HIGH_WATER);
    }

    fn maybe_gc(&mut self) -> Result<(), &'static str> {
        // s410j: dentro de um compact → nunca re-entrar (recursão infinita se
        // live-set real > HIGH_WATER; aí o volume SEMPRE vai re-disparar).
        // CHECK PRIMEIRO: independente de qualquer política externa
        // (GC_SUSPENDED é global e pode estar ativo durante o compact).
        if COMPACTING.load(Ordering::Acquire) {
            COMPACT_SKIPPED.fetch_add(1, Ordering::Relaxed);
            COMPACT_SKIPPED_TOTAL.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        if GC_SUSPENDED.load(Ordering::Acquire) {
            return Ok(());
        }
        // File/NVMe: compact = wipe+rewrite via PIO — nunca no hot path de put.
        // Compact explícito (`compact()` / SleepCycle) continua OK.
        if self.backend == "file" || self.backend == "nvme" {
            return Ok(());
        }
        if self.gc_due() {
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
        // s410j: regrava o live-set via BATCH (append contíguo, 1 GC-check no
        // fim) em vez de N× put_raw. O maybe_gc interno do batch é skipado
        // pelo guard COMPACTING — nunca re-entrar no GC dentro do GC.
        let refs: Vec<(&str, &[u8])> = live.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect();
        COMPACTING.store(true, Ordering::Release);
        let res = self.put_batch(&refs);
        // ckpt é gravado DENTRO do guard (sem ele, o put_raw do ckpt dispararia
        // maybe_gc com append_off do live-set grande → re-entrada).
        let ckpt_res = if res.is_ok() { self.write_ckpt().map(|_n| ()) } else { Ok(()) };
        COMPACTING.store(false, Ordering::Release);
        res?;
        ckpt_res?;
        // Honestidade s410j: o maybe_gc skipado pelo guard é contabilizado —
        // se o live-set real > HIGH_WATER, este número expõe que o GC está
        // sendo continuamente suprimido (sinal p/ HITL/re-particionar volume).
        k_nano_slog_gc_skipped(old_append);
        self.stats.compactions = self.stats.compactions.saturating_add(1);
        let _ = format!("gc freed={}", old_append.saturating_sub(self.append_off));
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
        if key != CKPT_KEY {
            self.ckpt_dirty = true;
        }
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
            self.maybe_high_water_flush();
        }
        Ok(())
    }

    /// Batch put (s410e): escreve N records com UMA aquisição de lock + GC
    /// adiado para o fim (instead of N× maybe_gc). Semântica idêntica ao
    /// `put` por item: invalidate in-place do antigo, append, índice.
    /// Falha é atômica por item (retorna a 1ª posição que falhou).
    /// s410j: também é o path de regravação do live-set no `compact()` —
    /// o maybe_gc interno é skipado pelo guard COMPACTING (sem recursão).
    pub fn put_batch(&mut self, items: &[(&str, &[u8])]) -> Result<(), &'static str> {
        if !self.ready {
            return Err("not mounted");
        }
        // 1) invalida todos os antigos ANTES de escrever (fail antes de tocar flash)
        for (key, _) in items {
            if *key != "sys/tickv_ckpt" && self.index.contains_key(*key) {
                let _ = self.invalidate_key(key);
            }
        }
        // 2) append contíguo
        for (key, val) in items {
            self.put_raw(key, val)?;
        }
        // 3) flush de HIGH_WATER uma única vez no fim (hot path de N puts paga
        // 1 verificação); o GC acontece fora do put individual (ponto certo).
        if items.iter().any(|(k, _)| *k != "__gc_lock") {
            self.maybe_high_water_flush();
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
            "TICKV live={} dead={} corrupt={} gc={} append={} keys={} backend={} deg={}",
            self.stats.live_bytes,
            self.stats.dead_bytes,
            self.stats.corrupt_records,
            self.stats.compactions,
            self.append_off,
            self.index.len(),
            self.backend,
            if self.degraded { 1 } else { 0 }
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
    if kv.is_degraded() {
        static WARNED: core::sync::atomic::AtomicBool =
            core::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, core::sync::atomic::Ordering::Relaxed) {
            crate::slog_nano!(
                "TICKV",
                "warn",
                "put while DEGRADED (recover timeout) — key example={}",
                key
            );
        }
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

/// True se recover timeoutou / índice parcial (ready mas honesty DEGRADED).
pub fn is_degraded() -> bool {
    TICKV.lock().as_ref().map(|k| k.is_degraded()).unwrap_or(false)
}

/// Flush oportunista global, SEM timer dedicado. O scheduler chama isto
/// quando está idle/entre slices (`FLUSH_IDLE`); o write path chama ao cruzar
/// o HIGH_WATER (`FLUSH_HIGH_WATER`). Compacta só quando devido e permitido;
/// loga `tickv flush reason=.. bytes=..` quando um GC real correu.
pub fn flush_opportunistic(reason: &'static str) -> bool {
    let mut g = TICKV.lock();
    g.as_mut()
        .map(|kv| kv.flush_opportunistic(reason))
        .unwrap_or(false)
}

/// Atalho canônico do idle hook (scheduler sem trabalho).
pub fn flush_idle() -> bool {
    flush_opportunistic(FLUSH_IDLE)
}

/// Após MSC: promove FileFlash, **migra** chaves RAM → stick, remonta TickvLite.
pub fn remount_after_usb_msc() -> bool {
    // Snapshot KV enquanto backend ainda é RAM (antes de trocar FLASH).
    let mut pending: alloc::vec::Vec<(alloc::string::String, alloc::vec::Vec<u8>)> =
        alloc::vec::Vec::new();
    {
        let mut g = TICKV.lock();
        if let Some(kv) = g.as_mut() {
            if kv.backend == "ram" && kv.ready {
                let keys: alloc::vec::Vec<alloc::string::String> =
                    kv.index.keys().cloned().collect();
                for k in keys {
                    if let Ok(v) = kv.get(&k) {
                        pending.push((k, v));
                    }
                }
                crate::slog_nano!(
                    "TICKV",
                    "ok",
                    "migrate snapshot ram_keys={}",
                    pending.len()
                );
            } else if kv.backend == "file" {
                return true;
            }
        }
    }

    if !crate::storage::flash::try_promote_usb_file_flash() {
        return false;
    }

    let mut g = TICKV.lock();
    let kv = g.get_or_insert_with(TickvLite::new);
    kv.ready = false;
    kv.index.clear();
    kv.append_off = 0;
    if let Err(e) = kv.mount() {
        crate::slog_nano!("TICKV", "warn", "remount USB FAIL {}", e);
        return false;
    }

    let mut migrated = 0usize;
    for (k, v) in pending.iter() {
        // Não sobrescrever chave já presente no NSGDB do stick (ckpt prior).
        if kv.get(k).is_ok() {
            continue;
        }
        if kv.put(k, v).is_ok() {
            migrated += 1;
        }
    }
    if migrated > 0 {
        let _ = kv.write_ckpt();
    }
    crate::slog_nano!(
        "TICKV",
        "ok",
        "remount USB FileFlash backend={} keys={} migrated={}",
        kv.backend(),
        kv.live_keys(),
        migrated
    );
    true
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

    /// Sessao AION: um ckpt NAO cobre o volume inteiro — so [0, append). Um
    /// record appendado depois do ultimo ckpt e antes de um hard-kill (sem novo
    /// ckpt) tem de sobreviver: o mount por ckpt precisa replayar a cauda.
    /// Falsifica o bug do F1.5 (skill/wasm escrito apos o ckpt sumia no boot 2).
    #[test]
    fn ckpt_mount_replays_post_ckpt_records_after_hard_kill() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(256 * 1024);
        {
            let mut kv = TickvLite::new();
            kv.mount().expect("mount1");
            kv.put("before/ckpt", b"old").expect("put pre");
            kv.write_ckpt().expect("ckpt");
            kv.put("skill/wasm/oracle_rt_expr_v1", b"WASM-BYTES")
                .expect("put post");
            // hard kill: drop SEM outro write_ckpt
        }
        // reabre sobre o MESMO flash (sem reset/reinstall)
        let mut kv2 = TickvLite::new();
        kv2.mount().expect("mount2");
        assert_eq!(kv2.get("before/ckpt").unwrap(), b"old");
        assert_eq!(
            kv2.get("skill/wasm/oracle_rt_expr_v1").unwrap(),
            b"WASM-BYTES",
            "record pos-ckpt tem de sobreviver ao hard-kill"
        );
        // e o proximo write nao pode sobrescrever a cauda (append_off correto)
        kv2.put("after/reopen", b"x").expect("put pos-reopen");
        assert_eq!(
            kv2.get("skill/wasm/oracle_rt_expr_v1").unwrap(),
            b"WASM-BYTES",
            "append_off tem de apontar depois da cauda replayada"
        );
        reset();
    }

    /// (A) timeout do scan NÃO pode subestimar append_off nem sobrescrever a cauda.
    #[test]
    fn scan_timeout_is_fail_closed_and_never_clobbers_tail() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(64 * 1024);
        {
            let mut kv = TickvLite::new();
            kv.mount().expect("m1");
            kv.put("a", b"1").expect("put a");
            kv.put("b", b"2").expect("put b");
        }
        force_scan_timeout(true);
        let mut kv2 = TickvLite::new();
        kv2.mount().expect("m2");
        force_scan_timeout(false);
        assert_eq!(kv2.append_off(), 64 * 1024, "timeout deve apontar p/ o fim");
        assert!(kv2.put("c", b"3").is_err(), "put pos-timeout = fail-closed");
        drop(kv2);
        let mut kv3 = TickvLite::new();
        kv3.mount().expect("m3");
        assert_eq!(kv3.get("a").unwrap(), b"1");
        assert_eq!(kv3.get("b").unwrap(), b"2");
        reset();
    }

    /// (B) file backend grava ckpt no flush; reopen reindexa o pos-ckpt (tail scan).
    #[test]
    fn file_backend_flush_writes_ckpt_and_reopen_recalls_post_ckpt() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(256 * 1024);
        {
            let mut kv = TickvLite::new();
            kv.mount().expect("m1");
            kv.backend = "file";
            kv.put("before", b"1").expect("put");
            assert!(kv.flush_opportunistic(FLUSH_IDLE), "file deve gravar ckpt");
            kv.put("skill/wasm/oracle_rt_expr_v1", b"WASM")
                .expect("put skill");
        }
        let mut kv2 = TickvLite::new();
        kv2.mount().expect("m2");
        assert_eq!(kv2.get("before").unwrap(), b"1");
        assert_eq!(kv2.get("skill/wasm/oracle_rt_expr_v1").unwrap(), b"WASM");
        reset();
    }

    /// Sessao AION: um record shaped oversized (vlen > MAX_VLEN, ex. sys/checkpoint
    /// ~2MB) NAO pode interromper o scan — os records depois dele devem ser
    /// indexados (bug F1.5: o scan avancava 512-a-512 pelo corpo e fazia break nos
    /// zeros do meio, perdendo o skill e o ckpt do fim).
    #[test]
    fn mount_scans_past_oversized_record() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(8 * 1024 * 1024);
        {
            let mut kv = TickvLite::new();
            kv.mount().expect("m1");
            let big = vec![0u8; MAX_VLEN + 4096];
            kv.put("sys/checkpoint", &big).expect("put big");
            kv.put("after/big", b"ok").expect("put after");
        }
        let mut kv2 = TickvLite::new();
        kv2.mount().expect("m2");
        assert_eq!(
            kv2.get("after/big").unwrap(),
            b"ok",
            "record apos um oversized deve ser indexado"
        );
        reset();
    }

    /// s410e: put_batch — roundtrip N items com 1 GC-check; overwrite no
    /// batch invalida o antigo (mesma semântica do put individual).
    #[test]
    fn put_batch_roundtrip_and_overwrite() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(64 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        kv.put("pre/a", b"old").expect("put");
        let items = [
            ("md/L0/x", b"1".as_slice()),
            ("md/L1/y", b"22".as_slice()),
            ("pre/a", b"new".as_slice()), // overwrite no meio do batch
        ];
        kv.put_batch(&items).expect("batch");
        assert_eq!(kv.get("md/L0/x").unwrap(), b"1");
        assert_eq!(kv.get("md/L1/y").unwrap(), b"22");
        assert_eq!(kv.get("pre/a").unwrap(), b"new"); // last-wins
        // Volume continua escaneável (CRC/records íntegros).
        let dump = dump_flash(64 * 1024).expect("dump");
        let scanned = scan_volume(&dump);
        assert_eq!(scanned.corrupt, 0);
        assert_eq!(scanned.map.get("pre/a").map(|v| v.as_slice()), Some(&b"new"[..]));
        reset();
    }

    /// s410e: put_batch vazio = no-op (não toca flash nem GC).
    #[test]
    fn put_batch_empty_is_noop() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(16 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        let before = kv.append_off;
        kv.put_batch(&[]).expect("empty batch");
        assert_eq!(kv.append_off(), before);
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

    /// s410j: compact regrava o live-set via put_batch — volume pós-compact
    /// é contíguo (sem buracos de invalidação), escaneável e com append_off
    /// igual ao fim do último record (bounds exato).
    #[test]
    fn compact_rewrites_liveset_via_batch_contiguous() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(256 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        // Gera fragmentação: 32 keys com 4 overwrites cada → 96 records mortos.
        for i in 0..32u32 {
            for gen in 0..4u32 {
                let key = alloc_format_key(i);
                let val = alloc_format_val(gen);
                kv.put(&key, &val).expect("put");
            }
        }
        let dead_before = kv.stats.dead_bytes;
        assert!(dead_before > 0);
        kv.compact().expect("compact");
        // Live-set intacto: última geração de cada key.
        for i in 0..32u32 {
            let key = alloc_format_key(i);
            let got = kv.get(&key).expect("get pós-compact");
            assert_eq!(&got[..4], &3u32.to_le_bytes(), "gen errada pós-compact");
        }
        // Volume contíguo: zero dead bytes; append_off = fim exato do live-set
        // (+ ckpt escrito no fim); scan limpo.
        assert_eq!(kv.stats.dead_bytes, 0);
        assert!(kv.append_off() > 0);
        assert!(kv.append_off() < 256 * 1024);
        let dump = dump_flash(256 * 1024).expect("dump");
        let scanned = scan_volume(&dump);
        assert_eq!(scanned.corrupt, 0);
        assert_eq!(scanned.map.len(), 32);
        for (k, v) in scanned.map {
            if k.starts_with("batch/compact/") {
                assert_eq!(&v[..4], &3u32.to_le_bytes());
            }
        }
        assert_eq!(kv.stats.compactions, 1);
        reset();
    }

    /// s410j: guard anti-recursão — live-set > HIGH_WATER (=256KB) no flash
    /// de 1MB: pós-compact o append_off AINDA dispara o gatilho do maybe_gc;
    /// sem o guard, compact chamaria put_batch→maybe_gc→compact em loop
    /// infinito (stack overflow). Com o guard: 1 compact e volta.
    #[test]
    fn compact_batch_guard_no_gc_recursion_with_big_liveset() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(1024 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        // Live-set ~512KB (2× HIGH_WATER) com overhead de records de 512B.
        // População via put_batch: o maybe_gc FINAL dele cruza o HIGH_WATER e
        // dispara compact → que regrava via put_batch → maybe_gc → SEM o guard
        // seria recursão infinita (live-set real > gatilho SEMPRE re-dispara).
        let val = [0xABu8; 448]; // 16 hdr + 16+448 body → record 512B
        let mut items: Vec<(String, Vec<u8>)> = Vec::with_capacity(1000);
        for i in 0..1000u32 {
            let mut key = String::from("batch/big/");
            push_u32_hex(&mut key, i);
            items.push((key, val.to_vec()));
        }
        let refs: Vec<(&str, &[u8])> =
            items.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect();
        kv.put_batch(&refs).expect("populate");
        assert!(kv.append_off() > HIGH_WATER, "live-set real > HIGH_WATER");
        assert_eq!(compact_skipped_count(), 1, "guard devia skipar 1 maybe_gc");
        assert_eq!(kv.stats.compactions, 1);
        assert_eq!(kv.stats.dead_bytes, 0);
        // Amostra de dados intactos.
        let mut probe = String::from("batch/big/");
        push_u32_hex(&mut probe, 999);
        assert_eq!(kv.get(&probe).unwrap(), &val[..]);
        reset();
    }

    /// ora-2 item 4 (adaptado): puts INDIVIDUAIS NÃO compactam a cada put — o
    /// GC só dispara UMA vez quando o append cruza o HIGH_WATER (ponto certo).
    /// Antes deste fix o put individual disparava compact completo ~N/2 vezes
    /// (O(n²)); agora a travessia é latched e a correção se mantém.
    #[test]
    fn put_individual_flushes_once_at_high_water_not_per_put() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(1024 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        let val = [0xCDu8; 448];
        for i in 0..520u32 {
            let mut key = String::from("solo/big/");
            push_u32_hex(&mut key, i);
            kv.put(&key, &val).expect("put");
        }
        // 520 records de 512B: cruza o HIGH_WATER 1× → 1 compact, não O(n²).
        assert!(
            kv.stats.compactions >= 1 && kv.stats.compactions <= 2,
            "compactions={} devia ser 1 (não ~N/2)",
            kv.stats.compactions
        );
        let mut probe = String::from("solo/big/");
        push_u32_hex(&mut probe, 519);
        assert_eq!(kv.get(&probe).unwrap(), &val[..]);
        reset();
    }

    /// ora-2 item 4 (adaptado): o flush IDLE (scheduler sem trabalho) compacta
    /// quando devido, sem timer dedicado. Reproduz o caso real: GC suspenso no
    /// boot (file/nvme) faz o write path latched e sem compact; ao retomar, o
    /// idle flush encontra o volume fragmentado e compacta — log `reason=idle`.
    #[test]
    fn flush_idle_compacts_when_due_after_resume() {
        let _g = TEST_LOCK.lock();
        reset();
        install_ram_flash(1024 * 1024);
        let mut kv = TickvLite::new();
        kv.mount().expect("mount");
        // Benigno: abaixo do HIGH_WATER e sem dead → nada a fazer.
        kv.put("k", b"v").expect("put");
        assert!(!kv.flush_opportunistic(FLUSH_IDLE));
        // GC suspenso: puts cruzam o HIGH_WATER mas não compactam.
        set_gc_suspended(true);
        let val = [0xEEu8; 448];
        for i in 0..600u32 {
            let mut key = String::from("flush/big/");
            push_u32_hex(&mut key, i);
            kv.put(&key, &val).expect("put");
        }
        assert_eq!(kv.stats.compactions, 0, "GC suspenso não compacta");
        // Retoma: o idle flush encontra o append acima do HIGH_WATER e compacta.
        set_gc_suspended(false);
        let before = kv.stats.compactions;
        assert!(
            kv.flush_opportunistic(FLUSH_IDLE),
            "flush idle devia compactar"
        );
        assert!(kv.stats.compactions > before);
        let mut probe = String::from("flush/big/");
        push_u32_hex(&mut probe, 599);
        assert_eq!(kv.get(&probe).unwrap(), &val[..]);
        reset();
    }

    /// Lê o total monotônico de maybe_gc skipados pelo guard COMPACTING.
    fn compact_skipped_count() -> u64 {
        super::COMPACT_SKIPPED_TOTAL.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Helper: key estável `batch/compact/NNNNNN` (zero-padded).
    fn alloc_format_key(i: u32) -> String {
        let mut s = String::from("batch/compact/");
        push_u32_hex(&mut s, i);
        s
    }

    /// Helper: valor de 8B com a geração no prefixo.
    fn alloc_format_val(gen: u32) -> [u8; 8] {
        let mut v = [0u8; 8];
        v[0..4].copy_from_slice(&gen.to_le_bytes());
        v[4..8].copy_from_slice(&i_dot_val(gen));
        v
    }

    fn i_dot_val(gen: u32) -> [u8; 4] {
        [gen as u8, 0, 0, (gen.wrapping_mul(7) & 0xFF) as u8]
    }

    /// Push u32 como 8 hex chars ASCII (zero-padded, sem alloc de format!).
    fn push_u32_hex(s: &mut String, v: u32) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut buf = [0u8; 8];
        let mut x = v;
        for j in (0..8).rev() {
            buf[j] = HEX[(x & 0xF) as usize];
            x >>= 4;
        }
        s.push_str(core::str::from_utf8(&buf).unwrap());
    }
}
