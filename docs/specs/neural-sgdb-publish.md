# neural-sgdb — checklist publish crates.io + interop OS

Contrato: ADR-0004 (repo comunitário) ↔ ADR-0063 (OS). Formatos estáveis: **NMD1**, **TKLV/TKCK**.

## Pré-requisitos no clone comunitário

Repo: `https://github.com/msrovani/neural-sgdb` (MIT OR Apache-2.0).

1. `Cargo.toml`: `name = "neural-sgdb"`, version semântica, `license`, `repository`,
   `description`, `readme`, `categories`, `keywords`, `exclude` de benches grandes.
2. `cargo test` (default + `--no-default-features`) PASS.
3. Goldens vivos: `golden_record_bytes`, `golden_nmd1_bytes`, `fnv1a64_known_vector`.
4. `cargo publish --dry-run` sem warnings bloqueantes (docs, missing metadata).
5. Token crates.io do owner; `cargo login` no host do maintainer (não no CI do OS AGPL).

## Comandos (maintainer)

```bash
git clone https://github.com/msrovani/neural-sgdb.git
cd neural-sgdb
cargo test
cargo test --no-default-features
cargo publish --dry-run
# após OK humano:
cargo publish
```

## Gate no OS (este repo)

Após mudanças em `k_nano::storage::tickv` ou `k_ai::sgdb::memory_doc`:

```bash
cargo test -p k-nano interop_tests
cargo test -p k-ai golden_nmd1
```

Os bytes de `encode_record(b"k", b"v")` e NMD1 L1/`k`/`[0xAA]` DEVEM coincidir com o
crate na mesma versão de contrato. Qualquer drift = bump de formato documentado em
`VERSIONING.md` / `MIGRATIONS.md` do crate **e** SESSION no OS.

## Porta futura (OS → crates.io)

Só depois de: (1) publish estável; (2) CI dual-golden; (3) HITL Mode 2 explícito
(AGPL kernel pode depender de MIT/Apache). Até lá: Mode 1 — cópia em `k_ai::sgdb`.
