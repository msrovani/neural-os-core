# SESSION 108 — N1 ✅ → BitNet 2B LOADED → v1.7.0

**Data:** 2026-07-15  
**Sprint:** 107 / marco **v1.7.0**  
**Status:** N1.1–N1.3 ✅ · N3 parcial (**2B LOADED** real) · TTS empty generate 🔴 · **não** v2.0.0

---

## Jornada (boot → Cap → ADR-0042 → 1.6-dev → 2B → TTS)

1. **Boot A/B + Cap P0–P9** (SESSION_107) — Runtime QEMU OK; demos non-fatal.
2. **ADR-0042** — cadeia `k-nano → k-ai → cortex → hermes → jarbas`; gate v2.0 = N1–N5.
3. **Linha 1.6.0-dev** — N1 telemetria `LoadStatus` + `[STATUS]`; log honesto (sem “2B carregado” falso).
4. **Ops QEMU** — soft-float target, `cargo nk`, jobs multicore; disco/FAT; `-RamGB 6 -Smp 4`; timeout ~5 min.
5. **BitNet 2B LOADED de verdade** — evidência `logs/boot_whpx_20260715_112049.txt`: ~590MB, ver=4, h=2560, **L=30**, FWD 0…29/30.
6. **Path clima** — `[JARBAS-STT-SIM]` → Hermes → FWD 30 layers → **`[JARBAS-TTS] FAILED empty generate`**.
7. **Release** — absorve 1.6.0-dev → tag **`v1.7.0`** (sem inventar tag 1.6.0 vazia).

## Evidência QEMU (2026-07-15 — `boot_whpx_20260715_112049.txt`)

| Item | Resultado |
|------|-----------|
| Loader | BITNET2B magic OK @0x100000000, **590680KB** |
| `load_model` | ver=4 h=2560 **L=30** file≈577MB |
| Status | `llm=LOADED` (2B); FWD layers 0→29/30 |
| TTS | **`FAILED empty generate`** |
| Clima e2e | **PARCIAL** (load+FWD OK; saída vazia) |

## Lições críticas

- **Soft-float:** nightly 1.98 + SSE no `x86_64-unknown-none` → LLVM `offset is not a multiple of 16`. Fix: `-C target-feature=-sse…` + alias `cargo nk`.
- **FAT free-scan:** `find_free_clusters` por entrada = hang em disco grande → scan **por setor**.
- **2B format:** ficheiro ~203MB ≠ layout 30×q_dim=2560; packing/export reconciliado → slice **~590MB** LOADED.
- **Timeout QEMU:** boot+load 2B+FWD precisa **≥~5 min** de captura serial (não matar cedo).
- **Treino GPU:** CUDA recente dropa sm_61 (GTX 1050); treino em CPU quando PyTorch não cobre a GPU.
- **Honestidade:** LOADED ≠ generate útil; TTS empty é known issue, não SUCCESS.

## Próximo

- Fix **`generate` / TTS empty** (N3/N5 path).
- N2 SelfHeal gated; N4/N5 além do stub clima.
- Gate **`v2.0.0`** ainda = N1–N5 (ADR-0042).
