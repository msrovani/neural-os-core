# SESSION_141 — ADR-0055 FeatureGate + SMP revision

**Data:** 2026-07-18  
**Foco:** Implementar plano Sandbox gates SMP (ADR-0055): PlatformProbe, FeatureGate, ISA/caches, Fases A–C.

## Entrega

### Docs
- `docs/architecture/0055-smp-revision.md` — canônica SMP/ISA/caches/CorePools
- Superseded: ADR-0037 (SMP), ADR-0005 (ISA), ADR-0014 §SMP → 0055
- INDEX Substituições + IDEA_BANK destino=0055

### Código
| Módulo | Papel |
|--------|-------|
| `k_nano::platform_probe` | HypervisorKind, CpuFeatures, CacheTopology, FeatureGate, IsaPath, ITD probe |
| `k_nano::simd` | SSE + OSXSAVE/XCR0 se `allow_avx` |
| `k_nano::smp` | gate early-out; SIPI; `ap_work` + barrier; CorePools 0x1A; WorkStealingPool global |
| `neural-kernel` boot | `detect()` antes de `enable_simd()` / `init_smp` |
| `cortex` | AVX2 via FeatureGate; `parallel_matmul` + tile L2; `cache_size` → probe |
| `agent-core` | `affinity_ring` + poll R0→R1→R2 |

### Política FeatureGate
| Ambiente | SMP | AVX2 |
|----------|-----|------|
| HwReal / KVM | ON | ON se ISA |
| TCG | ON | OFF |
| WHPX / VBox / VMware | OFF | ON se ISA |

## Validação
- `cargo build --release` — 0 erros (1 warn unused re-export, silenced)
- **TCG** `logs/boot_adr55_tcg_20260718_152948.txt`: `smp=true avx2=false`; MADT LAPICs=2; **APs acordados: 1**; CorePools r0=1 r1=1
- **WHPX** `logs/boot_adr55_whpx_20260718_153140.txt`: `smp=false avx2=true isa=avx2+fma`; **BSP-only (FeatureGate hv=WHPX)**; estável

## Fix colateral
- `BootInfo.rsdp_addr` → `acpi::set_boot_rsdp` (scan BIOS sozinho falhava no OVMF → APIC nunca subia)
- Gate AVX2 usa `XSAVE` (capacidade), não `OSXSAVE` (já ligado)

## Residuals
- Speedup matmul só em HW real (aceite Fase B)
- ITD enable deferred (probe log-only)
- CorePools hybrid (0x1A) só em Intel HW
- Trampoline PoC ≠ produção Ring3/SFI
- Atenção: sandbox Cursor pode setar `CARGO_TARGET_DIR` → imagens em `target/` ficam stale; unset antes de `cargo build --release`
