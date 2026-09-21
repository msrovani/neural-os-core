# SESSION_384 — Ordem residual: LazyLock + smoltcp 0.14 + wasmi 2.0

**Data:** 2026-09-21  
**Sprint:** v1.9.99-s384  
**Pista:** seguir ordem sugerida pós-s383 (Lazy → x86_64 → smoltcp → wasmi)

## Entregue

### 1. spin::Lazy → LazyLock ✅
- k_ai (cognitive/self_heal/economy/context_window), jarbas display/agent, hermes cognitive_bridge, cortex
- Remove deprecation warn do spin 0.12

### 2. x86_64 0.15 ❌ BLOQUEADO
- Abort: `Step::forward_overflowing` / `backward_overflowing` exigem nightly ≥1.99
- Toolchain pin `nightly-2026-07-05` (1.98) é **irrevogável** no ciclo atual (AGENTS.md: ≥1.99 quebra 0.14; ≤1.97 falta `str_from_utf16le`)
- Revertido para `0.14.11`
- IDEA #597: sub-item x86_64 = blocked-by-toolchain

### 3. smoltcp 0.13 → 0.14 ✅
- k_nano, hermes, neural-kernel
- `poll` → `PollResult` (ignorado via `let _ =`); API restante compatível
- check hermes/k-nano/neural-kernel OK

### 4. wasmi 0.47.2 → 2.0 ✅
- features: `validate`, `prefer-btree-collections`, `libm`, `stable` (no_std)
- API: `instantiate().start()` → `instantiate_and_start()`
- hermes 205/205 incl. wasm_build + wasmi self-tests

## Gates

| Gate | Resultado |
|------|-----------|
| hermes `--lib` | **205/205** |
| jarbas `--lib` | **102/102** |
| k_ai `--lib` | **47/47** (+1 ignored) |
| neural-kernel `--release` check | **0 erros** |

## Ainda abertos (voz metal — paralelo, não bloqueia deps)
- #558b PLAY_SAMPLES_DROPPED==0 (aceite QEMU/HW)
- #559b half-duplex/AEC
- #561b CTC incremental

## IDEA
- #597: spin ✅ + smoltcp ✅; x86_64 ⛔ toolchain
- #599: wasmi 2.0 ✅
