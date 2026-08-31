# ADR-0101: Falcon3-3B como laboratório arquitetural — Falcon3-BitNet Cognitive

**Data:** 2026-08-31  
**Status:** Proposed  
**Lifecycle (INDEX):** `pesquisa` → Onda 0 `por_fazer`  
**IDEA:** **#544** (substitui o *alvo de lab* de #540 / SESSION_291: 7B-first)  
**Sprint:** v1.9.99-s298  
**Evidência:** SESSION_298  
**Não substitui:** ADR-0084 (kernels/fidelidade 2B4T), ADR-0085 (formato v6), ADR-0060 (BEI), ADR-0061 (CPU-first dispatch), ADR-0046 (AirLLM/GGUF), ADR-0019 (Cortex LLM). Complementa todas.  
**Não implementa nesta sessão:** early-exit, vocab shortlist, GQA adaptativo, thinking budget, treino de esparsidade.

---

## 1. Contexto

O Neural OS já tem motor BitNet (packing 2-bit, `PackedTernaryTensor`, v6, Falcon3 pack, AVX2/SSE, AirLLM, Trinity). A tentação de um greenfield “Falcon3-BitNet Cognitive” duplicaria isso.

O maintainer mandou **deslocar o laboratório** de experiências DeepSeek V4-Flash / Falcon3-7B-as-lab para **Falcon3-3B Instruct 1.58-bit** como *small reasoning engine* (adaptive compute + contexto + memória), não extreme-edge 1B.

**Alvo canônico:** `tiiuae/Falcon3-3B-Instruct-1.58bit`.  
**1B:** referência/comparativo opcional.  
**7B (`PRO.v6`):** slot GeneratorPro se couber — não é o lab.  
**Falcon-Edge (`tiiuae/Falcon-E-3B-*`):** linha TII *nativa* BitNet — **arquitetura diferente**, não drop-in do Falcon3-3B.

### 1.1 Mentira já no tree (corrigida em AGENTS.md nesta sessão)

SESSION_288 / AGENTS.md afirmaram que Falcon3-7B e 3B 1.58bit compartilham `hidden=3072 / 28 layers / 12 heads / kv=4 / intermediate=23040`. **Isso é falso.** Configs HuggingFace (2026-08-31):

| | Falcon3-3B Instruct | Falcon3-3B Instruct **1.58bit** | Falcon3-7B Instruct | Falcon3-7B Instruct 1.58bit | Falcon3-1B Instruct | Falcon-E-3B Instruct |
|---|---|---|---|---|---|---|
| hidden | 3072 | 3072 | 3072 | 3072 | **2048** | **2048** |
| layers | **22** | **22** | **28** | **28** | 18 | **32** |
| Q / KV | 12 / 4 | 12 / 4 | 12 / 4 | 12 / 4 | **8 / 4** | **16 / 2** |
| head_dim | 256 | 256 | 256 | 256 | 256 | **128** |
| intermediate | **9216** | **9216** | **23040** | **23040** | 8192 | **13312** |
| vocab | 131072 | 131072 | 131072 | **131080** | 131072 | **32768** |
| context | **32768** | **4096** | 32768 | 32768 | 8192 | 32768 |
| act | silu/SwiGLU | silu + `quant_method=bitnet` | silu | silu + bitnet | silu | silu + bitnet |
| rope_theta | 1000042 | 1000042 | 1000042 | 1000042 | 1000042 | 1000000 |

O 3B e o 7B compartilham *família* (LlamaForCausalLM, GQA 12/4, head 256, hidden 3072, vocab ~131K). **Não** compartilham profundidade nem FFN. O 1.58bit 3B da TII declara `max_position_embeddings=4096` — o “32K do 3B” vale no checkpoint denso Instruct, **não** no 1.58bit publicado. Não inventar 32K no lab 1.58 sem evidência do tensor RoPE.

TECNOLOGIAS.md §7.9 já tinha os números **corretos** do 3B (22L / 9216). O conversor `tools/convert_falcon3_to_v6.py` também. A fonte podre era o parágrafo SESSION_288 em AGENTS.md.

### 1.2 Prova do forward (o entregável mais valioso da Onda 0)

**Pergunta:** o matmul Falcon3 no cortex já é ternário-nativo (ADD / SKIP / SUB) ou ainda é dequant → GEMM FP16/FP32?

**Resposta (código, 2026-08-31):**

| Caminho | Onde | O que faz | Ternário-nativo? |
|---|---|---|---|
| Scalar | `bitnet_avx2::scalar_ternary_matmul` / `bitnet_sse` | `match w { 1 => +=x, -1 => -=x, _ => skip }` sobre **ativações f32** | **Sim** (ADD/SUB/SKIP). Não materializa W denso. |
| SSE “bloco 4” | `bitnet_sse.rs` | idem, 4 colunas | Sim, mesmo contrato |
| AVX2 host `avx2_ternary_matmul_impl` | `bitnet_avx2.rs` | unpack i8 → `_mm256_cvtepi32_ps` → `_mm256_fmadd_ps` | **Não.** Dequant local 8 pesos → FMA f32. Semanticamente ±x, mas bandwidth de W vira f32 no YMM. |
| AVX2 “bitwise” LUT | `avx2_bitwise_matmul` | LUT byte→`[f32;4]` + `_mm_fmadd_ps` | **Não.** Dequant 4 pesos/byte → FMA. **Não compila no kernel** (`cfg(not(target_os="none"))`). |
| Bare-metal AVX2 | `avx2_ternary_matmul_impl` stub | **delega ao scalar** | Sim, lento. WHPX/metal **não** usam o kernel SIMD do host. |
| W2A8 maddubs | `bitnet_w2a8.rs` | int8×ternário (bitnet.cpp) | Mais perto de nativo-int; **`w2a8_enabled()=false`** (`GENERATION_GAPS_RESOLVED=false`) |
| GGUF Q4/Q6_K | `gguf.rs` | dequant → f32 | **Não.** Path AirLLM/legado. |
| GGUF i2_s (type 25) | — | — | **Ausente** (#540 GGUF; não confundir com IDEA #540 7B-lab) |
| BitLinear LLM | `nn.rs` → `matmul_hybrid` | dispatch acima | Segue a tabela |
| Scale per-tensor | `q_scale` *after* matmul | RMS/absmean | OK (não é GEMM denso) |

**Veredito:** o **contrato algébrico** do decode (m=1) no metal é ADD/SUB/SKIP f32. O **contrato de bandwidth** que o lab pede (W permanece packed; SIMD skip-zero; sem LUT→f32) **não está no path quente bare-metal**. ATLAS-TQ1_0 (~7.1 tok/s Falcon3-3B hybrid no i7-7700T, ~10.1 tok/s 1B f32 bypass) é evidência de que kernel+layout dominam — **não** é número do Neural OS. Não citar tok/s nosso até medir.

---

## 2. Decisão

1. **Lab canônico = Falcon3-3B Instruct 1.58-bit** (v6 no FAT: `FALCON3.V6` / `FALCON3B.v6`). Objetivo de produto: *small reasoning engine*.
2. **Onda 0 = kernel+formato+layout no 3B**, não features cognitivas. Critério: decode m=1 do 3B faz ADD/SUB/SKIP packed no SIMD do `x86_64-unknown-none`, com teste de paridade vs scalar. Sem isso, “Falcon3 Cognitive” é doutrina.
3. **Inventário 3B-first:** `falcon3_boot_names` e `fat_names_for(Active)` preferem o 3B. 7B continua GeneratorPro *se* o budget sobrar. 1B = Learner/comparativo.
4. **Não treinar** esparsidade/early-exit/KL até existir pipeline host já no repo (não inventar). Fine-tune de zeros = pesquisa (#544c).
5. **Falcon-Edge 3B** (32L / hidden 2048 / vocab 32K / ~999MB) é referência TII de BitNet *desde o pré-treino*. Converter só com header v6 autodescritivo — **nunca** assumir shapes do Falcon3-3B.
6. **Honesty:** Medusa no `Model` existe (heads no loader); decode speculativo wired = n-gram (`ngram_spec.rs`) + DSD mesh local (`speculative.rs`) **sem** verifier do 3B. KV H2O = eviction por norma, não compressão ternária. Não claim “done”.

---

## 3. Mapa dos 16 pontos (estratégia do maintainer)

| # | Item | Estado no tree | Risco | Onda |
|---|---|---|---|---|
| 1 | Falcon3-3B denso: tok/s sem mudar o modelo (quant, pack, SIMD, fused, cache, persistent) | **Parcial** — v6 + SSE/AVX2 host; metal = scalar; sem persistent kernel | Médio (bandwidth) | 0–1 |
| 2 | Falcon3-1.58 **ternary-native** (não dequant→GEMM) | **Parcial** — packing+scalar nativos; SIMD host = FMA f32; metal sem AVX2 real | **Alto** — é o gap do lab | **0** |
| 3 | Kernel lab W∈{-1,0,+1} ADD/SKIP/SUB SIMD-agressivo | **Parcial** — scalar sim; skip-zero SIMD não; W2A8 gated | Alto (codegen `none`) | **0** |
| 4 | Esparsidade ternária aprendida (zeros baratos) | **Novo** — sem pipeline 3B; receita 1-bit #487 é HW Expert/router | Treino | defer pesquisa |
| 5 | Dynamic Precision (1.58/2/4-bit por layer/proj) | **Novo** — v6 é homogéneo por tensor + Q6_K só no embed | Formato | 1+ (após 0) |
| 6 | Adaptive Vocabulary Projection (131K → shortlist 512/2048) | **Novo** — unembed full `matmul_hybrid` | Qualidade logits | 1 |
| 7 | Speculative vocab-aware (draft 2K + verifier full) | **Parcial** — n-gram + DSD local; Medusa no loader **não** é decode do 3B | Aceite/rejeita | 1 |
| 8 | Adaptive GQA (KV heads por layer) | **Novo** — GQA fixo do header (4 KV no 3B) | Numerics | defer |
| 9 | KV cache ternário/comprimido (FP8/INT4/INT2/latent) | **Parcial** — `kv_h2o` eviction; cache f32 | Qualidade longa ctx | 1–2 |
| 10 | Cognitive Runtime (Difficulty Gate cheap/normal/full) | **Parcial** — BEI/PonderNet/IterationBudget (ADR-0060) **não** gated no forward 3B | Produto | 2–3 |
| 11 | Early exit treinado em KL(P_inter \|\| P_final) | **Novo** — sem treino 3B | Precisa dados | 2 |
| 12 | Dynamic layer skip → Conditional Computation | **Novo** | Idem | 2 |
| 13 | Thinking budget (compute **por token**) | **Parcial** — IterationBudget/PonderNet não no loop Falcon3 | UX | 2–3 |
| 14 | Protótipo Falcon3-1.58 Adaptive (gate+spec+ternary+KV) | **Novo** — composição das ondas 0–2 | Integração | 3 |
| 15 | Split: 1B edge vs **3B reasoning** | **Parcial** — `Falcon3Kind` existe; lab era 7B (SESSION_291) | Política pack | **0** (docs+nomes) |
| 16 | ATLAS-TQ1_0 + Falcon-Edge como evidência externa | **Novo** (referência) — 7.1 / 10.1 tok/s são *deles* no i7-7700T | Não copiar .atlas | 0 pesquisa |

---

## 4. Ondas

### Onda 0 — Kernel lab 3B (única que pode virar código cedo)

Escopo: formato de pesos v6 do **3B 1.58**, layout packed, SIMD ADD/SUB/SKIP no target `none`, inventário FAT 3B-first.

Aceite:

- [ ] `FALCON3.V6` gerado de `tiiuae/Falcon3-3B-Instruct-1.58bit` via `convert_falcon3_bitnet.py` / `convert_falcon3_to_v6.py`; header bate 3072 / 22L / 12H / kv=4 / 9216 / vocab 131072 / silu.
- [ ] Forward metal: packed → ADD/SUB/SKIP **sem** materializar W f32 (teste paridade scalar vs SIMD).
- [ ] AVX2 no `x86_64-unknown-none` **ou** SSE skip-native documentado; hoje o stub metal→scalar é o gap.
- [ ] Sem tok/s inventado; opcional: microbench host vs ATLAS (ordem de grandeza, não claim de produto).

**Nesta SESSION_298:** prova + ADR + inventário 3B-first. **Não** reescrever AVX2 (é o lab, não um diff de 40 linhas honesto).

### Onda 1 — Decode / vocab (após Onda 0 verde)

Shortlist de logits, n-gram/Medusa *wired no 3B*, KV H2O medido, i2_s GGUF só se AirLLM for o path (não o lab nativo).

### Onda 2 — Adaptive compute

Difficulty gate no `generate_next` do 3B; early-exit só com KL treinado (senão softmax-confidence = mentira); layer skip = residual.

### Onda 3 — Falcon3-BitNet Cognitive

Composição: gate + spec + kernel nativo + KV compress + SGDB (ADR-0091). Nome de produto **depois** de Onda 0 medida.

---

## 5. Pack / conversão (gap honesto)

| Artefato | Estado conhecido |
|---|---|
| `target1/PRO.v6` | SESSION_288: Falcon3-**7B** ~1.86GB shipped |
| `FALCON3.V6` 3B | Pipeline existe (`PACK_LLM=falcon3` default em `build_image.py`); **não assumir** que o FAT HW atual tem o 3B — SESSION_291 priorizou 7B |
| Conversão 3B 1.58 | `python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-3B-Instruct-1.58bit --output target1/FALCON3.V6` |
| Alternativa | `tools/convert_falcon3_to_v6.py` (Base denso ou 1.58 unpack HF) |
| Slot bug legado | `hub_slot_for_kind(Daily3B)` era `Agent`; `slot_from_bitnet_bytes` 771MB → Agent. Lab 3B pertence a **Active** |
| Embed 1.58 TII | `modules_to_not_convert: lm_head` — lm_head/embed podem ser BF16 no HF; v6 usa Q6_K ou ternário conforme writer |

---

## 6. Relação com ADRs

- **0084:** F4 W2A8 continua gated; Onda 0 deste ADR é o *motivo* para ligar um path nativo no 3B (não no 2B4T).
- **0085:** formato não reabre; o 3B é mais um consumidor do header autodescritivo.
- **0060 BEI:** Cognitive Runtime (itens 10–14) reusa PonderNet/budget **como política**, não como substituto do forward.
- **0046:** AirLLM/GGUF = fallback se o 3B residente não couber; **não** é o lab (lab = v6 packed).
- **0100:** não absorve esta ADR; T-058–T-065 (W2A8) cruzam a Onda 0.

---

## 7. Non-goals

- Implementar os 16 itens.
- Claim tok/s Neural OS.
- Treinar early-exit/KL nesta sprint.
- Unificar Falcon-Edge shapes com Falcon3-3B.
- Engordar `neural-kernel` (lógica em `cortex`).
- Copiar o formato ATLAS TQ1.0 (5 trits/byte Base-3) — nosso packing é 4×2-bit/byte (ADR-0012/0085). Estudar, não fork.

---

## 8. Referências externas (fetched)

- https://huggingface.co/tiiuae/Falcon3-3B-Instruct-1.58bit (`config.json`)
- https://huggingface.co/tiiuae/Falcon3-3B-Instruct (`config.json`, ctx 32K)
- https://huggingface.co/tiiuae/Falcon3-7B-Instruct e `…-1.58bit`
- https://huggingface.co/tiiuae/Falcon3-1B-Instruct-1.58bit (existe; hidden 2048 / 18L)
- https://huggingface.co/evilsocket/Falcon3-1B-Instruct (espelho 1B denso; 18 blocks, GQA 8/4, head 256, SwiGLU, RMSNorm, vocab 131K, ctx 8K)
- https://falcon-lm.github.io/blog/falcon-edge/
- https://huggingface.co/tiiuae/Falcon-E-3B-Instruct (`config.json` ≠ Falcon3-3B)
- https://github.com/xxxn3m3s1sxxx/ATLAS-TQ1_0 (README: Falcon3-3B **7.1 tok/s** hybrid, 1B **10.1** f32 bypass, i7-7700T)
