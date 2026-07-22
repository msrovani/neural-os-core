# ADR-0061: CPU-First BitNet — Estratégia de Adaptação Cognitiva por Topologia de Hardware

**Status:** Proposed  
**Lifecycle:** `por_fazer`  
**Data:** 2026-07-21  
**Ideias:** #479–#490  

---

## Contexto

O Neural-OS-Core sempre tratou GPU/NPU como Tier 1 opcional. Esta ADR formaliza a **CPU como Tier 0 obrigatório** para inferência BitNet 1.58-bit, eliminando a dependência de aceleradores proprietários e transformando processadores de prateleira em motores neurais.

### Física do BitNet 1.58b

BitNet b1.58 elimina multiplicações de ponto flutuante: toda a matemática resume-se a somas e subtrações ternárias (-1, 0, +1). Quatro pesos ocupam 1 byte (2 bits por peso).

| Métrica | FP16 tradicional | BitNet 1.58b | Ganho |
|---------|-----------------|--------------|-------|
| Bytes/parâmetro | 2 | 0,25 | **8×** |
| Modelo 70B em RAM | 140 GB | ~17,5 GB | **8× menos** |
| Modelo 1T em RAM | 2000 GB | ~250 GB | **8× menos** |
| Operação dominante | MatMul FP16 | ADD/SUB i8 | **0 MatMul** |

### Oportunidade: In-Cache Execution

Processadores modernos atingem 2,5–5 TB/s de banda L3 com latência <10ns. Um especialista MoE de 20–80 MB cabe integralmente no L3 de CPUs como AMD EPYC-X (>1 GB L3), Ryzen X3D (96–128 MB), Xeon SPR+ (60–120 MB), Dual Xeon E5 v4 (90–110 MB).

### SIMD sem MatMul

| ISA | Largura | Pesos/ciclo | Cobertura |
|-----|---------|-------------|-----------|
| AVX-512F+BW+VNNI | 512 bits ZMM | 256 | Xeon SPR+, EPYC 4/5 |
| AVX2+FMA | 256 bits YMM | 128 | Xeon E5 v3+, Ryzen, Core |
| SSE4.2 | 128 bits XMM | 64 | Fallback universal |

### Topologia como Estratégia

| Arquitetura | Desafio | Solução no k-nano |
|-------------|---------|-------------------|
| Dual Xeon (QPI/UPI) | Penalidade cross-socket | Socket 0 = Hermes, Socket 1 = Cortex |
| AMD EPYC (NPS4) | 4 domínios NUMA/socket | Frame allocator por nó SRAT |
| AMD Ryzen X3D | Assimetria L3 entre CCDs | Pinning no CCD 3D V-Cache |
| Intel Hybrid (P/E-core) | Heterogeneidade ISA | P-core = matmul, E-core = supervisão |
| Monolítico (UMA) | Sem NUMA | Fast path: allocator global O(1) |

---

## Decisão

1. **CPU-first BitNet** como arquitetura canônica de inferência
2. **Auto-detecção de hardware no boot** via CPUID + ACPI (SRAT/SLIT/PPTT/MADT)
3. **Adaptação cognitiva automática** — Hermes decide política baseada na topologia
4. **Dispatch SIMD dinâmico** — AVX-512 → AVX2 → SSE4.2 → scalar via CPUID runtime
5. **Isolamento NUMA estrito** — frame allocator por nó onde houver múltiplos domínios
6. **Core pinning por classe** — P-core/E-core/CCD 3D V-Cache/NUMA node
7. **GPU/NPU mantidos como Tier 1 opcional** (ADR-0057 intacta)

---

## Arquitetura

```
Boot (UEFI)
  │
  ├─► k_nano::platform_probe::detect()
  │     ├── HypervisorKind, CpuFeatures, CacheTopology, FeatureGate
  │
  ├─► k_nano::hardware::probe::probe()
  │     └── HardwareProfile:: detect() → MultiDomainNuma | AsymmetricCcd | IntelHybrid | StandardUma
  │
  ├─► k_nano::acpi::srat::parse()     → NumaTopologyMap (se MultiDomainNuma)
  ├─► k_nano::mm::orchestrator::init() → instancia MemoryAndThreadStrategy
  │
  ├─► hermes::adaptation::cognitive_adaptation()
  │     └── Gera ExecutionStrategy + políticas (socket, core, simd, moe)
  │
  └─► k_ai::arch::simd::dispatch_bitnet_kernel()
        └── static KERNEL_FN → avx512 | avx2 | sse42 | scalar
```

### Mapa de Módulos

#### k_nano

| Módulo | Status | Descrição |
|--------|--------|-----------|
| `platform_probe` | ✅ | FeatureGate + CpuFeatures + CacheTopology |
| `simd` | ✅ | CR0/CR4/XCR0 enable (estender para AVX-512) |
| `hardware::xeon` | ✅ (536 linhas) | XeonTopologyReport |
| `hardware::topology` | ✅ (659 linhas) | ClientTopologyReport |
| `hardware::epyc` | ✅ (661 linhas) | EpycTopologyReport |
| `hardware::probe` | 🔴 Novo | `HardwareProfile` + `HardwareReport` |
| `acpi::srat` | 🔴 Novo | ACPI SRAT parser |
| `mm::numa_alloc` | 🔴 Novo | Frame allocator por nó NUMA |
| `mm::orchestrator` | 🔴 Novo | `MemoryAndThreadStrategy` trait + 4 impls |

#### k_ai / cortex

| Módulo | Status | Descrição |
|--------|--------|-----------|
| `arch::x86_64` | 🔴 **Disabled** (566 linhas) | 4 kernels SIMD + dispatch |
| `arch::simd` | 🔴 Novo | static `KERNEL_FN` dispatch |
| `cortex::bitnet_avx2` | ✅ | AVX2 ternary matmul |
| `cortex::bitnet_avx512` | 🔴 Novo | AVX-512 ternary matmul |
| `cortex::compute` | ✅ | Dispatch chain + AVX-512 tier |

#### hermes

| Módulo | Status | Descrição |
|--------|--------|-----------|
| `adaptation` | ✅ (433 linhas) | Xeon adaptation |
| `adaptation::client` | ✅ (537 linhas) | Ryzen/Intel Client |
| `adaptation::epyc` | ✅ (472 linhas) | EPYC |

### Perfis de Hardware

```rust
enum HardwareProfile {
    MultiDomainNuma,  // Dual Xeon, EPYC, Threadripper — SRAT obrigatório
    AsymmetricCcd,    // Ryzen 9 X3D — CCD pinning
    IntelHybrid,      // Core Ultra — P-core/E-core/LPE-core
    StandardUma,      // i3/i5/Ryzen 5/7 monolítico — fast path
}
```

### MemoryAndThreadStrategy

```rust
trait MemoryAndThreadStrategy {
    fn alloc_local(&self, size: usize, align: usize) -> Option<PhysAddr>;
    fn pin_thread(&self, thread: ThreadId, role: CoreRole);
    fn pool_for(&self, role: CoreRole) -> &CorePool;
}
```

Implementações:
- `MultiDomainNumaStrategy` — alloc por nó SRAT, 1GB huge pages, pinning por socket
- `AsymmetricCcdStrategy` — alloc UMA global, pinning no CCD 3D V-Cache
- `IntelHybridStrategy` — alloc UMA global, P-core/E-core/LPE-core separados
- `StandardUmaStrategy` — alloc UMA global O(1), MPMC queue

---

## Plano de Implementação

### Fase 0+1: SIMD Foundation + Kernel (agora)

1. **Fix `k_ai::arch` module** — re-enable `pub mod arch;` (diagnosticar erros SIMD)
2. **Add `allow_avx512`** — `BIT_AVX512`, `build_gate()`, `IsaPath::Avx512F`, accessor
3. **Extend `enable_simd`** — XCR0 bits 5 (opmask) e 6 (ZMM-high)
4. **`cortex::bitnet_avx512`** — kernel ZMM 512-bit com `#[target_feature]`
5. **Wire dispatch** — AVX-512 antes de AVX2 na chain, GPU hooks intactos
6. **Wire adaptation** — `hermes::adaptation::cognitive_adaptation()` no boot
7. **`cargo check --release`** 0 erros

### Fase 2: NUMA Memory (depois)

1. `k_nano::acpi::srat` — parser SRAT
2. `k_nano::mm::numa_alloc` — frame allocator por nó
3. 1GB Huge Pages para tensores

### Fase 3: Probe + Orchestrator (depois)

1. `k_nano::hardware::probe` — `HardwareProfile` + probe()
2. `k_nano::mm::orchestrator` — `MemoryAndThreadStrategy` trait + 4 impls
3. `k_ai::arch::simd` — static `KERNEL_FN`
4. Core pinning por classe

---

## Consequências

### Positivas
- Zero dependência de GPU/NPU para inferência BitNet
- Hardware de prateleira vira acelerador neural
- Recupera 15–30% de performance (sem taxa Spectre/Meltdown)
- NUMA-aware allocation maximiza banda em multi-socket
- In-cache execution via L3 (até 5 TB/s)

### Riscos
- AVX-512 indisponível em legacy — fallback para AVX2
- AMX (Intel) adiado para versão futura
- ACPI SRAT parser depende de bootloader 0.11
- GPU/NPU hooks mantidos mas não priorizados (ADR-0057)

---

## Checklist de Aceite

- [ ] `k_ai::arch` reabilitado e compilando
- [ ] `FeatureGate::allow_avx512` funcional
- [ ] `enable_simd` ativa XCR0 bits 5+6 em CPUs compatíveis
- [ ] `cortex::bitnet_avx512` roda ternário em ZMM (256 pesos/ciclo)
- [ ] AVX-512 na chain de dispatch antes de AVX2
- [ ] `hermes::adaptation::cognitive_adaptation()` chamado no boot
- [ ] `k_nano::hardware::probe` detecta 4 perfis
- [ ] `k_nano::acpi::srat` parseia SRAT
- [ ] `k_nano::mm::numa_alloc` com alocação por nó
- [ ] `MemoryAndThreadStrategy` trait + 4 impls
- [ ] Core pinning funcional
- [ ] `cargo check --release` 0 erros
- [ ] INDEX.md + IDEA_BANK atualizados

---

*Esta ADR formaliza a transição do Neural-OS-Core para CPU-first BitNet, onde o processador de prateleira — de um X99 chinês a um EPYC 9965 — é o motor neural principal.*
