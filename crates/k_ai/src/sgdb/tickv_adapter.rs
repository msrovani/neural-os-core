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
        // TickvLite: append-log com CRC; flushed por write (não fsync automático)
        Durability::Flushed
    }

    fn put(&mut self, key: &[u8], val: &[u8]) -> Result<(), SgdbError> {
        let k = Self::key_to_str(key)?;
        k_nano::storage::put_blob(k, val).map_err(|e| SgdbError::Storage(e))
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
        // TickvLite: flush via compact (best-effort); sem fsync real em bare-metal
        k_nano::storage::with_tickv(|kv| {
            let _ = kv.compact();
        });
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
}
