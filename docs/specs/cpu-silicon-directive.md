# CPU & Silício — Diretiva v2.0 (auditada contra o código real)

> Target: x86_64 / Limine HHDM / Rust `#![no_std]` · Dogma: IA na raiz — CPU,
> canais de memória e chipset geridos diretamente pelo Cortex.
> Regra: NÃO refazer o que já existe (verificado em commit/sessão).

## 1. BASELINE REAL (quase tudo da diretiva JÁ existe — não re-implementar)

| Item da diretiva | Estado real | Onde |
|---|---|---|
| NUMA via ACPI SRAT/SLIT + mapa de nós | ✅ real (ADR-0061) | `k_nano/src/acpi.rs:710+` — `NumaTopologyMap` + `parse_srat` |
| Hybrid P/E (CPUID 0x1A: 0x40 P-core / 0x20 E-core) | ✅ real | `k_nano/src/core_pinning.rs` — `detect_current_core_class` |
| AMD CCD/topologia (0x8000001E) + heurística V-Cache | ✅ real | `core_pinning.rs` + `k_nano/src/hardware/epyc.rs` |
| Dual Xeon (UPI/NUMA) | ✅ real (scaffold) | `k_nano/src/hardware/xeon.rs` + `hardware/topology.rs` |
| Afinidade P/E nos pools do scheduler | ✅ real (ADR-0057) | `k_nano/src/smp/corepools.rs` (r0=1 r1=2 r2=1 com -smp 4) |
| AVX-512 BitNet (VNNI) | ✅ real | `cortex/src/bitnet_avx512.rs` — `#[target_feature(avx512f,avx512bw,avx512vnni)]` |
| AVX2/SSE BitNet + tails | ✅ real | `cortex/src/bitnet_avx2.rs` / `bitnet_sse.rs` (SESSION_247) |
| SIMD enable (XCR0 AVX/AVX-512 + OSXSAVE + gates) | ✅ real (ADR-0055/0061) | `k_nano/src/simd.rs` + `platform_probe::{allow_avx2, allow_avx512}` |
| VPOPCNTDQ (SGDB) com gate CPUID | ✅ real | `k_ai/src/sgdb/hamming_dispatch.rs` |
| **AMX (Advanced Matrix Extensions)** | ❌ era gap → **IMPLEMENTADO agora** | `k_nano/src/simd.rs` |

## 2. Correções ao exemplo colado

- `crates/cortex/src/backend/x86_avx512.rs` **não existe** — o real é
  `cortex/src/bitnet_avx512.rs` (usa **VNNI** `vpdpbusd`, a instrução certa para
  produto de int8; VPTERNLOGD do exemplo é inferior para BitNet).
- O `bitnet_dot_product_avx512` colado **não é o produto ternário**:
  `Σ popcnt(a⊕b)` conta divergências, não `Σ(a·b)` de −1/0/+1. O correto é
  `2·popcnt(a&b) − popcnt(a) − popcnt(b)` (ou VNNI). O `bitnet_avx512.rs` real é
  o validado (SESSION_247: forward de treino espelha o kernel).
- AMX no exemplo é **incompleto**: exige CPUID leaf 7.0 EDX bit 24 (AMX_TILE) +
  OSXSAVE + XCR0 bits 17|18 + **IA32_XSS (MSR 0xDA0) bit 18** + `tilecfg` antes
  de `tdpnnud`. `xsetbv` sem OSXSAVE → #GP. O enable do repo faz o caminho
  completo com sanity check de readback e gate por hypervisor.

## 3. AMX (Intel Advanced Matrix Extensions) — GAP honesto

- **Detect/enable em `k_nano/src/simd.rs`:** ainda **não** implementados (`allow_amx`/`enable_amx`/`AmxSupport` ausentes no tree wired). Spec anterior overclaimava.
- **Kernel órfão:** `cortex/src/amx_int8.rs` (WIP no branch silicon) — matmul tiles via asm; requer `allow_amx` + CPU AMX antes de `pub mod`.
- **AWAITING_HW** para compute real (`tdpnnud`/`tileloadd`).

## 4. Verificação
- `cargo check --release` → 0 erros (crates wired)
- AMX matmul = próximo passo só após simd AMX + HW.

## 5. Próximos passos (por valor/risco)
1. **`allow_amx`/`enable_amx` em simd.rs** + wire `amx_int8` com gate runtime.
2. **NUMA-aware allocator** — Cortex Arena nos nós SRAT (parser já existe).
3. **Pinning L3/3D V-Cache** — `cache_topology()` em `platform_probe` → SGDB/HANR (AWAITING_HW).
4. **AVX10** — detecção CPUID 7.2 quando CPUs chegarem.
