# SESSION_499 — BitFuzzy-MoE Recurrent Engine: pipeline de 6 fases host-side + loader/forward kernel

**Sprint:** v1.9.99-s499 TEST
**Data:** 2026-10-19
**Plano:** BitFuzzy-MoE Recurrent Engine (ADR-0117) — student ternário para bare-metal
**Lanes:** (1) host-side Python (pipeline + exportador) · (2) kernel Rust
(loader `model_type=3` + forward, paridade VERIFIED **host-side**; **boot
continua recusando** o bfz; **nada treinado**)

## Premissa

Bare-metal pede pesos ternários, zero FPU na inferência e footprint < 500 MB.
O gargalo do modelo denso no nosso alvo não é a largura dos pesos: é a
**profundidade** (22 camadas × MACs por token). Daí a tese: transformar o
teacher Falcon3 1.58-bit em um **student recorrente** — um único bloco com
pesos compartilhados aplicado em K passos — com **MoE fuzzy** e ternarização
por QAT.

Três decisões fixadas **antes** de escrever código:

1. **Base canônica:** `tiiuae/Falcon3-3B-Instruct-1.58bit` — hidden 3072,
   22 layers, 12 heads, 4 kv heads, head_dim 256, intermediate 9216, vocab
   131072, rope_theta 1000042, rms_eps 1e-5, embeddings não atados.
   A linha 1.58-bit existe **só para o Falcon3** (Transformer puro);
   **Falcon-H1** (Mamba-2 híbrido) **não tem release ternário** → H1 é um
   caminho **separado e posterior**, não uma variante deste student.
2. **"1.58-bit" é o ALVO do pipeline, não a entrada.** O student começa de
   pesos **bf16** e é ternarizado por **QAT** (fuzzy STE): uma entrada
   ternária congelada **não tem caminho de gradiente** para a Fase 4.
3. **Recorrência de profundidade reduz PARÂMETROS, não memória de KV.** A
   atenção sobre o contexto continua crescendo linearmente com o contexto. O
   problema de KV (SESSION_413: KV 604 MB > modelo 544 MB) **não é resolvido
   por este pipeline**; o caminho SSM (Falcon-H1) é o candidato para esse eixo
   e merece ADR própria.

## Feito

### Fases do pipeline (host-side, Python)

| Fase | Conteúdo | Estado |
|------|----------|--------|
| 1 | Pruning angular estilo ShortGPT sobre o teacher denso | IMPLEMENTED host-side — exercitado **só** em modelo **SINTÉTICO** de 2 camadas |
| 2 | Recorrência de profundidade: K passos sobre UM bloco com pesos compartilhados + step-embedding ternário | IMPLEMENTED host-side (bug do ACT halting corrigido — a saída do passo que halta agora é SEMPRE aplicada) |
| 3 | MoE fuzzy: E experts, gating por pertinência fuzzy, T-norm, top-k, matriz de roteamento ternária | IMPLEMENTED host-side |
| 4 | Fuzzy STE + Sugeno BitLinear | IMPLEMENTED host-side |
| 5 | Destilação (KL + CE + esparsidade) + compressão de "latent thought" | **CODE ONLY** — NUNCA rodou em modelo real (BLOCKED: exige CUDA) |
| 6 | Low-rank A×B nos experts + congelamento ternário | **CODE ONLY** — nunca rodou em modelo real |

**Bug encontrado e corrigido no trainer (Fase 2):** o halting por ACT
descartava a saída do passo que haltava — o gradiente do bloco compartilhado
morria ali. Corrigido no mesmo dia.

**Defeito do ACT halting e o fix (segunda metade da sessão):**

- **Defeito:** a saída do passo que disparava o halt era **descartada** — o
  bloco compartilhado saía dos logits e **não recebia gradiente**. A
  recorrência de profundidade (Fase 2) degenerava **silenciosamente para
  K=1**.
- **Fix (semântica padrão do ACT):** a saída do passo é **sempre** aplicada
  antes da decisão de halt — o halt decide se roda **OUTRO** passo; os logits
  vêm do **último passo executado**. Surrogate straight-through para os
  parâmetros fuzzy do halting continuarem aprendendo.
- **Confidence agora relativa (sem escala):**
  `conf = 1 / (1 + RMS(delta) / RMS(h_prev))`, centro **0,8** / largura
  **0,05**, threshold **ajustável** (default **0,9**, persistido no
  checkpoint). Antes era o RMS **absoluto** do estado contra um centro fixo,
  o que degenerava para "sempre 1 passo" em modelos largos e nunca disparava
  em modelos estreitos.
- **Evidência:** gradiente **por parâmetro** do bloco (atenção/MLP) saiu de
  **0,0** (quebrado) para **1,71** (corrigido). O selftest do módulo passou de
  **11** para **15** checagens (**15/15 PASS**) e imprime relatório de halt
  (ex.: `mean_steps=3.00`, `halt_rate=0.00` em K=3).

### Ferramentas entregues (host-side, nesta sessão)

- `tools/bitfuzzy_moe_recurrent.py` — teacher Falcon3 **denso** fiel +
  student (bloco único compartilhado em K passos recorrentes, step-embedding
  ternário, `BitFuzzyMoE` com E experts + gating fuzzy + T-norm + top-k +
  roteamento ternário, `SugenoBitLinear`, fuzzy STE, pack/unpack 2-bit) com
  `--selftest` (**15/15 checagens PASS**).
- `tools/train_distill.py` — subcomandos `angular` / `collapse` / `distill` /
  `lowrank` / `report`; **gate CUDA que RECUSA** execução real sem GPU (sem
  fallback silencioso para CPU; imprime a estimativa de VRAM e sai ≠ 0);
  caminho `--synthetic` em CPU que roda a cadeia inteira; relatório **medido**
  de footprint/MACs (número de tensor real, nunca fórmula).
- `tools/export_no_std.py` — exportador bare-metal **ENTREGUE** nesta sessão
  (host-side): emite um container autocontido que **EXTENDE a família v6
  canônica** do repo (header canônico de **52 B**, `model_type=3` = BitFuzzy
  recorrente MoE) + um bloco de config de **20 B** para `model_type=3`
  (`num_experts`/`top_k`/`step_embeddings`/`head_dim`/`use_act_halting` — o
  header v6 não tem vaga) + corpo estritamente sequencial. **Reusa** os
  writers canônicos de `tools/bitnet_writer.py` (`pack_ternary`, `write_embed`,
  `write_rms`, `decode_q6k`, `compute_feat` + as constantes `MAGIC`/`VERSION`/
  `MODEL_LLM`/`EMBED_Q6K`/`EMBED_TERNARY`/`ACT_SILU`, e `write_header_v6` como
  referência de paridade) — **não**
  re-implementa packing. Emite header C (constantes de config, tabela de
  descritores de tensor com offsets de byte, tabelas de roteamento em `int16`
  **sem literal float**). `--selftest` **7/7 PASS**: paridade byte-a-byte do
  header contra o writer v6 canônico (só `model_type@14` e `feat@51` diferem),
  roundtrip exato de tensor, paridade de logits (student-fonte vs
  student-reconstruído: `max|delta|` 2,384e-07 / rel 1,799e-07), layout do
  header C monotônico e cobrindo o arquivo exato, e "tabelas de roteamento são
  inteiros". O header C gerado compila limpo em
  `gcc -fsyntax-only -Wall -Wextra -std=c99`.

### Honestidades de footprint e memória

- A meta **< 500 MB só fecha DEPOIS da Fase 6** (low-rank nos experts). Com
  blocos densos de 8 experts, **os pesos dos experts sozinhos seriam ~470 MB a
  2 bit** para um modelo de escala 3B. O ganho dominante é o **colapso de
  profundidade** (menos MACs/token), **não** a ternarização.
- O eixo de memória de KV **não é tocado** (ver Premissa 3).
- **Footprint REAL do container exportado (student SINTÉTICO):** **69.124 B**
  para **120.067 params** = **0,5757 B/param** — **não** 0,25 B/param. O
  container **NÃO é puramente 2-bit**: embed/unembed são **f32**
  (`embed_type=3`) e as tabelas de roteamento são **i16**; só `q/k/v/o`, os
  experts e a tabela de step-embedding são ternários 2-bit. As opções
  `--embed-encoding ternary|q6k` são **lossy** e o `--verify` reporta
  **FAIL** de propósito (q6k: rel 1,651e-02; ternary: rel 1,068e+02) e sai
  com código 1. **A meta < 500 MB para um student de escala 3B NÃO está
  demonstrada**: todo número acima vem de um student **SINTÉTICO** de 120 K
  params.

### Estado no kernel — lane kernel (IMPLEMENTED host-side, AINDA INUTILIZÁVEL pelo boot)

**O que existe agora (Rust, crate `cortex`):**

- `crates/cortex/src/bitfuzzy.rs` (NOVO) — loader/parser do contentor
  `model_type=3` (header v6 canônico de **52 B** + bloco de config de **20 B**
  + corpo percorrido por **UM `plan()`** — o probe e o loader não podem
  divergir) **e o forward** (loop recorrente de K passos sobre UM bloco com
  pesos compartilhados, roteamento MoE fuzzy, máscaras ternárias Sugeno,
  halting ACT). Reusa `PackedTernaryTensor` (não re-implementa packing de
  trits) e guarda as escalas **por LINHA** do contentor em
  `row_scales: Vec<f32>` à parte. Todos os buffers via
  `crate::tensor::f32_zeros` (nunca `vec![0.0; n]`).
- `crates/cortex/src/lib.rs` — +1 linha `pub mod bitfuzzy;`.
- `crates/cortex/src/model.rs` — `ModelKind::BitFuzzy = 2`,
  `ModelView::BitFuzzy(BitFuzzyModel)`, `as_bitfuzzy()`, o braço `3 =>` do
  despachante v6, e os validadores (`probe_llm_file_size`,
  `validate_bitnet_blob`, `validate_for_slot`) aprenderam `model_type=3`.
  `validate_for_slot` **RECUSA** um bfz em **TODOS** os slots (`not_llm` nos
  slots de LLM; `bitfuzzy_not_supported_in_slot` nos demais) — um bfz **não**
  é um LLM denso.
- `crates/cortex/src/model_hub.rs` — o `match view` exaustivo ganhou o braço:
  um bfz é **descrito** e **NÃO** fica registável (`register_bytes` devolve
  `false`; não fabrica um `TransformerModel`).
- `crates/cortex/src/cortex.rs` — **um** guard fail-closed (16 linhas) no
  `load_model` **legado**: blob com `version >= 6` e `data[14] != 0` é
  recusado, porque esse caminho não conhece `model_type` e leria um bfz como
  LLM denso com shapes erradas = **corrupção silenciosa**. O gate em
  `version >= 6` existe porque em v1..v5 esse byte é o campo `hidden`.

**O que isso NÃO é (honestidade obrigatória):**

- O bfz **NÃO é usável pelo boot**: `validate_for_slot` recusa em todos os
  slots e `model_hub::register_bytes` devolve `false`. O kernel ainda **não
  sabe como usar** um modelo recorrente (decisão de design em aberto).
- A prova de paridade é **HOST-side**: o forward **NUNCA rodou em bare metal**
  (sem boot QEMU, sem metal) = UNKNOWN.
- O golden é **NÃO TREINADO** (pesos aleatórios): a paridade prova
  a **MATEMÁTICA**, **não** a qualidade do modelo. Qualquer claim de
  acurácia/perplexidade = UNKNOWN.
- **Sem otimização de VRAM externa**: o loader copia os pesos para a RAM (as
  escalas por linha do contentor precisariam de ponteiros BAR próprios) =
  pendente.
- O contentor **não guarda `act_threshold`** (é parâmetro do forward, default
  **0,9**) nem **a qual módulo pertence cada máscara Sugeno** — por isso o
  forward aceita **SOMENTE** o layout canônico (uma máscara por expert = o
  gate desse expert, `in_features == hidden`) e **recusa** qualquer outro.
- Recusas fail-closed do forward (nunca silenciosas): LoRA (`num_lora > 0`),
  embed/unembed Q6_K, `rms_inner`/`rms_ffn_norm` presentes, `act_type != 0`,
  theta ausente, routing ausente, `mascaras != num_experts`, alocação
  recusada.
- O trainer **continua sem poder treinar aqui** (torch CPU-only, VRAM
  estimada 23,84 GB para a corrida 3B) — **NADA FOI TREINADO** (inalterado).

### Ferro disponível

**Sem GPU neste host**: `torch 2.13` **CPU-only**. O `estimate_vram_gb()` do
próprio trainer dá **23,84 GB** para a corrida 3B → o run real exige **CUDA
com ≥ 24 GB** (preferir ≥ 40 GB). Isso é uma **decisão de custo do
maintainer** (GPU na caixa ou GPU cloud).

## Honestidade

| Claim | Status |
|-------|--------|
| Pipeline de 6 fases implementado host-side (F1–F4) | IMPLEMENTED |
| F1 (pruning angular) exercitado em modelo real | UNKNOWN (só sintético de 2 camadas) |
| F2 (recorrência de profundidade) implementada host-side | IMPLEMENTED (bug do ACT halting corrigido: a saída do passo que halta é sempre aplicada; confidence relativa `1/(1+RMS(delta)/RMS(h_prev))`, centro 0,8 / largura 0,05, threshold 0,9 ajustável) |
| F3 (MoE fuzzy) implementada host-side | IMPLEMENTED |
| F4 (fuzzy STE + Sugeno BitLinear) implementada host-side | IMPLEMENTED (fuzzy STE = surrogado suave do STE padrão; mesma aritmética de forward) |
| F5 (destilação + latent thought) rodando em modelo real | BLOCKED (sem GPU CUDA: torch 2.13 CPU-only; VRAM estimada 23,84 GB) |
| F6 (low-rank A×B + congelamento ternário) rodando em modelo real | BLOCKED (code only, nunca executado) |
| Qualidade do student (suite/perplexidade/kl) | UNKNOWN (nada treinado) |
| Ganho de MACs/token por depth collapse | HYPOTHESIS (a medir na primeira corrida real) |
| Footprint < 500 MB | HYPOTHESIS (só fecha depois da F6; experts densos ≈ 470 MB a 2 bit) |
| Problema de KV (604 MB > 544 MB) resolvido | DEAD_WEIGHT neste pipeline (o eixo KV não é tocado: recorrência reduz parâmetros, não KV; SSM/H1 = ADR separada) |
| Loader Rust `model_type=3` (`crates/cortex/src/bitfuzzy.rs`) | IMPLEMENTED host-side (header v6 de 52 B + config de 20 B + corpo por UM `plan()`; reusa `PackedTernaryTensor`; escalas por linha em `row_scales`; buffers via `f32_zeros`) |
| Forward Rust do BitFuzzy (K passos recorrentes, MoE fuzzy, Sugeno, ACT) | IMPLEMENTED host-side (nunca executado em bare metal) |
| Paridade do forward Rust vs a referência Python | VERIFIED host-side (`max_abs 2,533197e-07`; `max_rel` estrito 8,959963e-05 dominado por 1 logit quase-zero com `\|ref\| = 1,256407e-03`; métrica significativa sobre `\|ref\| > 1e-2` = **6,333679e-06**; tolerância 1e-04 **NÃO foi aliviada**) |
| Paridade não-vaciosa (mutação) | VERIFIED host-side (1 trit de `q` → 3,151596e-06; `expert0.gate.row_scales *= 2` → 1,279962e-02; `embed[:, 11] *= 2` → 1,953993e-01) |
| Halting ACT por passo | VERIFIED host-side (mu 3,6016e-06 / 2,179066e-03 / 7,280091e-03; distância ao limiar 0,9 = 0,8999964 / 0,8978209 / 0,8927199; `steps_used = 3` = K) |
| Execução do forward em bare metal (QEMU/metal) | UNKNOWN (a prova é host-side; nunca houve boot) |
| Qualidade do modelo bfz (acurácia/perplexidade) | UNKNOWN (golden **NÃO TREINADO** — pesos aleatórios; a paridade prova a matemática, não o modelo) |
| Usabilidade do bfz pelo boot | BLOCKED (`validate_for_slot` recusa em TODOS os slots; `model_hub::register_bytes` devolve `false`; o kernel ainda não sabe consumir um modelo recorrente) |
| Guard do loader legado contra `model_type != 0` | IMPLEMENTED (`load_model` legado recusa `version >= 6` com `data[14] != 0` — sem isso leria um bfz como LLM denso com shapes erradas = corrupção silenciosa) |
| Caminho de VRAM externa (pesos + escalas por linha em BAR) | UNKNOWN / pendente (o loader copia para a RAM) |
| Recusas fail-closed do forward | IMPLEMENTED (LoRA, Q6_K embed/unembed, rms_inner/rms_ffn_norm, `act_type != 0`, theta ausente, routing ausente, `mascaras != num_experts`, alocação recusada) |
| Layout Sugeno aceito | Só o canônico (uma máscara por expert = o gate desse expert, `in_features == hidden`); o contentor não registra a que módulo cada máscara pertence |
| `act_threshold` | Não vive no contentor — é parâmetro do forward (default 0,9) |
| Exportador bare-metal (`tools/export_no_std.py`) | IMPLEMENTED host-side (entregue; `--selftest` 7/7, paridade de logits max\|delta\| 2,384e-07, header C compila em gcc) |
| Container exportado carregável pelo kernel | BLOCKED no caminho do **boot** (o loader Rust existe e parseia, mas o hub **recusa** registá-lo em qualquer slot) |
| Footprint do container (0,5757 B/param, 69.124 B / 120.067 params) | VERIFIED apenas no student SINTÉTICO; a meta < 500 MB em escala 3B é HYPOTHESIS (nada treinado, F6 pendente) |
| Container puramente 2-bit | NO (embed/unembed f32 `embed_type=3`; roteamento i16; só q/k/v/o + experts + step-emb são ternários) |
| Formato canônico de pesos | v6 = ADR-0085 (canônico); `model_type=3` é **extensão** — agora **aceita pelo loader Rust** (parse + forward), mas **ainda não é formato usável pelo boot** (hub recusa registá-lo) |
| Base do modelo (Falcon3 3B 1.58bit shapes) | OBSERVED (constantes dos conversores do repo) |

## Evidencia host

Comandos executados nesta sessão (saída real, não resumo) — lane host-side
(Python):

```
python tools/export_no_std.py --selftest
  == BitFuzzy no_std exporter self test ==
  NOTE: no kernel loader exists for model_type=3; this is a
        host-side serialisation + parity proof only.  No training.
  PASS: header_canonical_parity (model_type(14) and feat(51) excluded; 0 differing bytes)
  PASS: header_feat_low_bits (feat&0x07 == compute_feat(inner=False, ffn=False, theta=True))
  PASS: tensor_roundtrip (trits exact=True, worst f32 rel err=0.000e+00 )
  PASS: logits_parity (max|delta|=2.384e-07 rel=1.799e-07 steps src=3 rebuilt=3)
  PASS: c_header_layout (28 records, monotonic=True, covers file=True)
  PASS: manifest_covers_file (sum(sections)=23084 file=23084)
  PASS: routing_tables_are_int (no floating point literals in the fixed-point tables)
  SELFTEST PASS: 7/7 checks
  kernel support: NONE - the current cortex loader has NO model_type=3 path;
                  this container is a host-side serialisation artefact only

python tools/bitfuzzy_moe_recurrent.py --selftest
  [selftest] halt report: mean_steps=3.00 steps=[3, 3, 3, 3] halt_rate=0.00 (K=3)
  PASS: dense_forward (shape=(2, 5, 128))
  PASS: student_forward (shape=(2, 5, 128) steps_used=3)
  PASS: student_backward_finite (loss=0.027819)
  PASS: fuzzy_ste_gradient_live (a ternary weight got a non-zero grad)
  PASS: ternary_pack_roundtrip (packed_bytes=2)
  PASS: fuzzy_quantize_trits (unique=[-1.0, 0.0, 1.0])
  PASS: footprint_report (params=120522 packed=27712B fp32=482088B)
  PASS: routing_fixed_point (levels=255)
  PASS: latent_path (latents=(2, 3, 64))
  PASS: save_load_roundtrip (path=...\student.pt)
  PASS: safetensors_load (missing=[])
  PASS: block_influences_logits (max|dlogits|=3.564e-02 rel=6.564e-02)
  PASS: block_grad_with_early_halt (grad_sum=5.868e-05 steps_used=1)
  PASS: recurrence_depth_reachable (mean_steps=3.00 steps=[3, 3, 3, 3] halt_rate=0.00 (K=3))
  PASS: halt_threshold_roundtrip (threshold=0.42)
  SELFTEST PASS: 15/15 checks

python tools/train_distill.py --synthetic report --out %TEMP%\bf_docs_budget.json
  FOOTPRINT / COMPUTE BUDGET  (trained: false (synthetic))
  -- student (fresh-from-teacher (candidates=[0], sources=[1]))
  component            params         fp32        2-bit
  embed                  8.2K     32.77 KB     32.77 KB
  attention             12.4K     49.41 KB      3.09 KB
  experts               99.3K    397.31 KB     24.83 KB
  step-emb                 65     260 B        260 B
  other                   130     520 B        520 B
  TOTAL                120.1K    480.27 KB     61.47 KB
     MACs/token: 9.1K   (blocks measured: 1, tokens: 128)
  target <500MB: 2-bit projection 61.47 KB (without low-rank) / 73.76 KB (with low-rank) -> MET | fp32 480.27 KB -> MET

python tools/export_no_std.py --ckpt <student sintético> --verify
  == parity (container vs source checkpoint) ==
    max|delta logits| : 3.725e-07   rel: 6.280e-07   (tolerance 1e-03)
    recurrent steps   : source 1, rebuilt 1
    verdict           : PASS
  total bytes     : 69124 (header 52 + config 20 + body 69052)
  parameters      : 120067
  bytes / param   : 0.5757
  purely 2-bit    : NO - ... embedding/unembedding matrices are f32 ... routing
                    centres/widths are i16 fixed point ... Only the ternary
                    tensors are 2-bit.

python tools/export_no_std.py --ckpt <student sintético> --embed-encoding q6k --verify
  max|delta logits| : 9.793e-03   rel: 1.651e-02   (tolerance 1e-03)
  verdict           : FAIL      (exit 1)

python tools/export_no_std.py --ckpt <student sintético> --embed-encoding ternary --verify
  max|delta logits| : 6.333e+01   rel: 1.068e+02   (tolerance 1e-03)
  verdict           : FAIL      (exit 1)

gcc -fsyntax-only -Wall -Wextra -std=c99 <header C gerado>   -> exit 0 (sem warnings)

python tools/train_distill.py <fase> --dry-run       -> plano + estimativa de VRAM
python tools/train_distill.py ... (run real)         -> ERROR: exige CUDA GPU (nao ha fallback)
python tools/train_distill.py ... --synthetic        -> cadeia completa em CPU
```

VRAM da corrida 3B (medido com `estimate_vram_gb()` do próprio trainer sobre
`target1/falcon3/config.json` — hidden 3072, 22 layers, 12 heads, 4 kv heads,
head_dim 256, intermediate 9216, vocab 131072 — com os defaults do trainer
batch=2 / seq=128 / blocks=5): **23,84 GB**. `torch 2.13` **CPU-only** neste
host (`torch.cuda.is_available() = False`).

### Evidencia host — lane kernel (loader + forward Rust)

Comandos executados nesta lane (saída real, não resumo):

```
cargo test -p cortex --lib bitfuzzy -- --test-threads=1 --nocapture
  running 4 tests
  test bitfuzzy::tests::bfz_forward_matches_python_reference ... [BFZ] parity vs golden_bfz.ref.f32: max_abs=2.533197e-7 (idx 64) max_rel=8.959963e-5 (idx 258, ref=1.256407e-3)
  [BFZ] max_rel sobre logits com |ref|>1e-2 = 6.333679e-6 (min |ref| = 5.656336e-4)
  [BFZ] passo 0: conf=0.173292860 mu=0.000003602 |mu-0.9|=0.899996400
  [BFZ] passo 1: conf=0.493666142 mu=0.002179066 |mu-0.9|=0.897820890
  [BFZ] passo 2: conf=0.554234743 mu=0.007280091 |mu-0.9|=0.892719865
  ok
  test bitfuzzy::tests::bfz_forward_parity_check_is_live ... [BFZ] mutacao q[0][0]: max_abs_delta=3.151596e-6
  [BFZ] mutacao q.row_scales[0] *= 64: max_abs_delta=5.748682e-4
  [BFZ] mutacao expert0.gate.row_scales[*] *= 2: max_abs_delta=1.279962e-2
  [BFZ] mutacao embed[:, 11] *= 2: max_abs_delta=1.953993e-1
  ok
  test bitfuzzy::tests::bfz_golden_parses ... ok
  test bitfuzzy::tests::bfz_negative_rejects ... ok
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 166 filtered out; finished in 0.02s

cargo check --release -p cortex
  warning: `cortex` (lib) generated 4 warnings        <- pre-existentes (traits.rs, trinity.rs, cortex.rs, moe.rs)
  Finished `release` profile [optimized] target(s)   <- 0 erros; NENHUM warning em bitfuzzy.rs

cargo test -p cortex --lib -- --test-threads=1
  test trinity::tests::router_encode_and_decision_match_python_reference ... FAILED
    assertion `left == right` failed: rota NEURAL (nao keyword) esperada p/ "aumenta o volume"
    left: 0   right: 1        (crates\cortex\src\trinity.rs:1701)
  test result: FAILED. 169 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.56s
  -> PRE-EXISTENTE: trinity.rs NAO foi tocado nesta lane (git status: fora da lista de modificados)

cargo build --release
  warning: `neural-kernel` (bin "neural-kernel") generated 51 warnings
  warning: boot@1.9.9: Limine+logwriter ESP: ...\target\limine-esp.img (128 MB)
  Finished `release` profile [optimized] target(s)   <- 0 erros no membro BARE-METAL (cortex e linkado nele)

Get-FileHash -Algorithm SHA256 tools\golden_bfz.bin, tools\golden_bfz.ref.f32
  tools\golden_bfz.bin       : 963E67EC647392B2D00D3FEB059729BB733DA5B6F22DB2E92E91524D633C5A51
  tools\golden_bfz.ref.f32   : 39FC9091E8D83E335A532D8BF3E2D113A25EC9D9D164D41DFB53602E4738043F
```

**Como ler a métrica de paridade (honestidade):** a tolerância **NÃO foi
aliviada** para fazer o teste passar. `max_abs = 2,533197e-07` e
`max_rel = 8,959963e-05` (tolerância 1e-04) — mas o `max_rel` estrito é
**dominado por UM logit de referência quase zero** (`|ref| = 1,256407e-03`, no
piso de ruído f32). A métrica **significativa** (erro relativo só sobre
logits com `|ref| > 1e-2`) é **6,333679e-06**. As duas têm de ser citadas
juntas: 8,96e-05 **não** significa "8,96e-05 de erro de modelo".

**A paridade é NÃO-VACUOSA (mutação):** virar **um** trit de `q` move os
logits em apenas **3,151596e-06** (a atenção contribui ~1% do residual stream
— atenuação estrutural, não bug) — por isso o teste também aplica mutações
**materiais**: `expert0.gate.row_scales[*] *= 2` → **1,279962e-02** e
`embed[:, 11] *= 2` → **1,953993e-01**. A referência Python foi
**cross-validada de forma independente pela lane kernel** (mirror numpy lendo só o contentor:
`max_abs 3,58e-07`, deltas de mutação reproduzidos exatamente) — número
**herdado da lane**, não re-executado por esta frente de documentação.

**ACT halting por passo** (limiar 0,9): `mu` = 3,6016e-06 / 2,179066e-03 /
7,280091e-03; distância ao limiar = 0,8999964 / 0,8978209 / 0,8927199 →
**sem risco de inversão de halt**; `steps_used = 3` = K.

> **Nota sobre a saída do exportador:** as linhas
> `NOTE: no kernel loader exists for model_type=3` e `kernel support: NONE`
> que aparecem no bloco acima são o **texto do próprio exportador**
> (`tools/export_no_std.py`, não modificado nesta lane) e estão
> **DESATUALIZADAS** desde a lane kernel: o loader Rust passou a existir. A
> saída foi mantida **verbatim** (é output real) — a correção é do
> exportador, não deste registo.

Resultados da **lane de código** (as três ferramentas foram entregues na
sessão original; a lane kernel entregou o loader + forward Rust); esta frente
de **documentação/registro** re-executou os selftests e os gates acima. Nada
acima é evidência de **propriedade** do student: é evidência de **código
executável** e de **paridade da matemática** (ADR-0113: IMPLEMENTED ≠ DONE).

## Proximo

1. **Decisão do maintainer — onde o treino real roda:** GPU na caixa (upgrade)
   ou GPU cloud (orçamento). Requisito: CUDA ≥ 24 GB (preferir ≥ 40 GB).
2. **Decisão do maintainer — formato de saída:** (a) container família v6 com
   `model_type=3` (exige **loader Rust novo** + extensão do container de
   formato) ou (b) **engine C externo** (o exportador já produz header C),
   acionado como skill/WASM. A escolha muda o custo de kernel de "um loader"
   para "um loader + uma forma de container".
3. **Decisão do maintainer — orçamento de perda de qualidade** aceitável na
   ternarização sem calibração QAT (qual degradação de suite/perplexidade
   aborta a frente). Sem esse número, "distilou bem" não é verificável.
4. **Fase 5 → Fase 6** só depois de (1): rodar a F5 real, medir a F6 sobre o
   student treinado, e só então reportar footprint e MACs.
5. **Integração hub/boot** — o loader Rust e o forward **existem** (lane
   kernel), mas o bfz **não é usável pelo boot**: `validate_for_slot` recusa
   em todos os slots e `model_hub::register_bytes` devolve `false`. Falta
   decidir **como o boot consome um modelo recorrente** em vez de um
   `TransformerModel` (que tipo de slot, que agente, que política de
   inferência) — é decisão de design, não de parsing.
6. **Modelo treinado + medição de qualidade** — hoje o golden tem **pesos
   aleatórios**; a paridade prova a matemática, não o modelo. Depende de (1).
7. **Validação em QEMU/metal do forward bfz** — a prova é **host-side**;
   o forward **nunca rodou em bare metal**.
8. **Caminho de VRAM externa** — o loader copia os pesos para a RAM; as
   escalas por linha do contentor precisariam de ponteiros BAR próprios
   (relevante para o contentor de escala 3B).
9. **Eixo de memória de KV:** frente separada (SSM/Falcon-H1), ADR própria —
   não entra neste pipeline.

## Licoes

- **"1.58-bit" é alvo, não entrada:** um checkpoint ternário congelado não
  tem gradiente — QAT (fuzzy STE) é o que torna a Fase 4 possível.
- **Recorrência ≠ memória:** depth recurrence compra **parâmetros/MACs**, não
  KV; quem promete os dois está vendendo duas ADRs como uma.
- **Gate que recusa é honestidade executável:** o trainer **não** cai para
  CPU em run real — imprime a estimativa de VRAM e sai ≠ 0. Um fallback
  silencioso teria produzido "student treinado" falso.
- **Bug de ACT halting esconde gradiente:** o halting descartava a saída do
  passo halting — sintoma (bloco não aprende) apontava para o bloco, causa
  era o laço de halting. Lendo o caminho inteiro do estado (e não só a
  aritmética) achou.
- **ACT: o halt decide se roda OUTRO passo — a saída do passo halting é
  sempre aplicada.** Descartá-la tira o bloco compartilhado dos logits e
  **mata o gradiente dele**: a recorrência de profundidade vira um bloco
  único sem ninguém avisar. O sintoma (profundidade que não acontece) e a
  causa (semântica do halt) ficam em camadas diferentes — ler a semântica
  do halt, não só a aritmética do laço.
- **Confidence de halting precisa ser RELATIVA, não absoluta.** O RMS
  absoluto do estado contra um centro fixo satura em modelo largo ("sempre
  1 passo") e nunca dispara em modelo estreito. O tamanho **relativo** do
  passo (`1/(1+RMS(delta)/RMS(h_prev))`) é sem escala e os mesmos defaults
  valem para qualquer largura.
- **Métrica agregada não prova caminho vivo:** o `grad_block` do trainer
  **soma** `|grad|` de todo parâmetro cujo nome começa com `"block"` — e
  inclui os parâmetros do **roteador MoE**, que recebem gradiente da
  penalidade de esparsidade **independentemente da recorrência**. Logo
  `grad_block > 0` **não** prova que o caminho recorrente está vivo: a prova
  é o **breakdown por parâmetro** (0,0 → 1,71 no bloco).
- **Footprint é resultado de fase, não de promessa:** o número de < 500 MB
  depende da F6; anotar a dependência no próprio ADR evita vender a meta
  antes do stage que a fecha.
- **Ausência de loader é estado, não bug:** `model_type=3` sem loader no
  kernel era o estado correto daquele dia — o que seria errado é declarar
  "pronto para bare-metal". **Atualizado pela lane kernel:** o loader e o
  forward **existem** (IMPLEMENTED host-side, paridade VERIFIED host-side),
  mas o bfz continua **não usável pelo boot** (o hub recusa registá-lo em
  qualquer slot) e **nunca rodou em bare metal**. O que seria errado agora
  é dizer "roda no kernel" ou "no metal".
- **Um tipo de modelo novo precisa de um guard explícito em TODOS os pontos
  de entrada, não só no novo:** o `load_model` **legado** (v1..v5) não
  conhece `model_type` — sem o guard de 16 linhas, ele leria um bfz como um
  **LLM denso** (`hidden = data[14]`, `num_layers = 0`) e tentaria
  interpretá-lo com shapes erradas = **corrupção silenciosa** (nem erro, nem
  log, só resultado errado). O guard fica em `version >= 6` porque em v1..v5
  esse byte é o campo `hidden`. Regra: quando se acrescenta um discriminante
  novo ao header, **auditar cada leitor existente** desse header — o leitor
  novo é o óbvio, os velhos são os que corrompem em silêncio.
- **A métrica de paridade tem de ser lida com o seu piso:** `max_rel =
  8,96e-05` (tolerância 1e-04) **parece** folgado, mas é **dominado por UM
  logit de referência quase zero** (`|ref| = 1,256407e-03`, no piso de ruído
  f32) — o erro relativo de um valor quase zero é grande **por construção**.
  A métrica que significa alguma coisa é o erro relativo **só sobre logits
  com `|ref| > 1e-2`** = **6,33e-06**. Reportar **as duas** (a estrita e a
  significativa) e dizer qual delas é dominada pelo piso; a tolerância
  **NÃO foi aliviada** para fazer o teste passar.

## Arquivos tocados nesta frente

Lane host-side (original):

- `tools/bitfuzzy_moe_recurrent.py` (novo)
- `tools/train_distill.py` (novo)
- `tools/export_no_std.py` (novo — entregue, host-side)

Lane kernel (loader + forward Rust):

- `crates/cortex/src/bitfuzzy.rs` (novo — loader/parser `model_type=3` +
  forward recorrente)
- `crates/cortex/src/lib.rs` (+1 linha `pub mod bitfuzzy;`)
- `crates/cortex/src/model.rs` (`ModelKind::BitFuzzy`, `ModelView::BitFuzzy`,
  `as_bitfuzzy()`, braço `3 =>` do despachante v6, validadores)
- `crates/cortex/src/model_hub.rs` (braço do `match view` — bfz descrito e
  **não** registável)
- `crates/cortex/src/cortex.rs` (guard fail-closed no `load_model` legado)

Registo (docs):

- `docs/architecture/0117-bitfuzzy-moe-recurrent-engine.md` (novo)
- `docs/architecture/INDEX.md`, `docs/memory/IDEA_BANK.md`, `docs/memory/STATE.md`,
  `docs/memory/SESSION_INDEX.md`, `TODO.md`, `TECNOLOGIAS.md` (registro)
