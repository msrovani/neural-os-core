# ADR-0057: Compute Dispatch — SMP + GPU + NPU para a LLM (K-AI/Cortex/Hermes)

**Data:** 2026-07-20
**Status:** Accepted (WS-A implementado e evidenciado em QEMU `-smp 4`; WS-B/C wired; WS-D/E honestos gated HW/Layer S)
**Lifecycle (INDEX):** `fazendo`
**Relaciona:** ADR-0055 (SMP canônica), ADR-0048/0049/0050 (GPU multigeração), ADR-0014 §NPU/CorePools (deprecated), ADR-0022 (#211 ComputeBackend), ADR-0037 (superseded)
**Ideias:** #20–42, #211, #319–336, #329–331, #345–346, #351, #414, #43–52 (NPU), #454–456

---

## 1. Contexto e problema

Auditoria do código (SESSION pós-v1.9.1) confirmou que **todo o forward pass da LLM
roda em um único core**, no caminho escalar em QEMU (AVX2 desligado em TCG), e que
**nenhum acelerador está conectado ao matmul**:

- **SMP:** o wake de APs só acordava **1 AP** (broadcast SIPI "all excl self" +
  stack/PerCpu compartilhados → com ≥2 APs os APs corrompiam a stack na transição
  de modo). Evidência empírica: `-smp 2` → `APs acordados: 1`; `-smp 3/4` → `0`.
- **Contador dividido:** `AP_ENTRY_COUNTER` existia em `k_nano` **e** em
  `neural-kernel`; o `parallel_matmul` lê o do `k_nano`, que o boot nunca incrementava.
- **Paralelismo desligado:** `parallel_matmul` (f32) existia mas **nunca era chamado**;
  `parallel_ternary_matmul` era stub `None`.
- **GPU:** bring-up multivendor sério, mas **nenhum `gpu_matmul` real** e **zero aresta
  GPU→`cortex`**; `Ready` só com canário em silício.
- **NPU:** apenas premissa "Ring 0 roda em software"; sem detecção nem abstração.

Choke point único do matmul: `cortex/src/tensor.rs::matmul_hybrid` →
`bitnet_avx2::ternary_matmul` (ternário, ≥90% dos FLOPs) e `Tensor::matmul` (atenção f32).

## 2. Decisão

Convergir CPU-SMP, GPU e NPU numa **camada única de dispatch** (`cortex::compute`,
materializa IDEA #211) inserida nos dois choke points, com ordem de fallback **honesta
e determinística**:

```
NPU (Ring 0) → GPU (Ring 1) → CPU-SMP P-cores (Ring 1) → AVX2 → scalar
```

Cada camada só entra se seu gate passou (NPU/GPU registrados e prontos; APs vivos;
FeatureGate AVX2). Nada é "fingido" — sem backend real, cai em CPU/SMP.

Mapeamento de anéis (ADR-0014/0015) → dispatcher:

| Anel | Carga | Origem | Backend alvo |
|------|-------|--------|--------------|
| Ring 0 | intent routing + Trinity router | `hermes::cognitive_bridge` / `trinity.rs` | **NPU** senão BSP |
| Ring 1 | transformer BitNet (Q/K/V/O/FFN, atenção) | `cortex::cortex::forward_with_kv` | **GPU** → **P-cores (APs)** → AVX2 |
| Ring 2 | WASM skills, agents, RAG | `hermes` / `k_ai` | E-cores |

## 3. Workstreams

### WS-A — Wake multi-AP (✅ implementado, evidenciado)
- Directed IPI (`send_init_ipi_to`/`send_sipi_to`) em `k_nano::apic` e `neural-kernel::apic`.
- `k_nano::smp::wake_aps_sequential`: acorda APs **um a um** por LAPIC ID, cada um com
  **stack + PerCpu próprios** (array `AP_PCPU`), polling até `online`.
- Bin delega a `k_nano::smp` e reusa `k_nano::smp::ap_entry` → **unifica o contador** e
  **emagrece** o `neural-kernel` (remoção do `ap_entry`/counter/broadcast duplicados).
- **Evidência:** QEMU `-smp 4` → `APs acordados: 3`, `CorePools r0=1 r1=2 r2=1`.

### WS-B — Paralelismo na inferência (✅ wired, gated por `ap_pollable`)
- `parallel_ternary_matmul` real, particionado **por colunas** (decode `m=1` também escala),
  via `ap_work` + barreira (mesmo mecanismo do `parallel_matmul` f32).
- `Tensor::matmul` roteado a `parallel_matmul`; dispatcher usa APs.
- **Gate de segurança `k_nano::smp::ap_pollable()`** (hoje `false`): como os APs hoje
  estacionam em `hlt` sem IDT/IPI próprios (ver WS-F), enfileirar+esperar barreira
  deadlockaria. Enquanto `false`, o `parallel_*` retorna cedo e o BSP faz o matmul
  (AVX2/scalar) — correto, sem deadlock. Ativa (`true`) quando WS-F on-demand wake landar.
- Aceite de **speedup = HW real** (TCG desliga AVX2; QEMU valida corretude/roteamento).

### WS-C — Camada ComputeBackend (✅ wired)
- `cortex::compute`: `dispatch_ternary` + slots de registro por fn-pointer +
  telemetria (`dispatch_summary`). Inserido em `ternary_matmul` e `Tensor::matmul`.

### WS-D — GPU (✅ hook honesto; kernel = Layer S/HW)
- `k_hal::gpu::compute_dispatch::register_compute_if_ready()`: registra o backend GPU
  **só** quando `BackendState::Ready` (canário `vector_add` em silício).
- **LAYER-S/HW:** kernel ternário no device (BitLinearW2A8, IDEA #330) + KernelPack
  assinado (CUBIN/HSACO/zebin). Enquanto ausente, `gpu_ternary` → `None` (fallback honesto).

### WS-E — NPU XDNA / Intel (✅ detecção + veredito; driver = Layer S/firmware)
- `k_hal::npu`: `detect_npu()` por PCI (AMD XDNA `1022:1502/17F0`; Intel NPU
  `8086:7D1D/643E`), enum `Accelerator {Xdna, IntelNpu, Software}`, `init_npu()` com
  veredito honesto `[NPU-HW] VERDICT=...` e **fallback software** (Ring 0 MLP na CPU, #51).
- **LAYER-S/firmware:** fila+doorbell+MSI-X, overlay Vitis (XDNA) / NCE (Intel), kernel
  no NPU. Não testável sem HW + firmware fechado → não registra no dispatcher.

### WS-F — Scheduler heterogêneo (✅ parcial; on-demand wake = residual HW)
- **Wake robusto:** `wake_aps_sequential` agora faz **retry INIT-SIPI-SIPI (3x)** +
  poll mais longo → confiável no TCG (`-smp 4` → APs=3 de forma estável; jitter de
  agendamento antes causava 0/1 intermitente).
- **Idle `hlt` + gate `ap_pollable`:** APs estacionam em `hlt` (baixo custo, wake
  sequencial confiável). Seam `install_wake_fn`/`wake_aps`/`set_ap_pollable` pronto.
- **Residual (HW):** F1-full = IDT compartilhada por-AP + LAPIC habilitado + handler do
  reschedule-IPI (`sti` no AP, risco de #DF) para usar APs como **workers vivos** sob
  demanda → flip `ap_pollable=true` ativa WS-B. F2 (run-queues por core + RebalanceAgent),
  F4 (per-CPU slab #42), F3 (CFS/EEVDF #335, não-goal ADR-0055) — exigem validação em HW.

### WS-G — Otimizações de LLM (✅ #412; demais = residual com validação de modelo)
- **#412 structured decoding (✅):** `cortex::decode` — máscara de tokens permitidos
  antes do argmax (grammar/JSON). Default sem máscara = **idêntico** ao `argmax_row`
  (zero regressão). Wired em `generate_speculative`. Self-test de boot **PASS** (sem modelo).
- **Residual (exige modelo + geração para validar; não implementado às cegas):** Medusa
  no forward (#140), FlashAttention IO-aware (#414 — atenção atual usa softmax-por-bloco
  não-padrão; mexer altera numérica), PagedAttention (#413), huge pages (#92/#93),
  burn-flex CPU (#333), codebook VQ (#169/#170).

## 4. Layer S / firmware (sponsor / HW real)

| Item | Bloqueio |
|------|----------|
| GPU BitLinearW2A8 kernel + KernelPack assinado | canário em silício + toolchain CUDA/ROCm/L0 offline |
| AMD XDNA driver (fila/overlay) | HW Ryzen AI + firmware/Vitis fechados (#345 ❌ no HW atual) |
| Intel NPU driver (NCE) | HW Meteor/Lunar Lake + firmware (#346 ❌ no HW atual) |

Interface (`cortex::compute` + `k_hal::npu` + `register_compute_if_ready`) fica **pronta
antes** do HW (regra IDEA_BANK Camada S).

## 5. Critérios de aceite

- [x] `cargo check --release` 0 erros.
- [x] QEMU `-smp 4`: 3 APs online (retry) + CorePools `r0=1 r1=2 r2=1`.
- [x] Dispatcher wired; `[COMPUTE]`/`[NPU-HW]` honestos; `[DECODE] self-test PASS`; sem panics.
- [x] WS-B deadlock-proof: gated por `ap_pollable` (BSP faz matmul enquanto APs parked).
- [ ] **HW real:** WS-F on-demand AP wake (IDT/IPI) → `ap_pollable=true`; speedup WS-B; GPU `Ready`+W2A8 (WS-D).
- [ ] **Modelo:** WS-G residual (Medusa/FlashAttention/…) valida com geração real.
- [ ] **Sponsor:** XDNA/Intel NPU golden (WS-E).

## 6. Referências
- ADR-0055 (SMP), ADR-0048/0049/0050 (GPU), ADR-0014 (§NPU/CorePools), ADR-0022 (#211).
- Código: `crates/k_nano/src/{apic.rs,smp/*}`, `crates/cortex/src/{compute.rs,parallel_matmul.rs,bitnet_avx2.rs,tensor.rs}`, `crates/k_hal/src/{npu.rs,gpu/compute_dispatch.rs}`, `crates/neural-kernel/src/smp/mod.rs`.
