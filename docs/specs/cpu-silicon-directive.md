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

## 3. IMPLEMENTADO — AMX (Intel Advanced Matrix Extensions) em `k_nano/src/simd.rs`

- **`amx_cpuid()`** — detecção por tier (host-safe): leaf 7.0 EDX bit 24
  (AMX_TILE), bit 25 (AMX_INT8), leaf 7.1 EAX bit 5 (AMX_FP16) →
  `AmxSupport::{None, Tile, Int8, Fp16}`.
- **`allow_amx()`** — gate hv-aware (mesma política do `allow_ep_core_detect`:
  só `hv ∈ {None, Kvm}`; WHPX/TCG não emulam AMX) + `probe_done()` + OSXSAVE
  (CPUID 1 ECX bit 27 — se o kernel não ativou CR4.OSXSAVE, AMX fica fechado;
  `xsetbv` sem isso daria #GP).
- **`enable_amx()`** — XCR0 bits 17|18 via `xsetbv` + **readback verificado** +
  IA32_XSS (MSR 0xDA0) bit 18 via `rdmsr`/`wrmsr`. `#[cfg(target_os = "none")]`
  — em host (testes) é stub Err, jamais toca XCR0/MSR.
- **`amx_report()`** — status honesto p/ boot log (`amx=off|tile|int8|fp16`).
- **Boot wiring** (`main.rs`, após `enable_simd`): `simd::enable_amx()` com log
  do report (Err silencioso quando não suportado).
- **2 testes host** (gate fecha sem probe, host nunca habilita, report
  consistente com CPUID).

## 4. Verificação
- `cargo clean -p neural-kernel && cargo check --release` → 0 erros
- `cargo test -p k-nano --lib` (127/127) · k-hal 17 · cortex 28 · k_ai 22 ·
  hermes 63 — 0 falhas
- AMX é AWAITING_HW para o caminho de compute real (`tdpnnud`/`tileloadd`):
  enable + detecção prontos; uso dos tiles em matmul = próximo passo (requer
  HW com AMX + intrinsics `amx_tile`).

## 5. Próximos passos (por valor/risco)
1. **Matmul AMX real** — kernel `tdpbusd` (int8) com `#[target_feature(amx_tile,amx_int8)]`
   + runtime gate, espelhando o padrão do `bitnet_avx512.rs` (AWAITING_HW p/ teste).
2. **NUMA-aware allocator** — alocar a Cortex Arena nos nós locais do SRAT
   (parser já existe; falta o allocator por-nó).
3. **Pinning de cache L3/3D V-Cache** — `cache_topology()` já existe em
   `platform_probe`; ligar ao SGDB/HANR quando HW com V-Cache (AWAITING_HW).
4. **AVX10** — detecção via CPUID 7.2; decidir o gate quando CPUs AVX10
   chegarem ao parque.
