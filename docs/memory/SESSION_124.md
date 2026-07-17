# SESSION_124 — ADR-0040 Filesystem MVP aceite

**Data:** 2026-07-16  
**Objetivo:** Fechar ADR-0040 (`por_fazer`) com MVP honesto — gaps fecháveis + defer explícito.  
**Nota:** SESSION_123 = NeuralFS RAM I/O (paralelo); esta sessão fecha a ADR de governança.

## Mudanças

### Código
- **`k_nano/mhi.rs`:** soft-migrate — metadata + DRAM memcpy limitado; **removido** `write_bytes(0)`; contadores soft.
- **`neural-kernel/mhi.rs`:** re-export `k_nano::mhi` (registry unico DiskAgent + Optimizer).
- **`hermes/optimizer.rs`:** `k_nano::mhi::mhi_tick(_tick)` com tick real.
- **`exfat.rs` (k_nano + nk):** `ExfatFs` + `FilesystemDriver` (detect/mount/list root); write arquivo deferido.
- **Boot:** log `[ADR-0040] MVP wired: ...`.
- NeuralFS agent RAM (SESSION_123) permanece em `/mnt/neural`.

### Governança
- ADR-0040: **Accepted** + §0 MVP + defer.
- INDEX: `0040` → `completa`; `NeuralFS.md` → `fazendo` (disco fisico / multi-level).
- IDEA #417–423 sync; STATE/TODO/TECNOLOGIAS.

## Evidência

```text
cargo nk --target-dir target/check-adr0040
→ Finished release [optimized] ~1m18s — 0 erros
```

## Deferido

exFAT/NTFS/EXT write; DMA NVMe/VRAM; #421 SysInstaller; #423 GPU Direct; cloud pleno; NeuralFS disco fisico.

## Gate

ADR-0040 saiu de `por_fazer`. Gate v2.0.0 ainda exige outros `por_fazer` (ex. ADR-0046) + OK humano.
