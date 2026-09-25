# SESSION_405 — Memmap Limine sem teto de 64 (i7 16GB via 1378MB)

**Motivo:** greeting no i7 reportava 1378MB RAM com 16GB físicos — `TOTAL_RAM_MB` usa o fim da maior região usável, e a cópia do memmap truncava em 64 entradas (`min(count,64)`, array de 64 sem bound-check dedicado). Mapa denso de notebook (70+ entradas) perdia a cauda = justamente a RAM alta.

## Fix (crates/k_nano/src/limine.rs, +72/-9)
- `MAX_MEM_REGIONS=128` (struct + init).
- `push_usable()` com coalescência de adjacentes + saturação documentada (nunca estoura).
- Loop lê TODAS as entradas; `usable_regions()` inalterado (slice por count).
- 3 testes host (`mem_region_tests`): merge, split, saturação — verde.

## Verificação
`cargo check --release` 0 erros · `cargo test -p k-nano --lib limine` 3/3. Prova real só em HW (i7 deve anunciar ~15GB): pendente de imagem+reboot.
