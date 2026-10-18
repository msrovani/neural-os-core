# SESSION_492 — Materializar .V7 canônicos (QEMU + HW)

**Sprint:** v1.9.99-s492 · **Data:** 2026-10-18

## Goal

Gerar/atualizar artefatos `.V7` em `target1/` conforme ADR-0078 §2.2 para
comporem imagens QEMU (`mkfat32`) e HW (`build_image --hw --unified`).

## Feito

1. **SKU TII já validados** (não re-converter — fontes HF ≤ blobs V7; audit Ok):
   - `FALCON3_BASE.V7` (Active, L=22)
   - `FALCON3_7B.V7` / `FALCON3_10B.V7` (Pro)
   - `FALCON3_1B.V7` (Learner)
2. **`tools/materialize_hub_v7.py`** — hardlinks canônicos:
   - `FALCON3.V7`, `PRO.V7`, `FALCON7B.V7`, `FALCON10.V7`, `LEARNER.V7`, `FALCON1B.V7`
3. **`mkfat32.py`** — finders preferem `.V7`; um dirent/SKU (fim da triplicação V6+BIN).
4. **`fat_names_for`** — `.V7` primeiro em todos os slots.
5. Nichos **VISION / RERANKER / RUSTCDR** permanecem v5; **HwExpert** v6 — sem
   mentir rename→V7 (sem conversor).

## Prova

```
python tools/materialize_hub_v7.py --force   # ok=6 fail=0
python tools/audit_model_blobs.py --dir target1  # Ok=25 REJECT=0
cargo test -p cortex --lib model_hub -- --test-threads=1
```

Pack lab: `PACK_LLM=falcon3` → FAT `FALCON3.V7` + `LEARNER.V7` + nichos.
Pack HW full: `PACK_LLM=all` → + `PRO.V7` + `FALCON10.V7` (exige disco ≥8GB).

## Honestidade

- Reconversão HF→V7 **não** rodada nesta sessão (blobs já size_match).
- Agent / SigLino = residual (sem fonte válida / sem conversor).
