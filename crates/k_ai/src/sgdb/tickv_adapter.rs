//! TickvStorageAdapter — adapta o `Storage` trait do neural-sgdb ao TickvLite do k_nano.
//!
//! Permite usar `neural_sgdb::Sgdb::open(TickvStorageAdapter)` em bare-metal,
//! delegando put/get/scan/delete para o TickvLite global (k_nano::storage).
//!
//! Fase 1 da migração para neural-sgdb externo.

use alloc::vec::Vec;
use neural_sgdb::storage::{Durability, ScanResult, Storage, SgdbError};

/// Adapter que conecta o `Storage` trait do neural-sgdb ao TickvLite do k_nano.
///
/// # Uso
/// ```ignore
/// let mut adapter = TickvStorageAdapter;
/// let mut db = neural_sgdb::Sgdb::open(adapter)?;
/// ```
pub struct TickvStorageAdapter;

impl TickvStorageAdapter {
    /// Converte `&[u8]` key para `&str` (TickvLite exige UTF-8 string keys).
    fn key_to_str(key: &[u8]) -> Result<&str, SgdbError> {
        core::str::from_utf8(key).map_err(|_| SgdbError::Invalid("key not utf-8"))
    }
}

impl Storage for TickvStorageAdapter {
    fn name(&self) -> &'static str {
        "tickv"
    }

    fn durability(&self) -> Durability {
        // Honesty s385: TickvLite append+CRC sobrevive reboot do guest se media
        // escreveu (file/nvme) = Flushed. RAM = Buffered. Nunca Durable (sem fsync
        // power-fail no bare-metal). sync_durable NÃO faz compact.
        match k_nano::storage::backend_name() {
            "ram" | "none" => Durability::Buffered,
            _ => Durability::Flushed,
        }
    }

    fn put(&mut self, key: &[u8], val: &[u8]) -> Result<(), SgdbError> {
        let k = Self::key_to_str(key)?;
        k_nano::storage::put_blob(k, val).map_err(|e| SgdbError::Storage(e))
    }

    /// Batch put (s410e): UMA aquisição do lock TICKV para N writes + GC
    /// adiado ao fim. O default do trait fazia N× (lock + mount-check +
    /// maybe_gc) — checkpoint L0/L1 do engine e import do Sgdb pagavam isso.
    /// Falha é atômica por item (mesmo contrato do put individual).
    fn put_many(&mut self, items: &[(&[u8], &[u8])]) -> Result<(), SgdbError> {
        // Converte keys &[u8]→&str (UTF-8, contrato do TickvLite) ANTES do
        // lock único; item inválido aborta sem tocar flash.
        let mut converted: Vec<(&str, &[u8])> = Vec::with_capacity(items.len());
        for (k, v) in items {
            converted.push((Self::key_to_str(k)?, v));
        }
        k_nano::storage::put_batch(&converted).map_err(|e| SgdbError::Storage(e))
    }

    fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, SgdbError> {
        let k = Self::key_to_str(key)?;
        match k_nano::storage::get_blob(k) {
            Ok(v) => Ok(Some(v)),
            Err("missing") => Ok(None),
            Err("no tickv") => Ok(None),
            Err("not mounted") => Ok(None),
            Err(e) => Err(SgdbError::Storage(e)),
        }
    }

    fn scan_prefix(&mut self, prefix: &[u8]) -> Result<ScanResult, SgdbError> {
        let p = Self::key_to_str(prefix)?;
        let mut out = Vec::new();
        // Usa with_tickv para aceder ao índice e fazer keys_with_prefix + get
        k_nano::storage::with_tickv(|kv| {
            let keys = kv.keys_with_prefix(p);
            for k in &keys {
                if let Ok(val) = kv.get(k) {
                    out.push((k.as_bytes().to_vec(), val));
                }
            }
        });
        // with_tickv devolve Option<R> — se None (tickv não montado), retorna vazio
        Ok(out)
    }

    fn delete(&mut self, key: &[u8]) -> Result<(), SgdbError> {
        let k = Self::key_to_str(key)?;
        k_nano::storage::with_tickv(|kv| {
            let _ = kv.delete(k);
        });
        Ok(())
    }

    fn sync_durable(&mut self) -> Result<(), SgdbError> {
        // Honesty: TickvLite não tem fsync/power-barrier. put já escreveu na media.
        // compact() ≠ sync — é GC caro (file/nvme hang). No-op OK = Durability::Flushed.
        if k_nano::storage::is_degraded() {
            return Err(SgdbError::Storage("tickv degraded"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neural_sgdb::Sgdb;

    /// Smoke test: abre Sgdb com TickvStorageAdapter, faz put/get roundtrip.
    #[test]
    fn tickv_adapter_put_get_roundtrip() {
        // Este teste só funciona quando o TickvLite está montado (host com ram flash).
        // Em host puro sem montagem, o adapter retorna None/empty — comportamento gracioso.
        let mut adapter = TickvStorageAdapter;
        // Testa o adapter diretamente (sem Sgdb) para validar o bridge
        let result = adapter.put(b"test/adapter/key", b"hello world");
        // Se tickv não está pronto, put_blob retorna Err — aceitar como "skip"
        if result.is_err() {
            return; // tickv não montado neste ambiente de teste
        }
        let got = adapter.get(b"test/adapter/key").unwrap();
        assert_eq!(got, Some(b"hello world".to_vec()));

        // scan_prefix
        let results = adapter.scan_prefix(b"test/").unwrap();
        assert!(!results.is_empty());

        // delete
        adapter.delete(b"test/adapter/key").unwrap();
        let gone = adapter.get(b"test/adapter/key").unwrap();
        assert!(gone.is_none());
    }

    /// Teste neural-sgdb Sgdb::open com TickvStorageAdapter (host).
    #[test]
    fn sgdb_open_with_tickv_adapter() {
        let mut adapter = TickvStorageAdapter;
        // Sgdb::open instancía o engine com ART + BQ + os 8 layers
        let mut db = match Sgdb::open(adapter) {
            Ok(db) => db,
            Err(_) => return, // tickv não disponível neste host
        };

        // L3: remember_text (sem embedding)
        let _ = db.remember_fact("teste de adapter tickv", 1);

        // scan_prefix no ART
        let results = db.scan_prefix("md/L3/").unwrap();
        assert!(!results.is_empty(), "fact deveria ter sido indexado no ART");
    }

    /// s410e: benchmark put_many (batch) vs N× put — mede o ganho do lock
    /// único + GC adiado. Roda no host com RAM flash; imprime µs no stdout.
    #[test]
    fn bench_put_many_vs_individual() {
        use std::time::Instant;
        const N: usize = 256; // ~ checkpoint L0/L1 típico

        // ── baseline: N× put individual ──
        let mut adapter_a = TickvStorageAdapter;
        let t0 = Instant::now();
        for i in 0..N {
            let key = alloc::format!("md/L1/bench_a/{}", i);
            adapter_a.put(key.as_bytes(), b"payload-de-64-bytes-para-ser-realista......".as_slice()).unwrap();
        }
        let individual_us = t0.elapsed().as_micros();

        // ── batch: put_many (lock único + GC adiado) ──
        let mut adapter_b = TickvStorageAdapter;
        let mut items: Vec<(&[u8], &[u8])> = Vec::with_capacity(N);
        // keys precisam viver até o put_many — materializa em Vec<(Vec<u8>, Vec<u8>)>
        let owned: Vec<(Vec<u8>, Vec<u8>)> = (0..N)
            .map(|i| {
                (
                    alloc::format!("md/L1/bench_b/{}", i).into_bytes(),
                    b"payload-de-64-bytes-para-ser-realista......".to_vec(),
                )
            })
            .collect();
        for (k, v) in &owned {
            items.push((k.as_slice(), v.as_slice()));
        }
        let t1 = Instant::now();
        adapter_b.put_many(&items).unwrap();
        let batch_us = t1.elapsed().as_micros();

        // sanity: roundtrip batch
        let got = adapter_b.get(owned[0].0.as_slice()).unwrap();
        assert_eq!(got.as_deref(), Some(&b"payload-de-64-bytes-para-ser-realista......"[..]));

        println!(
            "BENCH put_many: individual(N={})={}us batch={}us speedup={:.2}x",
            N,
            individual_us,
            batch_us,
            individual_us as f64 / batch_us.max(1) as f64
        );
    }
}
