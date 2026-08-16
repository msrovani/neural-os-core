# SESSION_267 — Gate interop TickvLite ↔ neural-sgdb (TKLV/NMD1)

## Objetivo

Fechar o gap SESSION_256: leitor TickvLite do OS host-testável + goldens byte-exatos
alinhados a `neural-sgdb` (MIT OR Apache-2.0, repo comunitário) antes de publish crates.io
e de uma futura dep do OS no crate publicado.

## Entregas

1. **`k_nano::storage::tickv`**
   - API pública de codec: `MAGIC`, `HEADER`, `CKPT_KEY`, `crc32`, `record_size`,
     `encode_record`, `ScanResult`, `scan_volume`
   - Host: `install_ram_flash(size)`, `dump_flash(len)`
   - `mount()` preserva FLASH pré-instalado (não chama `init_flash` se já há RamFlash)
   - `put_raw` usa `encode_record` (contrato único)
   - Testes host `interop_tests`: golden TKLV (`k`/`v` → 512B), scan+tombstone,
     put→dump→`scan_volume`, remount, ckpt key fora do map

2. **`k_ai::sgdb::memory_doc`**
   - Golden NMD1 idêntico a `neural_sgdb::golden_nmd1_bytes` (L1 `"k"` `[0xAA]`)

3. **Docs**
   - Este SESSION + nota em STATE
   - Checklist publish: `docs/specs/neural-sgdb-publish.md`

## Verificação

```bash
cargo test -p k-nano interop_tests -- --nocapture
cargo test -p k-ai golden_nmd1 -- --nocapture
# opcional no clone:
cargo test --manifest-path /tmp/neural-sgdb/Cargo.toml --lib golden_record_bytes
cargo publish --dry-run --manifest-path /tmp/neural-sgdb/Cargo.toml
```

## Honesty / próximos

- OS **não** depende ainda de crates.io — Mode 1 (cópia AGPL em `k_ai::sgdb`).
- Publish `neural-sgdb` é ação do maintainer do repo comunitário (credenciais crates.io).
- Gate bidirecional CI (golden OS ↔ golden crate no mesmo job) = follow-up quando o
  crate estiver no registry + path `vendor` ou fetch em CI.
