# ADR-0019: Neural Cortex — BitNet LLM Integration Architecture

**Status:** Updated (2026-07-06) — v2→v3 format + RoPE + extra norms  
**Previous:** Accepted (2026-06-24)  
**Context:** AIOS needs a real neural brain for intent routing, hardware decision-making, skill orchestration, and memory tier management. The existing `IntentMlp` (16→8→3, hand-crafted weights) serves as a reflex layer. A full BitNet LLM (700M–2B params, 2-bit ternary, ~180–375 MB) serves as the cortex for all non-realtime decisions.

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
                    │  BitNet LLM     │  700M–2B params, 2-bit ternary
                    │  (~180-375 MB)  │  "intenção, ação, tier, skill"
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

The BitNet LLM is the unified brain for ALL non-realtime decisions:

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
| `rms_norm()` | `nn.rs` | Pre-attention, pre-FFN, inner attention, and post-FFN normalization |
| `EventBus` | `event-bus` | LLM output → skill dispatch |
| `SkillRegistry` | `skill-registry` | Execute decisions |
| `NeuralExecutor` | `task/` | Cooperative polling loop for generation |
| `BitmapFrameAllocator` | `memory.rs` | Allocate physical pages for model (375 MB) |
| `PHYS_MEM_OFFSET` | `memory.rs` | Map model data into virtual address space |

## Forward Pass Architecture (v3+)

### Per-Layer Flow (from `cortex.rs:forward_with_kv`)

```
input x: (seq, hidden)

1. RMSNorm attn (rms_attn weight, hidden dims)
2. Q, K, V projections (PackedTernary matmul_hybrid, ADD/SUB only)
   q = Wq @ norm    → (seq, kv_dim)    [kv_dim = q_dim for MHA]
   k = Wk @ norm    → (seq, k_dim)
   v = Wv @ norm    → (seq, k_dim)
3. KV cache append (full K,V stored for subsequent autoregressive calls)
4. RoPE (Rotary Position Embedding) applied to q, k at each position
5. GQA grouped attention (Q heads per KV head)
   For each KV group g:
     Extract k_g, v_g from total cache (seq_len, qk_head_dim)
     For each Q head h in group:
       Extract q_h from new tokens (new_len, qk_head_dim)
       scores = q_h @ k_g^T * (1/√head_dim)
       causal_mask + softmax
       attn_h = scores @ v_g
       Write to attn_out[new_len, kv_dim]
6. Inner attention RMSNorm (rms_inner_attn weight, kv_dim dims)  [v3.1+]
7. O projection → (seq, hidden)
   proj = Wo @ attn_out
8. Residual: x = x + proj
9. RMSNorm FFN (rms_ffn weight, hidden dims)
10. Gate + Up projections
    gate = Wgate @ norm → (seq, ffn_group_size)
    up = Wup @ norm     → (seq, ffn_group_size)
11. SiLU gate: gated = silu(gate) * up
12. Expand: repeat gated num_groups times → (seq, intermediate_size)
    [For standard FFN: ffn_group_size = intermediate_size, no expansion]
13. FFN LayerNorm (rms_ffn_norm weight, intermediate_size dims)  [v3.1+]
14. Down projection → (seq, down_out)
    down = Wdown @ gated
15. Residual: x[..down_out] += down  (partial update)
    [For MHA: down_out = hidden, full update]

Final: RMSNorm final (rms_final weight) + unembed/embed → logits
```

### Supported FFN Modes

| Mode | ffn_group_size | expand? | down_out | Model example |
|---|---|---|---|---|
| Standard (LLaMA) | == intermediate_size | No (≤1 group) | = hidden | 1bitLLM 700M |
| BitFFN grouped | < intermediate_size | Yes (repeat) | = q_dim (GQA) | Microsoft 2B-4T |

## Transformer Block Detail

```
┌──────────────────────────────────────────────┐
│  BitnetDecoderLayer                           │
│                                              │
│  x → RMSNorm(input_layernorm)                │
│    → QKV (BitLinear)                         │
│    → RoPE(q,k)                               │
│    → GQA Attention                           │
│    → RMSNorm(inner_attn_ln)  [v3.1+]         │
│    → O Projection (BitLinear)                │
│    → + residual                              │
│                                              │
│  x → RMSNorm(post_attention_layernorm)       │
│    → Gate (BitLinear)                        │
│    → Up (BitLinear)                          │
│    → SiLU(gate) × up                         │
│    → Expand (if grouped)                     │
│    → RMSNorm(ffn_layernorm)  [v3.1+]         │
│    → Down (BitLinear)                        │
│    → + residual                              │
└──────────────────────────────────────────────┘
```

## RoPE Implementation (v3.1)

Rotary Position Embedding encodes position by rotating Q and K vectors:

```
For each position pos and dimension pair (2d, 2d+1):
  theta = 1 / base^(2d / head_dim)    [base=10000.0 for Llama]
  cos_pos = cos(pos * theta)
  sin_pos = sin(pos * theta)
  
  q_rot[2d]   = q[2d] * cos_pos - q[2d+1] * sin_pos
  q_rot[2d+1] = q[2d] * sin_pos + q[2d+1] * cos_pos
  k_rot[2d]   = k[2d] * cos_pos - k[2d+1] * sin_pos
  k_rot[2d+1] = k[2d] * sin_pos + k[2d+1] * cos_pos
```

Applied in `cortex.rs:forward_with_kv` after QKV projection, before attention scores.
Pre-computes cos/sin tables for max_seq positions at model load time.

## Model Format — `.bitnet` (Binary BitNet)

### Format Evolution

| Version | Sprint | Changes |
|---|---|---|
| v1 | 25–26 | Original (ADR-0019). u16 vocab, char tokenizer, f32 embed+unembed. |
| v2 | 79 | u32 vocab (128K > 65535), ffn_dim added, BPE tokenizer embedded, RMS norm per-layer (vec vs scalar). |
| v3 | 80–84 | Transposed tensor layout (in,out), GQA fields (kv_heads, q_dim), BitFFN grouped (intermediate_size, ffn_group), tie_embeddings flag, medusa heads. |
| v3.1 | 85+ | RoPE cos/sin precompute, inner_attn_ln + ffn_layernorm weight vectors. |

### v3 Header Layout

```
Offset  Size  Field
─────────────────────────────────────
0       4     magic: 0xBE11BE11 ("BELL")
4       2     version: u16 (3)
6       4     num_params: u32
10      2     hidden_dim: u16
12      2     num_layers: u16
14      2     num_heads: u16 (query heads)
16      4     vocab_size: u32
20      2     max_seq_len: u16
22      2     intermediate_size: u16 (FFN hidden dim)
24      2     num_kv_heads: u16 (GQA)
26      2     q_dim: u16 (Q projection output dim)
28      4     num_medusa: u32
32      4     tie_word_embeddings: "TIED" | "    "
36      1     tokenizer_type: 0=none, 1=BPE (tokenizer.json)
37      4     tokenizer_len: u32
41      tokenizer_len     tokenizer_data (raw tokenizer.json)
─────────────────────────────────────
41+tok  N     emb_table: [ternary_packed; hidden × vocab]
41+tok+N      layers[0..num_layers]:
                rms_attn_weight: [f32; hidden]
                rms_ffn_weight: [f32; hidden]
                rms_inner_attn_weight: [f32; kv_dim]       [v3.1+]
                rms_ffn_norm_weight: [f32; intermediate_size]  [v3.1+]
                q_proj: PackedTernaryTensor(hidden, q_dim)
                k_proj: PackedTernaryTensor(hidden, k_dim)
                v_proj: PackedTernaryTensor(hidden, k_dim)
                o_proj: PackedTernaryTensor(q_dim, hidden)
                gate_proj: PackedTernaryTensor(hidden, ffn_group)
                up_proj: PackedTernaryTensor(hidden, ffn_group)
                down_proj: PackedTernaryTensor(intermediate_size, down_out)
─────────────────────────────────────
                rms_final: [f32; hidden]
                unembed (if not tied): PackedTernaryTensor(hidden, vocab)
                medusa_heads (0..num_medusa): PackedTernaryTensor(hidden, vocab)
```

All ternary weights packed 4-per-byte (2-bit encoding: `00=0, 01=+1, 10=-1`).

### Dimension Formulas (v3)

| Field | Formula | Notes |
|---|---|---|
| q_dim | stored in header | = num_heads × head_dim |
| k_dim | num_kv_heads × q_dim / num_heads | = num_kv_heads × head_dim |
| kv_dim | = q_dim | Used as attention output intermediate dim |
| ffn_group | intermediate_size × q_dim / hidden | Standard: == intermediate_size |
| down_out | = q_dim | GQA: q_dim < hidden; MHA: q_dim == hidden |
| head_dim | q_dim / num_heads | e.g., 640/20=32 (BitNet), 96 (1bitLLM) |

### v2 Compatibility

v2 format is still loadable via the `version >= 2` branch in `load_model()`:
- `ffn_dim` → `intermediate_size = ffn_dim * 4`
- `num_kv_heads` inferred as `num_heads / 4`
- Tokenizer data present
- Tensor layout: `(out, in)` — transposed internally if needed

v1 format (Sprint 25–26) is loadable but deprecated:
- Scalar RMS weights (broadcast to hidden dims)
- No tokenizer data
- Char-level ASCII tokenizer hardcoded

## Model Sources

| Model | Source | Params | .bitnet size | Arch | Status |
|---|---|---|---|---|---|
| micro.bitnet | `tools/gen_micro_model.py` | 272K | 12 KB | 64h,2L,2H,256voc | ✅ Dev fallback |
| **1bitLLM 700M** | `1bitLLM/bitnet_b1_58-large` | 728M | ~182 MB | 1536h,24L,16H,32Kvoc | 🔄 Sprint 85 target |
| Microsoft 2B-4T | `microsoft/bitnet-b1.58-2B-4T` | 850M | 1,464 MB | 2560h,30L,20H,128Kvoc | ⏳ QEMU WHPX or HW |
| Microsoft 2B-4T-bf16 | `microsoft/bitnet-b1.58-2B-4T-bf16` | 850M | 3.2 GB (BF16) | — | 🔄 Training only |
| Custom train | `tools/train_tiny_bitnet.py` (future) | 1M+ | Variable | Configurable (Sprint 89+) | ⏳ Pós-MVP |

### 1bitLLM/bitnet_b1_58-large Configuration (Sprint 85 Target)

```json
{
  "hidden_size": 1536, "num_hidden_layers": 24,
  "num_attention_heads": 16, "num_key_value_heads": 16,
  "intermediate_size": 4096, "vocab_size": 32002,
  "max_position_embeddings": 2048,
  "hidden_act": "silu", "tie_word_embeddings": true,
  "rms_norm_eps": 1e-05, "rope_theta": 10000.0,
  "weight_bits": 1, "input_bits": 8
}
```

## Memory Budget

| Component | Size | Allocator | Model |
|---|---|---|---|
| 1bitLLM 700M @ 2-bit | 182 MB | heap (auto-resize) | ✅ 2GB QEMU |
| BitNet 850M @ 2-bit | 375 MB | `allocate_contiguous()` | ⚠️ Needs 4GB+ QEMU |
| Kernel code + .rodata + .bss | ~8 MB | bootloader | ✅ |
| Heap (LockedHeap + Slab) | ~64 MB | `init_heap()` | ✅ |
| Stacks + DMA + WASM | ~40 MB | allocator | ✅ |
| Free | ~1.7 GB | — | ✅ |
| **Total** | **~2 GB** | | ✅ 1bitLLM fits |

## Implementation Status

### Sprint 25–26 (✅ v1 — Attention Engine + Transformer)
- [x] Tensor + PackedTernaryTensor + matmul_hybrid
- [x] RMSNorm, SiLU, causal attention, softmax
- [x] 4-layer BitNet, embed → blocks → rms → unembed
- [x] Char-level tokenizer + generate() autoregressive loop
- [x] `gen_micro_model.py` micro model (272K params)
- [x] `load_model()` bin loader

### Sprint 79–80 (✅ v2 — Real BitNet Support)
- [x] vocab_size u16→u32 (128K support)
- [x] ffn_dim + BPE tokenizer in header
- [x] Per-layer RMS weight vectors (not scalars)
- [x] `download_bitnet.py` HF converter
- [x] QEMU device loader pipeline (phys 0x100000000)
- [x] AVX2 ternary matmul
- [x] KvCache for autoregressive generation (k_dim from actual K projection)

### Sprint 84 (✅ v3 — GQA + BitFFN + Transposed Layout)
- [x] Transposed tensor layout (in, out) for matmul_hybrid compatibility
- [x] GQA: kv_heads, q_dim, k_dim, head_dim formulas
- [x] BitFFN grouped projections (ffn_group, intermediate_size, down_out)
- [x] tie_word_embeddings flag
- [x] Forward pass matches micro model (10 ticks prompt)

### Sprint 85 (🔄 v3.1 — RoPE + Extra Norms)
- [ ] RoPE implementation (cos/sin table, pre-apply to Q/K)
- [ ] inner_attn_ln: RMSNorm after attention output, before O proj
- [ ] ffn_layernorm: RMSNorm after gate×up expand, before down proj
- [ ] `gen_micro_model.py` updated with extra norm vectors
- [ ] `download_bitnet.py` updated for 1bitLLM model
- [ ] Download + convert 1bitLLM/bitnet_b1_58-large (700M, ~180MB)
- [ ] QEMU test: forward pass produces coherent output

### Sprint 89+ (⏳ Future)
- [ ] SubLN support for Microsoft 2B-4T
- [ ] ReLU² activation for Microsoft model
- [ ] Full RoPE table caching
- [ ] On-device fine-tuning (BitNetTrainer, ADR-0033)
- [ ] GGUF loader for third-party models (ADR-0028)
- [ ] Model hot-swap via HTTP download

## Consequences

- **Positive:** Unified neural brain for all AIOS decisions — intent, hardware, memory, trust, power
- **Positive:** Leverages existing tensor pipeline (Tensor, SiLU, RMSNorm, hybrid matmul)
- **Positive:** Reflex MLP keeps latency low for simple decisions; LLM only for complex ones
- **Risk:** 1bitLLM 700M at ~5-15 tok/s may be slow for interactive chat on QEMU TCG
- **Mitigation:** micro model (272K) for dev testing, full model on real hardware with AVX2
- **Risk:** RoPE increases per-token computation overhead (~2× Q/K element operations)
- **Mitigation:** Pre-computed cos/sin tables reduce runtime to O(head_dim) per token per head
