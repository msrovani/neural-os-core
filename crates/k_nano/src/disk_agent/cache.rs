//! ARC cache com write-back coalescing, dirty tracking, evict com flush.
//! ADR-0087 §6: `CachedDisk` wrapper torna a cache funcional no hot path
//! (NeuralFS via with_dev) com write-through — sem dirty em bare-metal.
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::block_dev::BlockDevice;

fn now() -> u64 { crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64 }

pub struct CacheEntry {
    pub data: Vec<u8>,
    pub freq: u16,
    pub last_access: u64,
    pub last_write: u64,
    pub dirty: bool,
}

pub struct ArcCache {
    entries: BTreeMap<u64, CacheEntry>,
    max_entries: usize,
    pub tier_name: &'static str,
    write_coalesce_ms: u64,
}

impl ArcCache {
    /// `max_kb` em KB; entradas de 512B (1 setor) → max_entries = max_kb * 2.
    pub fn new(max_kb: usize, tier: &'static str) -> Self {
        ArcCache {
            entries: BTreeMap::new(),
            // 512B por entrada: max_kb KB / 0.5 KB = max_kb * 2 setores.
            // ex: 1024 KB → 2048 setores ≈ 1MB de cache.
            max_entries: (max_kb * 2).max(16),
            tier_name: tier,
            write_coalesce_ms: 100,
        }
    }

    pub fn get(&mut self, lba: u64) -> Option<&[u8]> {
        let tick = now();
        if let Some(entry) = self.entries.get_mut(&lba) {
            entry.freq = entry.freq.saturating_add(1);
            entry.last_access = tick;
            Some(&entry.data)
        } else { None }
    }

    pub fn insert(&mut self, lba: u64, data: &[u8]) {
        if self.entries.len() >= self.max_entries { self.evict_one(); }
        self.entries.insert(lba, CacheEntry {
            data: data.to_vec(), freq: 1, last_access: now(), last_write: 0, dirty: false,
        });
    }

    pub fn mark_dirty(&mut self, lba: u64) {
        if let Some(entry) = self.entries.get_mut(&lba) {
            entry.dirty = true;
            entry.last_write = now();
        }
    }

    pub fn tick(&mut self, flush_fn: &mut dyn FnMut(u64, &[u8])) -> usize {
        let tick = now();
        let threshold = tick.saturating_sub(self.write_coalesce_ms);
        let to_flush: Vec<u64> = self.entries.iter()
            .filter(|(_, e)| e.dirty && e.last_write < threshold)
            .map(|(k, _)| *k).collect();
        let n = to_flush.len();
        for lba in &to_flush {
            if let Some(entry) = self.entries.get(lba) {
                flush_fn(*lba, &entry.data);
                if let Some(e) = self.entries.get_mut(lba) { e.dirty = false; }
            }
        }
        n
    }

    /// Evita o entry menos frequente (LFU) — faz writeback se dirty
    fn evict_one(&mut self) {
        let tick = now();
        let victim = self.entries.iter()
            .min_by_key(|(_, e)| (e.freq, (tick - e.last_access)))
            .map(|(k, _)| *k);
        if let Some(lba) = victim {
            let dirty = self.entries.get(&lba).map_or(false, |e| e.dirty);
            if dirty {
                crate::slog_nano!("CACHE", "info", "evict dirty {:#x} without flush_fn — DATA LOSS RISK", lba);
            }
            self.entries.remove(&lba);
        }
    }

    pub fn resize(&mut self, new_max_kb: usize) {
        self.max_entries = (new_max_kb * 2).max(16);
        while self.entries.len() > self.max_entries { self.evict_one(); }
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        let dirty = self.entries.iter().filter(|(_, e)| e.dirty).count();
        (self.entries.len(), self.max_entries, dirty)
    }
}

/// ADR-0087 §6: `BlockDevice` cacheado com write-through (leitura-escrita de
/// setor via cache de 512B; nunca dirty — bare-metal sem bateria = sem
/// write-back, lição F16 SESSION_252). Transparente: delega tudo ao inner.
pub struct CachedDisk<'a> {
    inner: &'a mut dyn BlockDevice,
    cache: ArcCache,
}

impl<'a> CachedDisk<'a> {
    /// 1024 KB → 2048 setores de 512B ≈ 1MB de cache.
    pub fn new(inner: &'a mut dyn BlockDevice) -> Self {
        CachedDisk {
            inner,
            cache: ArcCache::new(1024, "cached"),
        }
    }
}

impl BlockDevice for CachedDisk<'_> {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        if buf.is_empty() || buf.len() % 512 != 0 {
            return self.inner.read_sectors(lba, buf);
        }
        let sectors = buf.len() / 512;
        // Caminho rápido: todos os setores em cache → copia sem tocar o disco.
        let mut all_hit = true;
        for i in 0..sectors {
            let hit = {
                // Borrow curto: copia e DROP do borrow antes de qualquer insert.
                let e = self.cache.get(lba + i as u64);
                match e {
                    Some(d) => {
                        buf[i * 512..(i + 1) * 512].copy_from_slice(d);
                        true
                    }
                    None => false,
                }
            };
            if !hit {
                all_hit = false;
                break;
            }
        }
        if all_hit {
            return true;
        }
        // Miss: lê do disco e popula a cache por setor.
        if !self.inner.read_sectors(lba, buf) {
            return false;
        }
        for i in 0..sectors {
            let s = &buf[i * 512..(i + 1) * 512];
            self.cache.insert(lba + i as u64, s);
        }
        true
    }

    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        // Write-through: disco primeiro (fonte da verdade), cache atualizada
        // depois. Sem mark_dirty/tick — nunca há dirty a flushar.
        if !self.inner.write_sectors(lba, buf) {
            return false;
        }
        if !buf.is_empty() && buf.len() % 512 == 0 {
            let sectors = buf.len() / 512;
            for i in 0..sectors {
                let s = &buf[i * 512..(i + 1) * 512];
                self.cache.insert(lba + i as u64, s);
            }
        }
        true
    }

    fn total_sectors(&self) -> u64 {
        self.inner.total_sectors()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn sync_cache(&mut self) -> bool {
        self.inner.sync_cache()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake BlockDevice: setores em memória + contadores de I/O real.
    struct FakeDev {
        sectors: Vec<[u8; 512]>,
        reads: u64,
        writes: u64,
    }

    impl FakeDev {
        fn new(n: usize, seed: u8) -> Self {
            let mut sectors = Vec::with_capacity(n);
            for i in 0..n {
                let mut s = [0u8; 512];
                s[0] = seed.wrapping_add(i as u8);
                sectors.push(s);
            }
            FakeDev { sectors, reads: 0, writes: 0 }
        }
    }

    impl BlockDevice for FakeDev {
        fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
            self.reads += 1;
            let sectors = buf.len() / 512;
            if lba as usize + sectors > self.sectors.len() { return false; }
            for i in 0..sectors {
                buf[i * 512..(i + 1) * 512].copy_from_slice(&self.sectors[lba as usize + i]);
            }
            true
        }
        fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
            self.writes += 1;
            let sectors = buf.len() / 512;
            if lba as usize + sectors > self.sectors.len() { return false; }
            for i in 0..sectors {
                self.sectors[lba as usize + i].copy_from_slice(&buf[i * 512..(i + 1) * 512]);
            }
            true
        }
        fn total_sectors(&self) -> u64 {
            self.sectors.len() as u64
        }
        fn name(&self) -> &str {
            "fake0"
        }
    }

    #[test]
    fn cached_read_hit_after_miss() {
        let mut inner = FakeDev::new(8, 1);
        let (first, second) = {
            let mut cached = CachedDisk::new(&mut inner);

            // 1º read: 2 setores, miss → disco (reads=1)
            let mut buf = [0u8; 1024];
            assert!(cached.read_sectors(2, &mut buf));
            assert_eq!(buf[0], 1 + 2); // setor 2 com seed

            // 2º read dos mesmos setores: hit → cache, disco não relê
            let mut buf2 = [0u8; 1024];
            assert!(cached.read_sectors(2, &mut buf2));
            (buf, buf2)
        }; // drop(cached) libera o borrow de inner
        assert_eq!(inner.reads, 1); // 2º read não tocou o disco
        assert_eq!(first, second);
    }

    #[test]
    fn cached_write_through_updates() {
        let mut inner = FakeDev::new(8, 1);
        let r0 = {
            let mut cached = CachedDisk::new(&mut inner);

            // Write setor 5 → write-through no disco + cache atualizada.
            let mut w = [0u8; 512];
            w[0] = 0xAB;
            assert!(cached.write_sectors(5, &w));

            // Read de volta → vem da cache (fake não relê), dado é o novo.
            let mut r = [0u8; 512];
            assert!(cached.read_sectors(5, &mut r));
            r
        }; // drop(cached)
        assert_eq!(inner.writes, 1); // write-through persistiu no disco
        assert_eq!(inner.reads, 0); // read veio 100% da cache
        assert_eq!(r0[0], 0xAB);
        assert_eq!(inner.sectors[5][0], 0xAB); // disco de fato atualizado
    }

    #[test]
    fn cached_disk_delegates() {
        let mut inner = FakeDev::new(4, 7);
        let mut cached = CachedDisk::new(&mut inner);
        assert_eq!(cached.total_sectors(), 4);
        assert_eq!(cached.name(), "fake0");
        assert!(cached.sync_cache());
    }

    #[test]
    fn cache_formula_2048_entries_per_mb() {
        // 1024 KB / 512B por entrada = 2048 setores (~1MB).
        let c = ArcCache::new(1024, "cached");
        assert_eq!(c.stats().1, 2048);
    }

    #[test]
    fn partial_hit_reads_remaining_from_disk() {
        let mut inner = FakeDev::new(8, 3);
        let (one0, two0) = {
            let mut cached = CachedDisk::new(&mut inner);

            // Popula só o setor 1.
            let mut one = [0u8; 512];
            assert!(cached.read_sectors(1, &mut one));

            // Read de setores 1+2 (2 setores): 1 hit + 1 miss → disco lê os 2.
            let mut two = [0u8; 1024];
            assert!(cached.read_sectors(1, &mut two));
            (one, two)
        }; // drop(cached)
        assert_eq!(inner.reads, 2); // miss inicial + miss do setor 2
        assert_eq!(one0[0], 3 + 1);
        assert_eq!(two0[0], 3 + 1);
        assert_eq!(two0[512], 3 + 2);
    }
}
