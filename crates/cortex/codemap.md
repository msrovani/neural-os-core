# crates/cortex/ — Crate Map (Ring 2, Intelligence)

`no_std` BitNet LLM inference engine: ternary-weight transformer (ADD/SUB matmul, 2-bit packing), Trinity MoE routing, tensors, speculative decoding, structured decoding, GGUF loading, HW Expert multi-head classification, and compute dispatch (NPU/GPU/SMP/AVX-512/mesh). Depends on **k_nano** (platform probes, SMP AP workers, heap, FAT32/ATA, mesh net, EventBus, LatentBus); **k_hal** (GPU) and **k_ai** (NPU) register backends into it via fn-pointers. Ring 3 (hermes) consumes it. 32 `.rs` files; entry point `src/lib.rs`.

## Responsibility

R2 intelligence: runs the on-device LLM — tokenize (char BPE or SentencePiece), embed, run transformer layers whose weights are ternary (−1/0/+1) packed 4-per-byte, produce logits, and sample/argmax to text. Selects *what* to run (Trinity MoE intent routing with trainable ternary router), *where* to run it (compute dispatch ladder), and *how* to constrain output (structured/grammar decoding). Also hosts model lifecycle (ModelHub slots, .bitnet v1–v5 loader/saver, GGUF dequant + FAT streaming) and cognition scaffolding (R3 route replay for on-device router RL, HNSW, cognitive cells, consciousness metrics).

## Design Patterns

| Pattern | Where | Notes |
|---|---|---|
| **Ternary BitNet** | `tensor.rs`, `cortex.rs`, `nn.rs` | `PackedTernaryTensor` (`#[repr(C, align(64))]`), 2-bit encoding 00=0 / 01=+1 / 10=−1, 4 weights/byte. Matmul is ADD/SUB only (`scalar_ternary_matmul`: `1 → +x`, `−1 → −x`, `0 → skip`). `quantize_to_packed` thresholds ±1; `f32_to_ternary_packed` (gguf) uses per-tensor 85th-percentile magnitude threshold. |
| **Runtime SIMD gate (NOT cfg)** | `bitnet_avx2.rs`, `bitnet_avx512.rs`, `bitnet_sse.rs`, `tensor.rs` | All kernels compiled unconditionally for x86_64; dispatch chooses at runtime via `k_nano::platform_probe::allow_avx2()` / `allow_avx512()` (FeatureGate, ADR-0055/0061). `#[target_feature(enable = "avx2")]` / `avx512f,avx512bw,avx512vnni` make them safe under the soft-float build (`-C target-feature=-sse,-sse2,-avx2`). WHPX caveat: the AVX2 gate must NOT test `xsave` (WHPX hides XSAVE while exposing AVX2) — gate is `isa.avx2 && isa.avx && !tcg`. |
| **Honest dispatch ladder** | `compute.rs`, `bitnet_sse.rs`, `parallel_matmul.rs` | `dispatch_ternary` order: mesh Worker→Master (p2p feature) → NPU (R0, `k_ai`) → GPU (R1, `k_hal`, big ops only, registered only after canary Ready) → SMP APs (gated `allow_smp && ap_pollable && ap_entry_count>0`, row/col tile claiming + barrier + `send_ipi_reschedule`) → AVX-512 → AVX2 → SSE4.2 → scalar. Backends register via fn-pointer slots (`AtomicUsize`) to avoid the dep cycle `k_nano ← cortex ← {k_hal,k_ai}`; nothing is faked — unregistered rings are skipped. |
| **Trinity MoE** | `trinity.rs`, `r3.rs`, `global_arena.rs`, `moe.rs` | `TrinityRouter` = `Vec<Expert>` + trainable `router_weight` (PackedTernaryTensor) + f32 `router_embed` (VOCAB×64). Char-vocab `encode` (VOCAB=99), embed-sum → normalize → ternary matmul → softmax → best expert (score>0.15); fallback: keyword classifier + `FALLBACK_GENERATOR`. Expert list: generator, hw_control, hw_identify, rust_coder, disk_diag, security, speech_synth. R3 records `RouteTrace` (embedding/logits pointers) into `TensorArena` (Tier-2 bump allocator, O(1) epoch reset) for replay-based router RL (`update_with_replay` with PPO-style clipped ratio + deterministic Gaussian noise). |
| **MoE layer + dynamic experts** | `moe.rs` | `MoELayer` = shared `BitLinear` + `Int8Router` + expert `BitLinear`s, top-k routing. `DynamicMoE` tracks per-expert hits/confidence/entropy; birth (clone closest router column, ±5% noise), merge (cosine > threshold), split (overworked expert) with pending queues + `flush_all`. `self_test()` gates it. |
| **KV-cache AR generation** | `cortex.rs` | `KvCache` per-layer K/V append; GQA attention (q_group_size = num_heads/num_kv_heads) with FlashAttention-style tiling (`optimal_attention_block` from CacheTopology); RoPE precomputed tables; BitFFN (gate/up ternary → SiLU → down), `soft_stride=3` layer skipping on hidden≥2048 (soft-float budget). |
| **Speculative decoding** | `ngram_spec.rs`, `cortex.rs` | `NgramSpeculator`: rolling LCG hash of last N=8 tokens → last-writer-wins continuation map → draft M=4; parallel `forward_with_kv_all_logits` + `verify_draft`; bonus token after accepted prefix; accept-rate counters → `speedup_estimate`. Disabled under coherence sampling / structured decoder. |
| **Structured decoding** | `decode.rs`, `structured_decode.rs` | `decode::ALLOW_MASK` (global allow-mask, `argmax_constrained` — no-op identity by default, `self_test()` at boot). `StructuredDecoder` = compressed FSM (SGLang, arXiv 2405.16818) over char vocab: `DecodeMode` {Json, Number, Alpha, SkillCmd, ShellSafe, Free}, `OutputGrammar` public API, `mask_logits` + `step` enforce grammar per token. |
| **Pluggable Model trait** | `cortex.rs`, `model_hub.rs`, `gguf.rs` | `trait Model: Send { generate, embed_dim, vocab_size, max_seq }`; impls: `TransformerModel` (BitNet) and `GgufBackedModel` (GGUF → ternary via adaptive threshold). `ModelHub` holds 8 `ModelSlot`s (`Active/Vision/GeneratorPro/Reranker/RustCoder/HwExpert/Learner/Agent`) + `SLOT_MASK` bitmap; `select_generator_slot` escalates on `model_fit` too-tight scoring; FAT 8.3 candidate names per slot. |
| **Coherence sampling** | `cortex.rs` | `sample_token_coherence`: top-64 candidate heap → repetition penalty → temperature → Gumbel-max (`SampleRng` from HW RNG). Alternative selectors: `argmax_row_hf_vocab` (BPE re-score), greeting/weather-constrained, char-vocab. |
| **R3 cognition / replay** | `arena.rs`, `global_arena.rs`, `r3.rs`, `delta.rs` | `TensorArena` (bump, 2GB virt `0x4800_0000_0000` default 512MB, mapped via boot frame allocator); global serialized access + 64-deep trace ring + pending-route handoff Hermes→Cortex; `update_with_replay` trains i8 router weights; `persist_trained_router`/`router_delta_vs_seed` (XOR delta) feed mesh federated sync. |
| **Mesh compute (ADR-0081)** | `compute.rs`, `mesh_distrib.rs` | p2p feature: Worker serializes w+x → `udp_broadcast` port 42069 (`MW\0` request, signed), Master `handle_mesh_request` answers `MR\0`; synchronous wait with TIMER_TICKS timeout (~200), MTU gate 1200B; `poll_mesh_requests` drains EventBus `P2P_PACKET` per tick. `MeshExpertDistributor` scales Trinity experts across nodes by CapacityScore (fallback local when Undecided). |

## Data and Control Flow

**Inference pipeline (tokens → text):** `generate_speculative` → BPE (`bpe::encode`, SP32 with HF merges) or char `Tokenizer::encode` → slim prompt for heavy models (hidden≥2048: 8 tokens) → `forward_with_kv`: embed lookup (ternary column × scale) → per-layer [RMSNorm → Q/K/V ternary matmul via `matmul_hybrid` → `dispatch_ternary` → RoPE → GQA FlashAttention (masked softmax) → O proj → residual → RMSNorm → BitFFN (gate·SiLU·up → down) → residual] → final RMSNorm → unembed (or tied embed) → logits. Selection: structured-decoder/allow-mask filtering → `argmax_row_hf_vocab` / coherence Gumbel-max / char argmax → optional n-gram draft+verify → BPE decode → `projection::publish_thought` (last hidden → f16 LatentBus). Failures degrade to zero tensors, never panic.

**MoE routing flow:** utterance extraction (`extract_user_utterance`: `PERGUNTA:`/`[User]`/cognitive-context envelope) → if router loaded: char-encode → embed-sum → L2-norm → `router_weight.matmul_hybrid` → softmax → best expert > 0.15 wins, else keyword classifier (hw_control → generator chat → per-kind keyword rules → generator default) → `RouteTrace` recorded in arena (embedding/logits pointers, old_log_prob) → SleepCycle replay: `snapshot_route_traces` → `update_with_replay` (advantage = reward−0.5, clipped policy ratio, i8 step updates ±1, Gaussian noise) → `persist_trained_router`.

**Compute dispatch flow:** `dispatch_ternary(w, x)` → (p2p) Worker role → mesh request, fallback local on timeout/MTU → NPU fn-slot → GPU fn-slot if `n,k ≥ 64` → SMP: `parallel_ternary_matmul` claims column tiles atomically, enqueues `ternary_worker` on APs (`ap_work` queue + barrier + `send_ipi_reschedule`), BSP runs too → AVX-512 → AVX2 → SSE → scalar. Counters per ring (`dispatch_summary`) for telemetry.

**Model load flow:** `.bitnet` blob (magic `0xBE11BE11`, v1–v5; QEMU-loader or FAT32 `MODEL.BIN`) → header parse (hidden/layers/heads/vocab/max_seq/intermediate/num_kv/q_dim/medusa/tie/feat bits) → heap pre-grow estimate (`resize_heap_to_mb`, cap 2GB) → embed + per-layer RMS + 7 ternary tensors w/ scales → rms_final → unembed (or tied) → Medusa heads → RoPE tables → `TransformerModel`. `save_model` (v4) is the exact inverse (round-trip self-test). GGUF path: `load_gguf` parse header/meta/tensors → `dequantize_*` (Q4_0/Q5_0/Q8_0/Q4_K/Q6_K) → `f32_to_ternary_packed` → `GgufBackedModel`; streaming (`load_gguf_header_from_disk`) keeps payload on disk for >4GB models. HW Expert v4/v5: VID/DID → 4 packed tokens → backbone → 5 heads (family/fw/agent/caps/next) → `HwPrediction`.

## Integration Points

| Consumer | What it uses |
|---|---|
| **hermes** `agents.rs` | `CortexAgent`: `TOPIC_LLM_REQUEST`/`TOPIC_LLM_RESPONSE`, `generate_via_model`, `generate_via_model_with_decoder`, `model_status`/`model_info`, `model_hub::hub_status`; `/model` cmd: `load_model`+`set_model`, `gguf::load_gguf_model_from_disk`, `print_supported_formats`; `generate_register_map` (PCI HWID → register map); `Cortex::think` (Intent), `Consciousness`, `SelfImprovementLoop`; SleepCycle replay: `global_arena::{reset_moe_cache, snapshot_route_traces, clear_route_traces, token_steps}`, `r3::update_with_replay` |
| **hermes** `globals.rs` | `trinity::TrinityRouter` (owns the static router) |
| **hermes** `structured_decode.rs` | `pub use cortex::cortex::{StructuredDecoder, DecodeMode}` |
| **neural-kernel** `cortex.rs` | `pub use cortex_crate::cortex::*` (emagrecer re-export) |
| **neural-kernel** `main.rs` | QEMU-loader/FAT model bootstrap: `load_model`, `set_model`, `set_rustcoder_model`, `set_hwexpert_model`, `load_hwexpert_v5`+`set_hwexpert_v4_model`, `register_model_slot`, `load_models_multi`, `set_coherence`, `generate_via_model`, `generate_via_model_with_route`, `model_is_loaded`, `CURRENT_MODEL_EMBED_DIM`, `GLOBAL_MODEL_PARAMS` |
| **k_ai** | registers NPU ternary backend: `compute::register_npu_ternary` (Ring 0) |
| **k_hal** | registers GPU ternary backend: `compute::register_gpu_ternary` (Ring 1, after GPU canary Ready) |
| **k_nano** | `platform_probe::{allow_avx2, allow_avx512, allow_smp, cache_size, optimal_attention_block, matmul_tile_rows}`; `smp::{ap_pollable, ap_entry_count, ap_work}`; `apic::send_ipi_reschedule`; `interrupts::TIMER_TICKS` (ticks/timeouts); `allocator::resize_heap_to_mb` (model load); `ATA_DRIVER`+`fat32` (GGUF disk I/O); `net::{mesh, udp_broadcast, noproto}` (p2p matmul); `EVENT_BUS`, `globals::LATENT_BUS` (`projection::publish_thought`), `hw_rng` (sampling/HNSW), `hw_profiler::HwProfile` (`install_adviser`) |

**Key public exports:** `TransformerModel`, `LayerWeights`, `KvCache`, `MedusaHead`, `Tokenizer`, `load_model`, `save_model`, `model_save_roundtrip_self_test`, `generate_text`, `generate_speculative`, `generate_via_model`, `generate_via_model_with_decoder`, `generate_structured`, `generate_register_map`, `trait Model`, `ModelStatus`, `CURRENT_MODEL`, `set_model`, `argmax_row`, `argmax_row_hf_vocab`, `sample_token_coherence`, `Tensor`, `PackedTernaryTensor`, `quantize_to_packed`, `CodebookVQ`, `HwPrediction`; `TrinityRouter`/`Expert`/`ExpertKind`/`init_trinity`; `compute::{dispatch_ternary, register_npu_ternary, register_gpu_ternary, dispatch_summary, poll_mesh_requests}`; `decode::{set_allow_mask, clear_allow_mask, argmax_constrained, self_test}`; `StructuredDecoder`/`DecodeMode`/`OutputGrammar`; `NgramSpeculator`/`verify_draft`; `bpe` facade; `model_hub::{ModelSlot, register_model, select_generator_slot, fat_names_for}`; `gguf` facade; `nn::{BitLinear, Linear, silu, rms_norm}`; `r3`/`global_arena`/`arena`; `HwExpertV4Model`/`predict_hw_v4`/`hwexpert_v4_predict`; `delta::ArchiveTensor`; `model_fit::score_fit`; `install_adviser::advise`; `cellular::CellNetwork`; `hnsw::HnswIndex`; `mesh_distrib::MeshExpertDistributor`; `kv_h2o::h2o_evict`; `projection::publish_thought`; `tv_dsl::scan_and_execute`.

## Submodule Map (all modules are top-level in `src/`)

| Module | Responsibility (one line) |
|---|---|
| `cortex.rs` | Core: `TransformerModel` forward (GQA + FlashAttention + BitFFN + RoPE), `load_model`/`save_model` (.bitnet v1–v5), generation loop + speculative, sampling/argmax selectors, `Model` trait + `CURRENT_MODEL`, HW Expert v4/v5, Intent/Consciousness/SelfImprovementLoop, `generate_register_map` |
| `tensor.rs` | `Tensor` (f32 2D) with AVX2/SMP/scalar matmul, `PackedTernaryTensor` (2-bit pack/unpack, `matmul_hybrid`), VQ codebook, `HwPrediction`, `quantize_to_packed`; AVX2/cache probes via `k_nano::platform_probe` |
| `bitnet_avx2.rs` | Ternary matmul AVX2 kernels (bitwise 16 weights/iter, unpacked path, `avx2_ternary_matmul_impl`) + cache-aware adaptive dispatch |
| `bitnet_avx512.rs` | Ternary matmul AVX-512 (ZMM, 16 weights/iter, `#[target_feature(avx512f,avx512bw,avx512vnni)]`) + self-test |
| `bitnet_sse.rs` | Unified SIMD dispatch AVX-512→AVX2→SSE4.2→scalar, `SimdLevel` detection |
| `nn.rs` | `Linear`/`BitLinear` layers, `silu`, `rms_norm`, `argmax` |
| `compute.rs` | ADR-0057/0061 dispatch choke point: NPU/GPU fn-slots, SMP, AVX-512; ADR-0081 mesh ternary matmul (MW/MR protocol, signed, MTU gate) + `poll_mesh_requests` |
| `parallel_matmul.rs` | SMP parallel f32 + ternary matmul: atomic row/col tile claiming, AP work queue, barrier, IPI wake (gated `ap_pollable`) |
| `decode.rs` | ADR-0057 WS-G: global allow-mask constrained argmax (identity when unset) + deterministic self-test |
| `structured_decode.rs` | Compressed-FSM (SGLang) `StructuredDecoder`: JSON/Number/Alpha/SkillCmd/ShellSafe grammars, `mask_logits`/`step` |
| `trinity.rs` | Trinity MoE router: expert registry, trainable ternary router (embed+weight), keyword fallback, `generate_random_router_weights` (LCG seed 42), `init_trinity` |
| `moe.rs` | `MoELayer` (shared expert + int8 router + top-k experts) and `DynamicMoE` birth/merge/split lifecycle |
| `r3.rs` | Rollout routing replay: `RouteTrace`/`TokenRoute` in arena, `update_with_replay` (PPO-style, Gaussian noise), `persist_trained_router` + mesh delta seam |
| `arena.rs` | Tier-2 bump `TensorArena` (O(1) alloc/reset, 2GB virt region) + boot frame mapping |
| `global_arena.rs` | Global serialized arena access: trace ring (64), pending-route Hermes→Cortex handoff, token-step counter |
| `model_hub.rs` | 8-slot `ModelSlot` hub with `SLOT_MASK`, generator-slot selection + fit escalation, FAT 8.3 names, size→slot heuristic |
| `model_fit.rs` | FitPolicy: `FitClass` Perfect→Deny scoring (RAM/VRAM), footprint tables, slot too-tight escalation |
| `gguf.rs` | GGUF header/meta/tensor parse + Q4_0/Q5_0/Q8_0/Q4_K/Q6_K dequant, `GgufBackedModel`, FAT streaming/range/write for hot-swap |
| `bpe.rs` | Compact BPE vocab (Llama-3 128k + SentencePiece 32k): decode/encode, greeting/weather candidate sets + biases, special-id handling, loaders (QEMU-loader/FAT) |
| `ngram_spec.rs` | N-gram speculative decoder: hash window history, draft propose M=4, `verify_draft`, accept-rate bench |
| `kv_h2o.rs` | H2O heavy-hitter KV eviction + PagedAttention-lite page metadata (ADR-0047-G4) |
| `projection.rs` | Hidden → f16 LatentBus thought projection (`publish_thought`, mean-pool to 256D) |
| `delta.rs` | XOR delta archive for ternary tensors (`ArchiveTensor`) + buffer helpers |
| `hnsw.rs` | HNSW approximate nearest-neighbor index (multi-layer graph, L2) |
| `cellular.rs` | `CellNetwork` of `CognitiveCell`s (MPMC inboxes, fan-in/out, budget-per-tick scheduling) |
| `evolution.rs` | `PlasticityController`: per-region entropy/error/activation → grow/prune decisions |
| `mesh_distrib.rs` | ADR-0081 C2: MoE expert distribution across mesh nodes by capacity (local fallback) |
| `neuos_probe.rs` | Snapshot-safe weight health probe: per-layer Healthy/Degraded stats + soul vector |
| `tv_dsl.rs` | TV-DSL deterministic math expression parser/executor (post-generation, anti-hallucination) |
| `install_adviser.rs` | ADR-0079: LLM (via ModelHub) recommends install target/FS/model tier, hardcoded fallback |
| `burn_flex.rs` | burn-style `FlexBackend` wrapper over `Tensor::matmul`/ternary GEMM (stub, WIP) |

## Notes / Caveats

- p2p feature is **opt-in** (bin enables it); without it compute.rs falls straight to the local ladder. Mesh matmul is synchronous with a ~2s timeout; payloads >1200B fall back locally.
- `#[cfg(test)]` self-tests exist (`structured_decode`, `delta`, `burn_flex`, `bitnet_avx512::self_test`) but `cargo test` is not runnable in the no_std kernel target — boot-time `self_test()` fns (`decode`, `model_save_roundtrip_self_test`, `moe::DynamicMoE::self_test`) are the runtime gates.
- Docs in `docs/architecture/`: ADR-0057 (compute dispatch WS-A..G), ADR-0058 (cards), ADR-0061 (SIMD ladder), ADR-0081 (mesh P2P).
