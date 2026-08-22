# ADR-0055: Revisão SMP — FeatureGate, ISA, Caches e Multicore Real

**Data:** 2026-07-18  
**Status:** Accepted (Fase A evidenciada TCG; metal K22 SESSION_281 — ICR/GDT; aceite `online==madt-1` aberto)  
**Lifecycle (INDEX):** `fazendo`  
**Substitui (deprecated):**
- ADR-0037 — tema SMP / SPSC / IPI / PerCpu / work-steal / parallel matmul CPU
- ADR-0005 — autoridade de enablement ISA/FPU/SIMD (histórico SSE absorvido)
- ADR-0014 § SMP / CorePools / P·E / assign_cores  
**GPU:** ADR-0048 NVIDIA · ADR-0049 AMD · ADR-0050 Intel (ADR-0029 histórico). Não usar ADR-0037 como fonte.  
**Ideias:** #16–42, #20–33, #35–41, #317, #319–324, #347

---

## 0. Absorção das ADRs deprecated

### De ADR-0037
Plano 9 passos SMP (SPSC, IPI, PerCpu, Chase-Lev, parallel-for); crates bbqueue/st3/fast-steal; papers (LithOS, Agent.xpu, …); premissa HW real = aceite de performance; política AVX2 vs hypervisor (regra vigente: só TCG bloqueia; WHPX `-cpu host` = nativo).

### De ADR-0014 §SMP
CorePools R0=1P BSP / R1=P matmul / R2=E+WASM; CPUID 0x1A / 0x0B; `assign_cores()`; HT↔L1 (1 logical/core físico em R0/R1); bordas N-only / `-smp 1`.

### De ADR-0005
CR0/CR4 SSE (EM clear, MP/NE/OSFXSR/OSXMMEXCPT) permanece obrigatório no boot; extensão canônica aqui: OSXSAVE + XCR0 ymm, FeatureGate e `IsaPath`.

---

## 1. Contexto (gap v1.8.6 — snapshot 2026-07; **não** é o tree atual)

Documentação (0037 / IDEA ✅) à frente do código **naquela data**:

O tree 2026-08 (SESSION_279–281) fechou trampoline jmp@0, MADT u32, ICR x2APIC canônico e GDT 1 TSS/CPU. Residual: evidência metal K23; `ap_pollable`/matmul HW; BSS 511.

Snapshot 2026-07 (histórico): trampoline stub, WHPX SIPI VP exit 4, FeatureGate incompleto.

---

## 2. Decisão

### 2.1 PlatformProbe + FeatureGate (`k_nano`)

Inventário único cedo no boot:

1. `HypervisorKind` (None / Kvm / Tcg / MicrosoftHv / VBox / VMware / QemuGeneric / UnknownHv)
2. `CpuFeatures` (SSE4.2, AVX/AVX2, FMA, BMI, OSXSAVE, CLFLUSHOPT, PREFETCHW, UMWAIT, AVX-512/AMX log-only, hybrid 0x1A, topo 0x0B)
3. `CacheTopology` (L1D/L1I/L2/L3, `line_size`; leaf 0x4 / 0x8000_001D)
4. `FeatureGate` = ISA ∩ política ambiente

| Ambiente | SMP SIPI | AVX2/FMA | max APs |
|----------|----------|----------|---------|
| HwReal | ON | ON se ISA+XCR0 | MADT-1 |
| KVM | ON | ON se ISA | min(MADT-1, 4) |
| TCG | ON | OFF | min(MADT-1, 4) |
| WHPX | OFF | ON se ISA | 0 |
| VBox / VMware | OFF | ON se ISA | 0 |

`HwReal` **não** se infere só por NIC — bit hypervisor manda.

Logs canônicos: `[ENV]`, `[CPU]`, `[CACHE]`.

### 2.2 ISA

- SSE (histórico ADR-0005) no boot.
- Se gate AVX: `CR4.OSXSAVE` + `xsetbv` XCR0 = xmm|ymm antes de kernels AVX2.
- Dispatch `IsaPath::{Scalar, Sse42, Avx2Fma}` uma vez no boot.
- Prefetch / CLFLUSHOPT só com flags + gate.
- AMX / AVX-512: inventário log-only (IDEA #347).

### 2.3 Caches

- Tiles attention/matmul derivados de L1/L2.
- Align SPSC/PerCpu/atomics = `line_size` (não hardcode 64).
- Prefetch distance parametrizada.
- HT: 1 logical por core físico em R0/R1 (L1 compartilhado).

### 2.4 Fases de implementação

| Fase | Escopo |
|------|--------|
| **A** | Trampoline raw bytes; PerCpu/AP; SPSC+IPI work loop; gate `allow_smp` |
| **B** | CorePools 0x1A/0x0B; work-steal wired; `parallel_matmul` tile L2 |
| **C** | AgentRegistry affinity R0/R1/R2; ITD/HFI só HwReal se CPUID |

### 2.5 Verdade documental

IDEA #20–41 e claims “AgentScheduler multicore ✅” voltam a `fazendo` até evidência serial (`ap_entry_count`, `[ENV]`, `[CPU]`, `[CACHE]`).

---

## 3. Fora de escopo

- GPU compute (ADR-0048–0050)
- CFS / EEVDF POSIX
- Steal CCD/NUMA sem PPTT
- Kernels AMX / AVX-512

---

## 4. Critérios de aceite

- [x] ADR-0055 gravada; banners Superseded em 0037 / 0005 / 0014§SMP; INDEX atualizado
- [x] Logs `[ENV]` / `[CPU]` / `[CACHE]` no boot (SESSION_141)
- [x] TCG: AVX2 off; WHPX: SMP off estável (`logs/boot_adr55_*`)
- [x] TCG `-smp 2`: `APs acordados: 1` + CorePools (Fase A)
- [x] SESSION_281: ICR x2APIC sem bits reservados; GDT própria 1 TSS/CPU (ADR-0088: teto crate ≠ silício)
- [ ] Metal: K23 + `online == madt_enabled - 1` (i5 7ª / 240H)
- [ ] `parallel_matmul` speedup em HW (Fase B) — código wired; aceite HW
- [ ] CorePools log em Intel hybrid (Fase B/C) — log P-only em QEMU; hybrid = HW

---

## 5. Planos Cursor

| Plano | Escopo |
|-------|--------|
| Sandbox gates SMP | FeatureGate + ISA/caches + Fases A–C |

---

## 6. Referências

- smp-nostd, st3, bbqueue; Intel hybrid WP; LWN ITMT/ITD; ARCAS; A2WS
- Textos históricos ADR-0005 / 0014 / 0037 = archive only (não fonte operacional)
