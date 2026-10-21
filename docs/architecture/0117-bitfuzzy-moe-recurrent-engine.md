# ADR-0117: BitFuzzy-MoE Recurrent Engine — student ternário recorrente para execução bare-metal

- **Status:** Proposed
- **Lifecycle:** `fazendo` (a partir de 2026-10-19)
- **Revisão (s499, lane kernel):** o texto original afirmava "**NADA no
  kernel**"; essa afirmação **era verdadeira** quando escrita e passou a ser
  **falsa** depois da lane kernel (loader + forward Rust com paridade
  VERIFIED host-side). A seção "Estado no kernel" foi reescrita com o estado
  **medido**, mantendo todas as ressalvas de honestidade: o bfz **não** é
  usável pelo boot, o forward **nunca rodou em bare metal**, e o modelo
  **não** está treinado.
- **Origem:** sessão s499 — pipeline de 6 fases host-side (teacher Falcon3 denso →
  student "BitFuzzy-MoE" recorrente); IDEIA **#706**
- **Escopo:** arquitetura do student (profundidade recorrente + MoE fuzzy +
  ternarização QAT) e o pipeline host-side que o treina/exporta. Formato
  canônico de pesos: **ADR-0085** (v6). Governança/honestidade: **ADR-0113**
  (taxonomia OBSERVED/VERIFIED/IMPLEMENTED/HYPOTHESIS/UNKNOWN/BLOCKED/
  DEAD_WEIGHT + cadeia claim→evidência). Premissa do projeto: **ADR-0088**.

## Contexto

O alvo bare-metal pede três coisas que o modelo atual não dá: pesos
**ternários**, **zero FPU na inferência** e **footprint < 500 MB**. O
caminho escolhido aqui não é re-embaralhar um `.v6` existente: é **transformar
um teacher Falcon3 1.58-bit em um student recorrente**, porque o gargalo do
modelo denso no nosso alvo é a **profundidade** (22 camadas × MACs por token),
não a largura dos pesos.

Três fatos delimitam o desenho e foram fixados antes de escrever qualquer
código:

1. **A linha 1.58-bit existe só para o Falcon3** (Transformer puro). O
   **Falcon-H1** (híbrido Mamba-2) **não tem release ternário** — portanto H1
   é um caminho **separado e posterior**, não uma variante deste student.
2. **"1.58-bit" é o ALVO do pipeline, não a sua entrada.** O student começa
   de pesos **bf16** e é ternarizado por **QAT** (fuzzy STE): uma entrada
   ternária congelada **não tem caminho de gradiente** para a Fase 4.
3. **Recorrência de profundidade reduz PARÂMETROS, não memória de KV.** A
   atenção sobre o contexto continua crescendo linearmente com o contexto. O
   problema de KV (SESSION_413: KV 604 MB > modelo 544 MB) **não é resolvido
   por este pipeline**; o caminho SSM (Falcon-H1) é o candidato para esse eixo
   e merece ADR própria.

Base canônica: `tiiuae/Falcon3-3B-Instruct-1.58bit` — hidden 3072, 22 layers,
12 heads, 4 kv heads, head_dim 256, intermediate 9216, vocab 131072,
rope_theta 1000042, rms_eps 1e-5, embeddings **não atados** (untied).

## Decisão

Adotar o **BitFuzzy-MoE Recurrent Engine**: um student que aplica **um único
bloco com pesos compartilhados** em K passos recorrentes, com **MoE fuzzy**
(com E experts, gating por pertinência fuzzy + T-norm + top-k + matriz de
roteamento ternária), **BitLinear Sugeno** e **fuzzy STE** (surrogado suave
`tanh` do STE padrão — **mesma aritmética de forward**), empacotado a
**2 bit**.

O pipeline tem 6 fases, executadas **host-side** (Python/torch):

| Fase | Conteúdo | Estado real (2026-10-19) |
|------|----------|---------------------------|
| 1 | Pruning angular estilo ShortGPT sobre o teacher denso | IMPLEMENTED host-side — exercitado **só** em modelo SINTÉTICO de 2 camadas |
| 2 | Recorrência de profundidade: K passos sobre UM bloco com pesos compartilhados + tabela de step-embedding ternária | IMPLEMENTED host-side (bug do ACT halting corrigido — ver "Licoes") |
| 3 | MoE fuzzy: E experts, gating por pertinência fuzzy, T-norm, top-k, roteamento ternário | IMPLEMENTED host-side |
| 4 | Fuzzy STE + Sugeno BitLinear | IMPLEMENTED host-side |
| 5 | Destilação (KL + CE + esparsidade) + compressão de "latent thought" | **CODE ONLY** — NUNCA rodou em modelo real (exige CUDA; VRAM estimada 23,84 GB; **BLOCKED** neste host) |
| 6 | Low-rank A×B nos experts + congelamento ternário | **CODE ONLY** — nunca rodou em modelo real |

**Entregas desta sessão (host-side, Python):**

- `tools/bitfuzzy_moe_recurrent.py` — teacher Falcon3 **denso** (fiel) + student
  (bloco único compartilhado em K passos recorrentes, step-embedding ternário,
  `BitFuzzyMoE` com E experts, `SugenoBitLinear`, fuzzy STE, pack/unpack
  2-bit, `--selftest` com **15/15 checagens PASS**).
- `tools/train_distill.py` — subcomandos `angular` / `collapse` / `distill` /
  `lowrank` / `report`; **gate CUDA que RECUSA** execução real sem GPU (sem
  fallback silencioso para CPU); caminho `--synthetic` em CPU que roda a
  cadeia inteira; relatório medido de footprint/MACs.
- `tools/export_no_std.py` — exportador bare-metal **ENTREGUE** nesta sessão
  (host-side, Python): emite um container autocontido que **EXTENDE a família
  v6 canônica** do repo (header canônico de **52 B**, `model_type=3` =
  BitFuzzy recorrente MoE) + um bloco de config de **20 B** para
  `model_type=3` (`num_experts`/`top_k`/`step_embeddings`/`head_dim`/
  `use_act_halting` — o header v6 não tem vaga para eles) + corpo estritamente
  sequencial. **Reusa** os writers canônicos de `tools/bitnet_writer.py`
  (`pack_ternary`, `write_embed`, `write_rms`, `decode_q6k`, `compute_feat` +
  as constantes `MAGIC`/`VERSION`/`MODEL_LLM`/`EMBED_Q6K`/`EMBED_TERNARY`/
  `ACT_SILU`, e `write_header_v6` como referência de paridade) — **não**
  re-implementa packing. Emite header C (constantes
  de config, tabela de descritores de tensor com offsets de byte, tabelas de
  roteamento em `int16` **sem literal float**). `--selftest` **7/7 PASS**:
  paridade byte-a-byte do header contra o writer v6 canônico (só `model_type@14`
  e `feat@51` diferem), roundtrip exato de tensor, paridade de logits
  (student-fonte vs student-reconstruído do container: `max|delta|` 2,384e-07 /
  rel 1,799e-07), layout do header C monotônico e cobrindo o arquivo exato, e
  "tabelas de roteamento são inteiros". O header C gerado compila limpo em
  `gcc -fsyntax-only -Wall -Wextra -std=c99`.

**Honestidade de footprint:** a meta de < 500 MB **só fecha DEPOIS** da Fase 6
(low-rank nos experts). Com blocos densos de 8 experts, **os pesos dos experts
sozinhos seriam ~470 MB a 2 bit** para um modelo de escala 3B. O ganho
dominante é o **colapso de profundidade** (menos MACs/token), **não** a
ternarização.

**Footprint REAL medido (exportador, student SINTÉTICO):** container de
**69.124 B** para **120.067 params** = **0,5757 B/param** — **não** 0,25
B/param. O container **NÃO é puramente 2-bit**: as matrizes de embed/unembed
são **f32** (`embed_type=3`) e as tabelas de roteamento são **i16**; só
`q/k/v/o`, os experts e a tabela de step-embedding são ternários 2-bit. As
opções `--embed-encoding ternary|q6k` são **lossy** e o `--verify` reporta
**FAIL** de propósito (q6k: rel 1,651e-02; ternary: rel 1,068e+02) e sai com
código 1. A meta **< 500 MB para um student de escala 3B NÃO está
demonstrada**: todo número acima vem de um student **SINTÉTICO** de 120 K
params.

**Estado no kernel: IMPLEMENTED host-side (loader + forward) — o bfz ainda NÃO
é usável pelo boot.** (Atualizado nesta revisão: a afirmação anterior
"NADA no kernel" era verdadeira quando escrita e passou a ser falsa.)

- `crates/cortex/src/bitfuzzy.rs` (NOVO) — loader/parser do contentor
  `model_type=3` (header v6 canônico de **52 B** + bloco de config de **20 B**
  + corpo percorrido por **UM `plan()`**, para que probe e loader não possam
  divergir) **e o forward** (loop recorrente de K passos sobre UM bloco com
  pesos compartilhados, roteamento MoE fuzzy, máscaras ternárias Sugeno,
  halting ACT). Reusa `PackedTernaryTensor` (não re-implementa o packing de
  trits) e mantém as escalas **por linha** do contentor em
  `row_scales: Vec<f32>` à parte. Todos os buffers via
  `crate::tensor::f32_zeros` (nunca `vec![0.0; n]`).
- `crates/cortex/src/model.rs` — `ModelKind::BitFuzzy = 2`,
  `ModelView::BitFuzzy(BitFuzzyModel)`, `as_bitfuzzy()`, o braço `3 =>` do
  despachante v6, e os validadores (`probe_llm_file_size`,
  `validate_bitnet_blob`, `validate_for_slot`) aprenderam `model_type=3`.
  `validate_for_slot` **RECUSA** um bfz em **TODOS** os slots (`not_llm` nos
  slots de LLM; `bitfuzzy_not_supported_in_slot` nos demais): um bfz **não**
  é um LLM denso.
- `crates/cortex/src/model_hub.rs` — o `match view` exaustivo ganhou o braço:
  um bfz é **descrito** e **NÃO** fica registável (`register_bytes` devolve
  `false`; não fabrica um `TransformerModel`).
- `crates/cortex/src/cortex.rs` — **um** guard fail-closed (16 linhas) no
  `load_model` **legado**: blob com `version >= 6` e `data[14] != 0` é
  recusado (esse caminho não conhece `model_type` e leria um bfz como LLM
  denso com shapes erradas = **corrupção silenciosa**). O gate em
  `version >= 6` existe porque em v1..v5 esse byte é o campo `hidden`.

**Paridade (o gate de aceitação) — medida, host-side:** o forward Rust, lendo
**só** `tools/golden_bfz.bin`, reproduz os logits de referência de
`tools/golden_bfz.ref.f32` para os ids de entrada `[11, 5, 59, 38, 21]`:
`max_abs = 2,533197e-07`, `max_rel = 8,959963e-05` (tolerância 1e-04 — **NÃO
foi aliviada**). O `max_rel` estrito é **dominado por UM logit de referência
quase zero** (`|ref| = 1,256407e-03`, no piso de ruído f32); a métrica
**significativa** (erro relativo só sobre logits com `|ref| > 1e-2`) é
**6,333679e-06**. Halting ACT por passo: `mu` = 3,6016e-06 / 2,179066e-03 /
7,280091e-03, distância ao limiar 0,9 = 0,8999964 / 0,8978209 / 0,8927199
(sem risco de inversão de halt), `steps_used = 3` = K. A paridade é
**não-vacuosa**: virar um trit de `q` move os logits em 3,151596e-06 (a
atenção contribui ~1% do residual stream), por isso o teste também aplica
mutações **materiais** (`expert0.gate.row_scales *= 2` → 1,279962e-02;
`embed[:, 11] *= 2` → 1,953993e-01). A referência Python foi
**cross-validada de forma independente pela lane kernel** (mirror numpy lendo
só o contentor: `max_abs 3,58e-07`, deltas de mutação reproduzidos
exatamente) — número **herdado da lane**, não re-executado por esta frente
de documentação.

**O que continua a NÃO ser verdade (honestidade, ADR-0113):**

- O bfz **NÃO é usável pelo boot**: `validate_for_slot` recusa em todos os
  slots e `model_hub::register_bytes` devolve `false`. O kernel ainda **não
  sabe como usar** um modelo recorrente — isso é uma **decisão de design em
  aberto**, não uma falha de parsing.
- A prova é **HOST-side**: o forward **nunca rodou em bare metal** (sem boot
  QEMU, sem metal).
- O golden é **NÃO TREINADO** (pesos aleatórios): a paridade prova a
  **MATEMÁTICA**, **não** a qualidade do modelo. Qualquer claim de
  acurácia/perplexidade é **UNKNOWN**.
- **Sem otimização de VRAM externa**: o loader copia os pesos para a RAM (as
  escalas por linha do contentor precisariam de ponteiros BAR próprios).
- O contentor **não guarda `act_threshold`** (é parâmetro do forward, default
  **0,9**) nem **a qual módulo pertence cada máscara Sugeno** — o forward
  aceita **SOMENTE** o layout canônico (uma máscara por expert = o gate
  desse expert, `in_features == hidden`) e **recusa** qualquer outro.
- Recusas fail-closed do forward (nunca silenciosas): LoRA (`num_lora > 0`),
  embed/unembed Q6_K, `rms_inner`/`rms_ffn_norm` presentes, `act_type != 0`,
  theta ausente, routing ausente, `mascaras != num_experts`, alocação
  recusada.
- O trainer **continua sem poder treinar aqui** (torch CPU-only, VRAM
  estimada 23,84 GB para a corrida 3B) — **NADA FOI TREINADO** (inalterado).

## Consequências

**Ganhos esperados (a medir, não a declarar):** menos MACs por token (colapso de
profundidade), pesos ternários (ADD/SUB/SKIP no bare-metal), footprint
compatível com a janela de heap, e um student **distilado** de um teacher que
já conhecemos (rig da ADR-0116).

**Custos/riscos:** QAT fuzzy é novo e não calibrado (a ternarização atual é
absmean sem calibração); a destilação em GPU exige orçamento que este host não
tem; o MoE fuzzy adiciona um gating **não-linear** no caminho quente (a lição
SESSION_331 proíbe custo desnecessário no hot path — o gating precisa de forma
inteira/lookup); o **loader Rust existe** (host-side, `crates/cortex/src/
bitfuzzy.rs`) mas **o kernel que o consome ainda não existe**: o hub recusa
registar um bfz em qualquer slot, e o **boot ainda não sabe** o que fazer com
um modelo recorrente (que tipo de slot, que agente, que política de
inferência).

**Trabalho restante (explícito, para não vender a frente como fechada):**

1. **Integração hub/boot** — como o boot consome um modelo **recorrente** em
   vez de um `TransformerModel` (tipo de slot, agente, política de inferência,
   e o que `ModelSlot`/`register_bytes` passam a aceitar). Hoje:
   `validate_for_slot` recusa em todos os slots e `register_bytes` devolve
   `false` — decisão de design, não de parsing.
2. **Modelo treinado + medição de qualidade** — o golden atual tem **pesos
   aleatórios**; falta a corrida real em GPU (F5/F6) e a medição de
   suite/perplexidade/KL sobre o student treinado.
3. **Validação em QEMU/metal do forward bfz** — a paridade é **host-side**; o
   forward **nunca rodou em bare metal**.
4. **Caminho de VRAM externa** — hoje o loader copia os pesos para a RAM; o
   contentor de escala 3B precisaria de ponteiros BAR próprios (pesos +
   escalas por linha), alinhado com a ADR-0112 (BAR Compute).

**Decisões abertas (precisam do maintainer):**

1. **Onde o treino real roda** — GPU host próprio (upgrade da caixa) vs GPU
   cloud (orçamento). Requisito: **CUDA com ≥ 24 GB** (melhor ≥ 40 GB),
   porque `estimate_vram_gb()` do próprio trainer dá **23,84 GB** para a
   corrida 3B. Hoje: `torch 2.13` **CPU-only**, sem GPU.
2. **Formato de saída do student** — (a) container **família v6 com
   `model_type=3`**, que **já tem loader Rust** (host-side, IMPLEMENTED:
   parse + forward com paridade VERIFIED contra a referência Python) e está
   à espera da **integração hub/boot**; ou (b) **engine C externo** (o
   exportador `tools/export_no_std.py` já produz header C), acionado como
   skill/WASM. A escolha muda o custo de kernel de "um loader" para "um
   loader + uma forma de container" **mais** a forma de o boot consumir um
   modelo recorrente.
3. **Orçamento de perda de qualidade** aceitável na ternarização sem
   calibração QAT — qual degradação de suite/perplexidade o maintainer aceita
   antes de considerar a frente abortada. Sem esse número, "distilou bem" não é
   verificável.

**Regra de honestidade herdada (ADR-0113):** código implementado ≠ propriedade
demonstrada. Enquanto não houver corrida real em GPU, esta ADR descreve um
**pipeline host-side IMPLEMENTED**, com Fases 5 e 6 em **BLOCKED**; nenhum
número de qualidade, tok/s ou perplexidade pode ser citado antes da
primeira corrida medida. Do mesmo modo, o **loader + forward Rust** é
**IMPLEMENTED** com paridade **VERIFIED host-side**: isso **não** é
"roda no kernel" (o boot recusa o bfz) nem "roda no metal" (nunca houve
boot), nem "o modelo é bom" (pesos aleatórios).

## Licoes

- **ACT halting: a saída do passo que halta NÃO pode ser descartada.** O
  halting por ACT decide se roda **OUTRO** passo; o estado/logits vêm do
  **último passo executado**. Na implementação quebrada, a saída do passo
  que disparava o halt era descartada: o bloco compartilhado saía dos logits
  e **não recebia gradiente** — a Fase 2 degenerava silenciosamente para
  K=1 (a "recorrência de profundidade" virava um bloco só). Fix: a saída do
  passo é **sempre** aplicada antes da decisão de halt, com surrogate
  straight-through para os parâmetros fuzzy do halting continuarem
  aprendendo. Evidência: gradiente por parâmetro do bloco (atenção/MLP) saiu
  de **0,0** (quebrado) para **1,71** (corrigido).
- **A confidence do halting tem que ser RELATIVA.** A versão anterior era o
  RMS **absoluto** do estado contra um centro fixo: isso satura em modelos
  largos (vira "sempre 1 passo") e nunca dispara em modelos estreitos. Agora
  é o **tamanho relativo do passo**, sem escala:
  `conf = 1 / (1 + RMS(delta) / RMS(h_prev))`, centro **0,8** / largura
  **0,05**, com **threshold ajustável** (default **0,9**, persistido no
  checkpoint). O selftest do módulo passou de **11** para **15** checagens
  (**15/15 PASS**) e imprime o relatório de halt (ex.: `mean_steps=3.00`,
  `halt_rate=0.00` em K=3).
- **Métrica agregada não prova o caminho vivo.** O `grad_block` do trainer
  **soma** `|grad|` de todo parâmetro cujo nome começa com `"block"` — e isso
  **inclui os parâmetros do roteador MoE**, que recebem gradiente da
  penalidade de esparsidade **independentemente da recorrência**. Ou seja:
  `grad_block > 0` **não** prova que o caminho recorrente está vivo. A prova
  válida é o **breakdown por parâmetro**.
- **Um tipo de modelo novo precisa de um guard explícito em TODOS os pontos
  de entrada, não só no novo.** O `load_model` **legado** (v1..v5) não
  conhece `model_type`: sem o guard de 16 linhas, ele leria um bfz como um
  **LLM denso** (`hidden = data[14]`, `num_layers = 0`) e tentaria
  interpretá-lo com shapes erradas = **corrupção silenciosa** (sem erro, sem
  log, só resultado errado). O guard fica em `version >= 6` porque em v1..v5
  esse byte é o campo `hidden`. Regra: ao acrescentar um discriminante novo
  ao header, **auditar cada leitor existente** desse header.
- **A métrica de paridade tem de ser lida com o seu piso.** `max_rel =
  8,96e-05` (tolerância 1e-04) **parece** folgado, mas é **dominado por UM
  logit de referência quase zero** (`|ref| = 1,256407e-03`, no piso de ruído
  f32) — o erro relativo de um valor quase zero é grande **por construção**.
  A métrica que significa algo é o erro relativo **só sobre logits com
  `|ref| > 1e-2`** = **6,33e-06**. Reportar **as duas** (a estrita e a
  significativa) e dizer qual é dominada pelo piso; a tolerância **NÃO foi
  aliviada** para fazer o teste passar.
- **Paridade que não se pode falsear não é paridade.** Um trit virado em `q`
  move os logits em 3,15e-06 (a atenção contribui ~1% do residual stream) —
  um teste que só mede isso passaria com o attention errado. Por isso o gate
  aplica **mutações materiais** (escala do gate de um expert, coluna do
  embedding) e espera deltas **visíveis**.

## Referências

- ADR-0085 — formato v6 canônico (pesos, header, `model_type`).
- ADR-0113 — missão de otimização sistêmica (taxonomia + cadeia de evidência).
- ADR-0088 — premissa máxima AIOS-First.
- ADR-0116 — rig de validação `.v6` (o gate que o student teria de passar).
- SESSION_499 — evidência da sessão (host-side, sintético + lane kernel).
- `tools/bitfuzzy_moe_recurrent.py`, `tools/train_distill.py`,
  `tools/export_no_std.py` (os três entregues, host-side).
- `crates/cortex/src/bitfuzzy.rs` (loader + forward `model_type=3`, lane
  kernel), `crates/cortex/src/model.rs`, `crates/cortex/src/model_hub.rs`,
  `crates/cortex/src/cortex.rs` (guard do loader legado).
