# ADR-0010: Strategic Roadmap and Architectural Innovations

**Status:** Accepted  
**Date:** 2026-06-21  
**Driver:** Register the long-term strategic roadmap in the knowledge base so that all future architectural decisions remain aligned with state-of-the-art AI Operating System research, phases 3–6.

## Context

The kernel chassis (phases 1–2) is consolidated:

- **Phase 1 (Sprints 1–4):** Toolchain, bootloader, VGA/serial logging, IDT/GDT/TSS, memory paging, heap allocator. The kernel boots on bare-metal x86_64 and can allocate dynamic memory.
- **Phase 2 (Sprints 5–8):** SIMD enablement, Tensor engine, neural primitives (SiLU, RMSNorm), Intent Router MLP, PIC remap, PIT watchdog, Page Fault safety, FrameDeallocator trait.

The following four phases define the innovation roadmap that will differentiate this AIOS from legacy kernel designs. Each phase is a self-contained research vector that may be developed in parallel by separate subagents.

---

## Phase 3 — Ternary Inference (BitNet b1.58)

**Status:** Research  
**Priority:** Critical  
**Depends on:** Phase 2 (SIMD, Tensor engine, Linear layer)

### Problem

Current `f32` matmul is O(n³) and energy-inefficient for billion-parameter models on bare-metal. Running full-precision transformers on CPU without a GPU is impractical.

### Solution

BitNet b1.58 proposes ternary weights constrained to the set {-1, 0, +1}. Matrix multiplication collapses to pure addition and subtraction:

```
Y = X · W_ternary  ⟹  y_ij = Σ_k (± x_ik)  (selectively accumulated)
```

Instead of `multiply-add`, we only `conditional-add` or `conditional-subtract`. This eliminates all FPU multiplications during inference.

### Implementation Strategy

1. **TernaryTensor** (`src/ternary.rs`): storage as two bitmasks (sign + zero) per element, using 2 bits per weight → 4× compression vs `f32`.
2. **Ternarized Linear**: `inference_forward()` accumulates input rows weighted by ternary states. No `libm::expf`, no FPU matmul.
3. **Calibration pass**: convert `f32` weights to ternary via thresholding:

```
w_t = sign(w)  if |w| > Δ, else 0
```

Where `Δ` is learned or computed as `α · E[|w|]`.

### Key Metrics

| Metric | f32 Linear | Ternary Linear |
|---|---|---|
| Ops per neuron | O(n) FMAs | O(n) ADD/SUB |
| Weight memory | 32 bits/param | 2 bits/param |
| Power per op | ~5 pJ (FPU) | ~0.2 pJ (integer) |

### Risks

- Accuracy loss without fine-grained quantization calibration
- Requires `Δ` tuning per layer; may need a small calibration dataset

### References

- BitNet: Scaling 1.58-bit Transformers (Wang et al., 2024, https://arxiv.org/abs/2402.17764)
- Ternary weight networks (Li et al., 2016)

---

## Phase 4 — Zero-Copy Semantic File System (SFS)

**Status:** Research  
**Priority:** High  
**Depends on:** Phase 2 (page tables, OffsetPageTable), Phase 3 (ternary tensors for embedding storage)

### Problem

Legacy vector databases and filesystems introduce multiple copy operations: disk → kernel buffer → user buffer → application. This latency is unacceptable for real-time neural inference in Ring 0, where context memory (KV-cache) must be accessed every forward pass.

### Solution

Abandon block-based storage entirely. Implement a Semantic File System where NVMe storage is mapped directly into the kernel's virtual address space via page tables, using DMA and `zerocopy` crate for safe transmutation:

```
NVMe Physical Pages
    ↓ (DMA + IOMMU)
Mapped into Ring 0 VAS at semantic offsets
    ↓ (no copy)
Neural Microkernel reads KV-cache directly from mapped memory
```

### Architecture

1. **NVMe Driver** (`src/nvme.rs`): minimal driver for NVMe submission/completion queues. Maps device BAR regions via PCIe.
2. **SFS Namespace**: a virtual address range (e.g., `0x5000_0000_0000 – 0x6000_0000_0000`) reserved for memory-mapped semantic storage.
3. **`zerocopy`-based serialization**: Tensor data stored as raw `[f32]` slices transmuted from mapped pages. No `serde`, no allocation.
4. **Episodic memory**: KV-cache entries persisted by keeping physical pages alive across reboots (battery-backed NVMe or S3 sleep).

### `zerocopy` Integration

```rust
use zerocopy::{AsBytes, FromBytes};

// SAFETY: Tensor data layout is contiguous f32
unsafe impl FromBytes for Tensor {}
unsafe impl AsBytes for Tensor {}
```

This permits `&[u8]` ↔ `&Tensor` transmutation over mapped NVMe pages.

### Key Metrics

| Metric | Legacy DB | SFS (Phase 4) |
|---|---|---|
| Read latency | ~10 µs (NVMe + filesystem) | ~1 µs (direct page access) |
| Copy operations | 3–4 | 0 |
| Memory overhead | Double buffering | Zero-copy |

### Risks

- NVMe driver complexity (MSI-X, PRP lists, SGL)
- Page alignment requirements for DMA
- IOMMU setup required for physical device isolation

### References

- `zerocopy` crate: https://crates.io/crates/zerocopy
- NVMe specification 1.4
- Linux VFIO + vfio-pci for reference

---

## Phase 5 — Skills-as-Modules (WASM Component Model)

**Status:** Research  
**Priority:** High  
**Depends on:** Phase 2 (Page Fault barrier, FrameDeallocator)

### Problem

Traditional OSes have "installed applications" with persistent state, files, and long lifetimes. This model conflicts with AIOS principles where the Neural Cortex should instantiate computation on-demand and reclaim memory immediately after execution.

### Solution

Abandon monolithic user-space processes. Use WebAssembly Component Model (`wasmi` crate) for ephemeral *skills* — single-purpose, sandboxed, zero-latency-instantiation micro-modules.

```
User Intent → Neural Cortex → argmax → Skill UUID
    ↓
Cortex loads .wasm from SFS → instantiates in Ring 2
    ↓
Skill executes with bounded memory (pre-allocated WASM linear memory)
    ↓
Skill returns tensor result → Cortex logs → .wasm dropped from RAM
```

### Key Properties

| Property | Legacy Process | WASM Skill |
|---|---|---|
| Instantiation | ~ms (fork/exec) | ~µs (wasmi compile + instantiate) |
| Memory isolation | Page tables (4KB granularity) | Linear memory (1 byte granularity) |
| Resource cleanup | Zombie processes, fd leaks | Automatic drop on `wasmi::Instance` drop |
| Capabilities | Ambient authority | Explicit imports (capability-based) |

### Implementation

1. **`wasmi` integration** (`src/wasm.rs`): embed interpreter with custom host functions for tensor ops.
2. **Linear memory pool**: pre-allocated 64-page (256 KB) slabs per skill, allocated from heap.
3. **Capability imports**: each skill declares required imports (e.g., `nn:silu`, `tensor:matmul`). Cortex validates against a allowlist.
4. **Ephemeral lifetime**: after `argmax` determines the skill is done, `drop(instance)` frees all memory. No GC needed.

### Risks

- `wasmi` interpreter performance (10–50× slower than native)
- WASM component model not yet stable — may require `wasmtime` instead
- Skill ABI design: how do skills communicate back to Cortex?

### References

- `wasmi` crate: https://crates.io/crates/wasmi
- WASM Component Model: https://github.com/WebAssembly/component-model
- Lucet / Fastly: fast WASM sandboxing for edge computing

---

## Phase 6 — Hardware-Aware AIOS Syscalls (Zero-Trust)

**Status:** Research  
**Priority:** Medium  
**Depends on:** Phase 5 (WASM sandbox), Phase 4 (SFS)

### Problem

When a WASM skill (Phase 5) attempts I/O (network, persistent storage, hardware access), the kernel must not grant ambient authority. Traditional syscalls are either allowed or denied; they are not *evaluated semantically*.

### Solution

Every privilege-escalating call from Ring 2 (WASM) is intercepted by a syscall dispatcher that routes the request to the Neural Cortex (Ring 0 LLM) for semantic evaluation *before* execution on silicon.

```
WASM Skill         Cortex (LLM)          Hardware
    │                    │                   │
    │── syscall(read) ──→│                   │
    │                    │── evaluate ──────→│
    │                    │   "Is {addr}      │
    │                    │    in this        │
    │                    │    skill's        │
    │                    │    allowed        │
    │                    │    range?"        │
    │                    │←─ allow/deny ─────│
    │←─ result/error ────│                   │
```

### Implementation

1. **`SyscallIntercept`** (`src/syscall.rs`): a `#[no_mangle] extern "C"` table of entry points callable from WASM via `call_indirect`.
2. **Capability token**: each skill receives a context object on instantiation containing a cryptographically signed (or kernel-verified) token scoped to its allowed operations.
3. **Neural evaluation**: for high-risk syscalls (network write, MMIO access), the dispatcher calls `nn::Linear` → `argmax` to produce allow/deny. This decision is logged as an ADR audit trail.
4. **Zero-Trust default**: all syscalls default to deny. Skills must explicitly request capabilities in their WASM manifest.

### Syscall Categories

| Category | Example | Policy |
|---|---|---|
| Read-only memory | `tensor.matmul` | Always allow |
| Ephemeral allocate | `memory.alloc_page` | Allow up to skill budget |
| Persistent write | `sfs.write` | Cortex evaluation required |
| Hardware access | `pci.mmio_read` | Always deny (reserved for Ring 0) |

### Risks

- LLM evaluation latency on every syscall may be prohibitive (mitigation: cache decisions per token)
- Determining the correct level of semantic granularity for allow/deny

### References

- seL4: capability-based access control
- Google Fuchsia: Zircon capability system
- CHERI: capability hardware extensions

---

## Cross-Phase Dependency Graph

```
Phase 2 (SIMD, Tensor, MLP, PIC, #PF) ──────────────────────────┐
         │                                                        │
         ├──→ Phase 3 (Ternary Inference) ───→ accuracy validation│
         │                                                        │
         ├──→ Phase 4 (SFS / zerocopy) ───────→ IOMMU + NVMe driver
         │                                                        │
         └──→ Phase 5 (WASM Skills) ─────────→ Phase 6 (Syscalls)
                     │                                │
                     └── capability model ────────────┘
```

## Timeline (Estimated)

| Phase | Sprints | Dependencies | Target |
|---|---|---|---|
| Phase 3 | 9–11 | Phase 2, calibration tools | Q3 2026 |
| Phase 4 | 12–15 | Phase 2, NVMe emulation in QEMU | Q4 2026 |
| Phase 5 | 16–18 | Phase 2, Phase 4 (SFS for .wasm storage) | Q1 2027 |
| Phase 6 | 19–21 | Phase 5, Cortex maturity | Q2 2027 |

## Consequences

**Positive:**
- Provides clear research vectors that can be delegated to subagents working in parallel
- Each phase is self-contained, testable, and independently useful
- The roadmap communicates maturity to external contributors and academic partners

**Negative:**
- Phases 5–6 depend on WASM crate ecosystem stability; `wasmi` may need upstream patches
- Phase 4 hardware access requires QEMU with NVMe emulation (`-device nvme`)

**Risks:**
- Accuracy loss in Phase 3 may require hybrid (ternary + f32 residual) approach
- Phase 6 "LLM judges syscall" introduces unpredictable latency; caching layer essential

## References

- ADR-0005: SIMD and FPU Enablement
- ADR-0006: Neural Primitives and libm
- ADR-0007: Intent Router MLP
- ADR-0009: PIC Watchdog and Page Fault Safety
