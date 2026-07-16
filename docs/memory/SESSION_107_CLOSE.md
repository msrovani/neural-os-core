# SESSION 107 CLOSE — Sprint 107 Voice I/O (2026-07-16)

5 loops WHPX (`KillMinutes=15`, bridge ON, `-Window -Smp 2`). Soft-float via `cargo nk`; imagem via `bootloader_linker -u` (sem hang `cargo build -p boot`).

## Resultado
**Sprint 107: FECHADA — PASS parcial forte+** (não v2.0; soft-float latency = known blocker doc-only).

## Loops
| # | Log | Cargo | GEN | HWEXPERT | STT |
|---|-----|-------|-----|----------|-----|
| 1 | `boot_whpx_20260716_095549.txt` | 0e/0w | `'LOA,BLOA…'` h=128 | LOADED | blanks 100% |
| 2 | `boot_whpx_20260716_101215.txt` | 0e/0w | `' tempo esta bom'` | LOADED | `ctc='so'` |
| 3 | `boot_whpx_20260716_102813.txt` | 0e/0w | `' tempo esta bom'` | LOADED | `so` + retries |
| 4 | `boot_whpx_20260716_104440.txt` | 0e/0w | `' tempo esta bom'` | LOADED | EventBus STT/INTENT |
| 5 | `boot_whpx_20260716_110041.txt` | 0e/0w | `'O tempo esta'` | LOADED | idem + bias O |

## Fixes chave
1. **L1→L2:** Trinity default `hw_identify` + HWEXPERT LOADED → GEN no vocab=64. Fix: default/rota `generator` + `generate_via_model_with_route(..., "generator")`.
2. **STT:** blank-suppress + CMVN; path CTC non-empty (`so`); LLM ainda seed — `train_stt.py` treina em MFCC synth, não PCM.
3. **EventBus:** `TOPIC_STT_TEXT` + `USER_INTENT` no path clima boot.
4. **Warnings:** 0 em neural-kernel (imports + `model_loaded`).

## Handoff → 108 / N2–N5
- Retrain STT em PCM→MFCC real (fecha CTC útil no LLM).
- Soft-float 2B latency (AVX/host path) — sem fake.
- Mic→WakeWord→STT runtime (além do skinny boot).
- N2 SelfHeal gated; N5 jarbas wire (allocator conflict).
