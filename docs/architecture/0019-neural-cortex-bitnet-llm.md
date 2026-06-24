# ADR-0019: Neural Cortex — BitNet LLM Integration Architecture

**Status:** Accepted (2026-06-24)  
**Context:** AIOS needs a real neural brain for intent routing, hardware decision-making, skill orchestration, and memory tier management. The existing `IntentMlp` (16→8→3, hand-crafted weights) serves as a reflex layer. A full BitNet LLM (1.5B params, 2-bit ternary, ~375 MB) serves as the cortex for all non-realtime decisions.

## Architecture

### Three-Layer Neural Decision Pipeline

```
                ┌──────────────────────────────────────────┐
                │           USER / HARDWARE EVENT          │
                └────────────┬─────────────────────────────┘
                             │
                    ┌────────▼────────┐
                    │  Ring 0         │
                    │  Reflex MLP     │  Tiny MLP (16→8→3)
                    │  (~0.4 KB)      │  sub-ms: "precisa do LLM?"
                    │  sub-µs         │
                    └────────┬────────┘
                             │ se "precisa pensar"
                    ┌────────▼────────┐
                    │  Ring 1         │
                    │  BitNet LLM     │  1.5B params, 2-bit ternary
                    │  (~375 MB)      │  "intenção, ação, tier, skill"
                    │  ~5-15 tok/s    │
                    └────────┬────────┘
                             │ decisão
                    ┌────────▼────────┐
                    │  Ring 2         │
                    │  WASM Skills    │  executa ação
                    │  SkillRegistry  │  SystemStatus, Echo,
                    │                 │  TrustAllow, NetDiag, ...
                    └─────────────────┘
```

### When the LLM fires (not every interrupt)

| Event | Layer | Latency |
|---|---|---|
| Keyboard scancode IRQ | Ring 0 Top-Half | ~1 µs |
| → `hw_bridge_daemon` polls atomic | Ring 0 Reflex MLP | ~10 µs |
| → ENTER → `USER_INTENT` | Ring 0 Reflex MLP | ~10 µs |
| → If complex command → **LLM** | Ring 1 BitNet | ~100-500 ms |
| USB device insertion | Ring 0 Reflex MLP | ~10 µs |
| → Unknown device → **LLM classifies** | Ring 1 BitNet | ~100-500 ms |
| Memory pressure → choose tier | Ring 0 Reflex MLP | ~10 µs |
| → Complex allocation → **LLM decides** | Ring 1 BitNet | ~100-500 ms |
| Mouse movement / PIT tick | Ring 0 hardware (no ML) | ~1 µs |

### LLM Decision Domains

The BitNet LLM (1.5B) is the unified brain for ALL non-realtime decisions:

1. **Intent Classification** — "/status", "/echo", "/trust allow", "/hw", chat, etc.
2. **Hardware Architecture** — `SystemArchitecture::infer()` via MLP (currently heuristics, items #51, #48)
3. **Memory Tier Routing** — choose Dram/Vram/Nvme/Hdd per allocation request (item #116)
4. **USB Device Trust** — Neural Cortex classify MLP 7→5: allow/deny/learn/no_intent/suspect (items #3, #10)
5. **Ring Assignment** — which ring (0/1/2) should handle a given task
6. **Skill Dispatch** — which skill to invoke, with what token, and what priority
7. **Power Management** — governor selection based on workload prediction
8. **Network Decisions** — trust remote hosts, route packets, filter threats

### Existing Components (reused)

| Component | File | Use in LLM pipeline |
|---|---|---|
| `Tensor` (f32 matmul) | `tensor.rs` | Attention QK^T, V projection, FFN |
| `PackedTernaryTensor` (2-bit) | `tensor.rs` | All weight storage (12× compression vs f32) |
| `matmul_hybrid()` (ADD/SUB) | `tensor.rs` | Weight matmuls — zero multiplications |
| `Linear { weights, bias }` | `nn.rs` | Per-layer projections |
| `silu()` | `nn.rs` | FFN activation |
| `rms_norm()` | `nn.rs` | Pre-attention and pre-FFN normalization |
| `EventBus` | `event-bus` | LLM output → skill dispatch |
| `SkillRegistry` | `skill-registry` | Execute decisions |
| `NeuralExecutor` | `task/` | Cooperative polling loop for generation |
| `BitmapFrameAllocator` | `memory.rs` | Allocate physical pages for model (375 MB) |
| `PHYS_MEM_OFFSET` | `memory.rs` | Map model data into virtual address space |

## Model Format — `.bitnet` (Binary BitNet)

```
Offset  Size  Field
─────────────────────────────────────
0       4     magic: 0xBE11BE11 ("BELL")
4       2     version: 0x0001
6       4     num_params: u32 (e.g., 1_500_000_000)
10      2     hidden_dim: u16
12      2     num_layers: u16
14      2     num_heads: u16
16      2     vocab_size: u16 (emb_table entries)
18      2     max_seq_len: u16
20      4     reserved[4]: zero
24      N     emb_table: [ternary_packed; vocab_size * hidden_dim]
24+N    M     layers[0..num_layers]:
                ├── rms_attn_weight: f32
                ├── q_proj: PackedTernaryTensor(hidden_dim, hidden_dim)
                ├── k_proj: PackedTernaryTensor(hidden_dim, hidden_dim)
                ├── v_proj: PackedTernaryTensor(hidden_dim, hidden_dim)
                ├── o_proj: PackedTernaryTensor(hidden_dim, hidden_dim)
                ├── rms_ffn_weight: f32
                ├── gate_proj: PackedTernaryTensor(hidden_dim, ffn_dim)
                ├── up_proj: PackedTernaryTensor(hidden_dim, ffn_dim)
                └── down_proj: PackedTernaryTensor(ffn_dim, hidden_dim)
24+N+M   P     unemb_table: [ternary_packed; hidden_dim * vocab_size]
```

All ternary weights packed 4-per-byte (2-bit encoding: `00=0, 01=+1, 10=-1`).

## Memory Budget (2 GB QEMU)

| Component | Size | Allocator |
|---|---|---|
| BitNet LLM 1.5B @ 2-bit | 375 MB | `allocate_contiguous()` direct physical |
| Kernel code + .rodata + .bss | ~8 MB | bootloader |
| Heap (LockedHeap + Slab) | ~64 MB | `init_heap()` expandido |
| Stacks (BSP + APs) | ~2 MB | bootloader + allocator |
| DMA buffers (e1000, NVMe) | ~4 MB | allocator |
| RX/TX network buffers | ~8 MB | allocator |
| WASM linear memory pool | ~32 MB | allocator |
| Free | ~1.5 GB | — |
| **Total** | **~2 GB** | ✅ Folgado |

## Implementation Plan — 5 Sprints

### Sprint 25 — Attention Engine (v0.19.0)

**Goal:** Implement the transformer building blocks, test with micro-model (1M params)

Deliverables:
- [ ] `Attention` struct: `q_proj`, `k_proj`, `v_proj`, `o_proj` (all `Linear` with packed weights)
- [ ] `causal_mask` — upper-triangular matrix (-inf above diagonal)
- [ ] `softmax` — row-wise on `Tensor`
- [ ] `TransformerBlock` struct: `rms_norm` → `Attention` → residual → `rms_norm` → FFN (SiLU) → residual
- [ ] `Transformer` struct: embed → blocks → rms_norm → unembed
- [ ] `Tokenizer` — character-level (ASCII 32-126 + special tokens: `<BOS>`, `<EOS>`, `<PAD>`)
- [ ] `generate()` — autoregressive loop: tokenize → forward → sample → next token → stop at `<EOS>` or `max_len`
- [ ] Training script (Python): train tiny BitNet (~1M params) on TinyStories → export `.bitnet`
- [ ] Model loader: `include_bytes!` micro-model → `allocate_contiguous` → unpack into `PackedTernaryTensor`
- [ ] Dry-run: "Hermes: hello" → generate → character-by-character output
- [ ] ADR-0020: Transformer Engine Implementation

Tests:
- Forward pass matches Python reference output
- Inference at ~100+ tok/s (micro-model 1M)
- No new deps (all `no_std`)

### Sprint 26 — Cortex Daemon (v0.20.0)

**Goal:** Replace mock `intent_router_daemon` with real BitNet LLM decision daemon

Deliverables:
- [ ] `cortex_daemon` — async task: subscribe `LLM_REQUEST` → `generate()` → publish `LLM_RESPONSE`
- [ ] Prompt template: `[INST] user_text [/INST]` style for instruction tuning
- [ ] `intent_router_daemon` refactored: complex text → `LLM_REQUEST` event → await response
- [ ] USB classify via LLM: `device_descriptor → LLM → TrustAllow/Deny`
- [ ] Memory tier decision: `allocation_request → LLM → Dram/Vram/Nvme/Hdd`
- [ ] Skill dispatch via LLM: `user_intent → LLM → which_skill + payload`
- [ ] Model update via HTTP: `POST /model/update` → download new `.bitnet` → hot-swap weights
- [ ] 1.5B model training (Python, host-side): distill from Llama 3.2 1B → ternary quantize → export `.bitnet`
- [ ] Benchmark: 5-15 tok/s on single x86-64 core, QEMU

Tests:
- End-to-end: keyboard → USER_INTENT → LLM → skill output displayed on VGA
- Model hot-swap without reboot

### Sprint 27 — Reflex+LLM Tuning (v0.21.0)

**Goal:** Tune the Reflex MLP routing threshold + LLM response quality

Deliverables:
- [ ] Reflex MLP confidence threshold: if MLP confidence > 0.9 → route without LLM
- [ ] LLM context window management: sliding window of last N tokens
- [ ] Instruction prompt optimization (shorter → faster generation)
- [ ] Sampling strategies: argmax, top-k, temperature
- [ ] Speculative decoding: Reflex MLP predicts next token, LLM verifies
- [ ] Model quantization calibration (`.bitnet` export improvements)

### Sprint 28 — Networked Cortex (v0.22.0) [Post-Network Sprint]

**Goal:** LLM can communicate over network, download updates, collaborate

Deliverables:
- [ ] `llm_skill` — skill that wraps LLM for external agents
- [ ] Remote LLM fallback: if local model too slow, query remote API
- [ ] Weight update via `bitnet.download(url, hash)` — authenticated model updates
- [ ] Multi-model support: tiny (250 KB) for quick decisions, full (375 MB) for complex reasoning

### Sprint 29+ — Success Engine (v1.0.0) [Item #106]

**Goal:** Online weight adjustment based on user feedback

Deliverables:
- [ ] Feedback loop: user rates LLM response (👍/👎) → adjust weights
- [ ] Ternary weight update: {-1,0,+1} → {-1,0,+1} with probability
- [ ] Experience replay buffer (last N interactions)
- [ ] Periodic weight consolidation → export updated model
- [ ] **Pesquisa acadêmica:** success-based online learning in bare-metal `no_std`

## Training Pipeline (Host-side, Python)

The AIOS does NOT train the model. Training happens on a beefy host:

```bash
# 1. Clone model
git clone https://github.com/microsoft/bitnet.cpp
cd bitnet.cpp

# 2. Download/convert base model
wget https://huggingface.co/meta-llama/Llama-3.2-1B

# 3. Quantize to 1.58-bit (ternary)
python3 bitnet_quantize.py \
    --model Llama-3.2-1B \
    --output model.bitnet \
    --format bitnet-v1 \
    --pack 2bit

# 4. Test inference
python3 bitnet_infer.py --model model.bitnet --prompt "Hello"

# 5. Embed in kernel
rustc -C embed-bitnet=model.bitnet
```

Or custom train on TinyStories for micro-model:
```bash
python3 train_tiny_bitnet.py \
    --dataset tinystories \
    --params 1M \
    --layers 4 \
    --hidden 128 \
    --heads 4 \
    --epochs 10 \
    --quantize ternary \
    --export micro.bitnet
```

## Success Criteria

| Milestone | Date | Criteria |
|---|---|---|
| Sprint 25 | TBD | Micro-model generates coherent single words |
| Sprint 26 | TBD | LLM routes intents better than heuristic MLP |
| Sprint 28 | TBD | Model downloaded via HTTP, hot-swapped |
| Sprint 29+ | TBD | User feedback improves next responses |

## Consequences

- **Positive:** Unified neural brain for all AIOS decisions — intent, hardware, memory, trust, power
- **Positive:** Leverages existing tensor pipeline (Tensor, Linear, SiLU, RMSNorm, BitLinear)
- **Positive:** Reflex MLP keeps latency low for simple decisions; LLM only for complex ones
- **Risk:** 1.5B model at ~5-15 tok/s may be too slow for interactive chat on QEMU TCG
- **Mitigation:** Tiny model (1M) for MVP, full model only on real hardware with AVX2
- **Risk:** `.bitnet` training pipeline requires Python toolchain (host-side, not in kernel)
- **Mitigation:** Training pipeline is well-understood (bitnet.cpp + HuggingFace)
