# SESSION_409 — Estudo "Runtime Ternário + GPU HAL" (verificação externa + mapa do repo)

**Motivo:** estudar o resumo/white paper v0.9 (runtime ternário no_std-first, Bonsai-8B alvo, BitNet 2B4T referência) contra realidade externa e repo. Duas lanes: @librarian (modelos/papers/toolchains) + @explorer (substrato de compute no repo). Nota de governança: o lote #612–#616 já existia commitado por outra frente (`5cb87da7`) — não duplicado; aqui só #617 (item faltante) + este registro.

## Verificação externa (@librarian, 2026-09-26)
- **Bonsai-8B ✅**: `prism-ml/Ternary-Bonsai-8B-gguf`, Apache-2.0, 8.19B, 36L, GQA 32Q/8KV, vocab 151936, ctx 65536, Q2_0 g128 2.125bpw, 2.03 GiB / F16 16.38 GB; unpacked + MLX existem. **Ressalva**: Q2_0 proprietário exige fork `PrismML-Eng/llama.cpp` (terceiros converteram p/ TQ2_0); origem canônica só `prism-ml`; NOTICE checar antes de redistribuir.
- **Falcon3 1.58-bit ✅ todos, incl. 10B** (`tiiuae/Falcon3-10B-Instruct-1.58bit` + `-GGUF` i2_s); licença **TII Falcon 2.0**; **10B = 40L/FFN 23040** (não 22L/9216 do 3B).
- **BitNet 2B4T ✅ MIT, 4T tokens, bitnet.cpp, kernel W2A8 oficial** (dp4a, interleave); correção: **2.4B params**, `hidden 2560` = variante, não spec universal.
- **BITCOS ✅**: arXiv:2609.16338 (Intel, set/2026); bitmap presença + sinais, `2−z` bpw, vence five-trit se z>0.375, unpack pdep, até ~1.28× decode.
- **rust-gpu/CubeCL/Burn**: nenhum dá host bare-metal `no_std` (host exige nightly+runtime / std / só Flex-CPU). Confirma S344: nvcc segue único produtor GPU real.

## Mapa do repo (@explorer)
- **JÁ-EXISTE**: W2A8/ternário CPU (`bitnet_avx2/sse/avx512/w2a8` + parity), loader GGUF (Q4_0…Q8_K, TQ1_0/TQ2_0) + conversor → FALCON3.V6, hub 8 slots + `get_or_mmap_expert` + arena 4GB + prefill/decode (`infer_queue`), BPE (`bpe.rs`), wasmi+fuel+CapGate.
- **PARCIAL**: GPU (detect wired, compute `None`-honesto, `kernel_pack.rs` envelope sem pack ativo, canário vector_add); KV (f32 denso + H2O + AirLLM streaming).
- **AUSENTE**: KV-INT8/paginado, BITCOS-like/per-tensor/autotune/compiler, Safetensors no kernel (por design: só `tools/` converte).
- **Correção JEV §5**: `AgentJev-0.6B` existe (Qwen3 decoder→classifier, Apache-2.0, 79.25%) — backend local mais factível que o doc assume; rota = destilação #611.

## Tensões (posição do orquestrador)
1. **Estender, não forkar**: `compute-*` paralelo duplicaria hub/arena/dispatch/loaders — absorver em cortex (Runtime), k_hal::gpu (HAL), tools/ (compiler).
2. Mesh ~55ms/tick + FRACK → dispatch fino GPU pela malha inviável; local-fallback é a norma.
3. VRAM: Bonsai-16K (5–5.5 GiB) exige KV-INT8 **antes** de ser útil no metal; 2B4T roda hoje.
4. Delta real ordenado: **KV-INT8 → BITCOS-like → pack GPU Ready → compiler/autotune**, cada um com gate de parity.

## Verificação
Docs-only; zero código. Lote IDEA: #612–#616 pré-existentes (outra frente) + **#617** aqui.
