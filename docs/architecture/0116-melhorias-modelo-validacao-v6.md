# ADR-0116: Validação de melhorias no modelo (.v6) — rig host + diário de vereditos

- **Status:** Accepted (processo contínuo — documento vivo de vereditos)
- **Lifecycle:** `fazendo` (a partir de 2026-10-13)
- **Origem:** sessão de validação .v6 (out/2026): conversor Falcon3→v6 refeito,
  suite de aceite Q1/Q2/Q3, paridade contra `transformers`, benchmark fonte-vs-v6
- **Escopo:** qualidade de geração dos modelos `.v6` (1B/3B/7B/10B) e do pipeline
  que os produz (conversor, formato, forward, decode). Governança: ADR-0113
  (taxonomia OBSERVED/VERIFIED/IMPLEMENTED/HYPOTHESIS/UNKNOWN/BLOCKED +
  ficha anti-complexidade); formato canônico: ADR-0085; lab cognitivo: ADR-0101.

## Contexto

Arquivos `.v6` convertidos geravam loops degenerados (`Ireland>>TITLE<<`,
`the un`, `and`-loops) sem que fosse claro se a culpa era do artefato, do
forward, do tokenizer ou do modelo. Esta ADR fixa o **rig de validação** e
registra o **baseline medido**; cada estudo de melhoria proposto entra abaixo
como item E-NN com veredito (nada entra no kernel sem passar pelo rig).

## Decisão: o rig (tools/, host Windows, sem QEMU, sem Rust)

- `tools/chat_v6.py` — parse `.v6` byte-exato + forward torch-CPU + chat
  (`--model`, `--ask`, `--temp/--top-p/--seed`; greedy default).
- `tools/test_v6_suite.py` — gate de aceite, exit 0 sse Q1+Q2+Q3 verdes:
  Q1 nome→linguagem natural; Q2 `1+1=`→contém `2` (+ stop); Q3 JSON
  (fence-stripped, contém `4`/`quatro`). Heurísticas = port de
  `bpe::gibberish_stop` (distinct-2, 4-gram, peça-média; peça off p/ JSON).
- `tools/diag_v6.py` — H1 stats (scales/zeros/RMS revelam conversão quebrada:
  scale 1e-6 + 100% zeros = chave ausente; `bytes_lidos==tamanho` = layout ok)
  + H3 entropia/top-10 + H4 trace de magnitudes.
- `tools/parity_*.py` — paridade hidden-state/logits por layer e por op
  contra `transformers` (mesmos ids); métrica canônica = **cosseno + RMS**
  (rel-err com piso mente quando a referência tem elementos ~zero).
- `tools/ref_transformers.py` — árbitro executável (fonte + template TII,
  greedy); exige `TORCHDYNAMO_DISABLE=1` (sem `cl.exe` o inductor falha).
- `tools/bench_v6.py` — fonte-vs-v6 lado a lado (TTFT, tok/s, textos, vereditos).
- Regras: mesmos ids nos dois lados; commit semântico pequeno por vez;
  cada fix precisa de antes/depois medido no mesmo prompt.

## Baseline medido (VERIFIED, 2026-10-13)

- **1B** `target1/FALCON3_1B.V6` (723.821.112 B, feat `0x0c`, Q6K unembed):
  suite **1/3** — Q1 ✓ (`Meu nome é:` + eos); Q2 `1`+eos (flip greedy:
  `eos` 0,43 vs `+` 0,29 aqui, `+` 0,60 lá); Q3 tenta `{"resultado":…}` mas
  valor errado. Pesos 7/7 idênticos à referência; forward cos 1,0 no `q`;
  teacher-forcing cos 0,76–0,92 com top-1 0/13 (direção certa, topo trocado).
- **3B** `target1/FALCON3_BASE.V6` (1.266.709.160 B): 0 buracos, entropia 5,71,
  suite 0/3 (começa certo, entra em loop). **A fonte exata também dá loop
  no Q1-greedy** — dinâmica modelo+greedy, não bug de conversão.
- **7B** (2.323.861.712 B) e **10B** (3.036.624.032 B): refeitos, triagem
  limpa (0 buracos, bytes exatos), unpack 4/4 MATCH vs referência.
  Inferência **não executável neste host** (7B ~9 GB, 10B ~11 GB vs ~10 GB
  livres) → coerência = HYPOTHESIS até ferro maior.
- **Benchmark 1B:** v6 **3–4x mais rápido** que transformers-CPU
  (2,5–3,6 vs 0,8–1,0 tok/s; TTFT 3–5x menor). Q1 ambos OK; Q2 só fonte;
  Q3 ambos falham (a Q3 com `<quanto e 2+2>` derrota até o modelo exato).
- **Conversor** (`tools/convert_falcon3_to_v6.py`): 6 bugs provados e fixados —
  chaves retangulares zeradas, scale ignorada (1,0 vs ~16–43), branch
  float frouxo (unembed 100% zeros), scale invertida (BitLinear divide),
  unpack consecutivo-vs-estridado, unembed ternário→Q6K (feat bit3;
  Q6K empata com o denso caractere a caractere). Originais quebrados em
  `*.broken`; espelho `models/FALCON3.V6` restaurado após cada conversão.
- **Kernel:** `unembed_q6k` ligado (loader + forward + save + estimate;
  `@fixer`, `cargo check --release` verde, golden intacto). WIP alheio em
  `hermes/agents.rs` (frase de energia) intocado.
- n-sgdb: `scope=project/neural-os-core`, entities `adr-0116` + `mom/decision`.

## Diário de estudos (vereditos)

Formato por estudo: hipótese → protocolo no rig → números antes/depois →
veredito (ADOTADO/REJEITADO/PARCIAL com condição) → destino
(kernel/tools/nada). Sem veredito medido, o status é HYPOTHESIS (ADR-0113).

### E01 — Test-Time Compute Scaling (GRPO/PRMs, thinking models)
- **Veredito:** PARCIAL (mecanismos pobres já existem; thinking pleno BLOCKED).
- **Porquê:** exige checkpoint reasoning-treinado (Falcon3 não é thinking;
  nenhum existe em `.v6`) + orçamento de 10–100x tokens. A 0,15–3 tok/s,
  "dezenas de segundos de pensamento" = 3–10 tokens aqui — invíavel hoje.
  Sem PRM/verificador no repo; treinar GRPO exige cluster (fora do GTX 1050).
- **O que já existe (mesma direção, escala adequada):** difficulty tiers +
  budgets max_gen (`difficulty_gate`/`heap_aios`), `generate_speculative` +
  cabeças Medusa (slots no formato), `StructuredDecoder` + TV-DSL (ferramenta
  exata em vez de raciocínio LLM).
- **Para adotar:** tok/s subir 10x+ E checkpoint reasoning em `.v6`.
- **Falsificação:** converter R1-distill p/ v6; Q2/Q3 com budget thinking vs
  direto no `bench_v6.py`.

### E02 — MLA e compressão de KV cache
- **Veredito:** REJEITADO como arquitetura; REFERENCE para loader futuro.
- **Porquê:** MLA não é retrofitável (exige retreino 1–10B — sem orçamento);
  o ganho "90%" é escala datacenter (ctx 100k–1M, multi-tenant); nosso ctx
  ≤4K single-user. YAGNI (ADR-0113 §3).
- **O que já existe:** ADR-0111 (KV INT8 3,76x real, paginado `KvPages`,
  H2O evict) — a frente KV-memória já está coberta com números medidos.
- **Falsificação:** nenhuma sem retreino → BLOCKED em orçamento de treino.

### E03 — MoE granularidade fina + roteamento dinâmico
- **Veredito:** PARCIAL (arquitetura já adotada; specifics datacenter rejeitados).
- **Porquê:** Trinity MoE + router treinável + telemetria neural/keyword/fallback
  + experts on-demand (`get_or_mmap_expert`) + AutoLearn JÁ existem. O estudo
  (671B/37B ativos, All-to-All, load-aware) pressupõe cluster e 100B+ —
  inexistentes aqui.
- **Gaps reais (acionáveis):** router neural ratio (medir via telemetria) e
  4/7 experts sem pesos — não "mais granularidade".
- **Falsificação:** subir neural_route_ratio + wirar experts restantes; medir
  sucesso em probes tipo-suite.

### E04 — Speculative decoding em árvore
- **Veredito:** PARCIAL (mecanismo existe; faltam draft heads treinadas).
- **Porquê:** formato v6 tem slots Medusa + `generate_speculative` +
  `ngram_spec` com telemetria — mas todos os nossos arquivos têm medusa=0.
  O "2–3x" do estudo pressupõe verificação barata em GPU rica em banda;
  aqui o gargalo é COMPUTE (soft-float CPU): uplift esperado ~1,2–1,5x,
  nunca assumido — medir (SESSION_413).
- **Para adotar:** treinar 1 cabeça Medusa p/ o 1B (pequena; possivel no
  GTX 1050?) + suporte no harness; medir acceptance rate + tok/s.
- **Falsificação:** acceptance <20% ou uplift <10% → REJEITADO.

### E05 — Chunked prefill + PagedAttention (vLLM/SGLang)
- **Veredito:** PARCIAL (paging adotado; chunked multi-tenant rejeitado).
- **Porquê:** PagedAttention ≈ `KvPages` (ADR-0111 P2) + H2O — já reinventado
  em `no_std`. Chunked prefill serve latência multi-tenant; somos single-user
  sem contenção inter-usuários — YAGNI.
- **Falsificação:** N/A (sem carga multi-usuário para medir); revisitar se
  inferência concorrente multi-agente um dia contender.

### E06 — Speculative decoding com drafts gramaticais + payload C-ABI (E04 concretizado)
- **Veredito:** PARCIAL (algoritmo adotável em CPU; transporte CUDA rejeitado).
- **Porquê:** não há stack CUDA no bare-metal (sem drivers/runtime; GTX 1050
  inalcançável; VRAM via BAR da ADR-0112 é dados, não compute). Todo o §1/§4
  do estudo (pinned memory, `cudaMemcpyAsync`, FlashAttention kernel, streams,
  verify kernel in-VRAM) é inaplicável. Os números (<100µs fill, <2µs PCIe,
  2–3x) pressupõem forward de ms em GPU rica em banda; aqui um forward custa
  segundos em CPU compute-bound (soft-float) — a economia INVERTE: verificar
  16 nós custa ≈ gerá-los (ganho esperado ~1,0–1,5x, nunca 2–3x sem medir).
- **Adotável (CPU, sem CUDA):** máscara de atenção em árvore + verificação em
  passada única (algoritmo puro); drafts gerados pelo DFA/gramática
  (`StructuredDecoder` + `OutputGrammar` — dispensa modelo de rascunho
  treinado, ao contrário das cabeças Medusa); API de retorno enxuta
  (`accepted_path_length` + `last_node_id`); payload de layout fixo C-ABI
  como padrão (sem serialização — mas sem DMA; é chamada local).
  Encaixe natural: saídas constrangidas (JSON/shell/skill), que são a carga
  real do kernel — não chat aberto.
- **Pré-requisito:** taxa de aceitação medida (condição do E04); gramática só
  acelera onde a tarefa já é constrangida.
- **Falsificação:** protótipo no harness Python (tree-verify sobre
  `generate_speculative` + drafts do FSM) medindo accept rate + tok/s;
  uplift <10% (SESSION_413) ou accept <20% → REJEITADO; só então kernel.

### E07 — Draft simbólico em CPU: 5 pilares comparativos (v6 e v7)
- **Veredito:** PARCIAL (divisão sintaxe/semântica adotável; números rejeitados;
  v7 não precisa de mudança de formato por este estudo).
- **Porquê:** os pilares 1/4/5 (0 MB VRAM, <2µs PCIe, TCO de cloud) pressupõem
  GPU que não existe aqui — sem stack CUDA, sem A100/H100, single machine.
  "0 MB de VRAM" é vácuo (tudo aqui já é 0 MB de VRAM); TCO local = RAM
  (janela heap ~2 GB) + tok/s, não placas. Números 2,5–4x: mesma objeção do
  E06 (forward de segundos em CPU; medir, nunca assumir).
- **Adotável (o ponto forte, novo em relação ao E06):** a DIVISÃO determinística
  sintaxe (CPU/gramática) × semântica (LLM) — e a economia de RAM que ela
  implica: drafts gramaticais custam ~KBs residentes vs cabeças Medusa que
  custariam centenas de MB (268 MB–1,6 GB cada) — argumento local FORTE a
  favor de drafts gramaticais sobre neurais, invertendo a questão aberta do
  E04. Metade já existe (`StructuredDecoder` + `OutputGrammar` + gimberish
  stop como freio); falta o modo especulativo (pré-construir ramos + verificar
  em 1 passada) sob a mesma condição do E06.
- **v7:** NENHUMA mudança de formato necessária — o loop draft/verify vive no
  runtime (InferQueue/CortexAgent), não no arquivo; afinidade
  gramática×vocab é cache derivado em RAM, não seção do formato. Medusa>0
  futuro já cabe nos slots atuais.
- **Falsificação:** a do E06 (protótipo + accept rate + tok/s); soma-se o
  comparativo RAM (KBs vs MBs medidos, não estimados).

### E08 — BitNet 1.58b: viabilidade e regras da migração v6→v7
- **Veredito:** VIÁVEL, com regra de conteúdo estrita (só checkpoint
  nativo-ternário) e sem pressa de formato (o núcleo v6 está provado).
- **Porquê (confronto com medido):**
  - §1 (≥3B ternário ≈ FP16): compatível com o observado (1B fraco, 3B no
    limiar) — mas perplexidade de paper ≠ robustez greedy em chat aberto
    (nossas Q2/Q3). Não usar como promessa de coerência.
  - §2 (CPU 2–6x): CONFIRMADO direcionalmente — medimos v6 3–4x vs
    transformers-CPU no host. No bare-metal o teto é outro (soft-float,
    SSE2 sem AVX por SESSION_336/412; 0,15 tok/s WHPX). v7 preserva o que
    é rápido (ternário packed + Q6K) e não persegue GPU-isms.
  - §3 (GPU parcial): irrelevante aqui (sem stack CUDA) — e CONFIRMA não
    desenhar o v7 para Tensor Cores (ADR-0112 segue dados, não compute).
  - §4 (PTQ destrói; exige QAT): PROVADO DUAS VEZES — na literatura E na
    nossa árvore (a via absmean-de-denso gerou o FALCON3_1B.V6 quebrado:
    zeros + escalas erradas). Consequência estratégica: v7 **não** é
    "reempacotar denso melhor"; conteúdo viável = checkpoints
    nativos-ternários (TII/Mútuo). Risco de suprimento (HYPOTHESIS): v7
    depende de releases upstream — sem cluster próprio p/ QAT.
- **Regra de migração (falsificável):** v7 adota uma mudança SOMENTE com
  antes/depois medido no rig (suite + paridade + bench); formato carrega o
  provado (ternário packed, Q6K, header autodescritivo, feat bits); fora:
  layouts p/ GPU, conteúdo PTQ-de-denso, EXL3/affine4 como formato (0114).
- **Falsificação:** qualquer candidato v7 que não bata v6-feat8 na suite +
  bench fica de fora; sem mudança, v6-feat8 JÁ é o v7 de facto.

### E09 — BitNet em GPU: bitplanes, warp-skip, INT4, KV ternário (v6→v7)
- **Veredito:** majoritariamente REJEITADO (veículo GPU inexistente); dois
  pontos testados de verdade, um morto na matemática, um HYPOTHESIS.
- **Porquê, ponto a ponto:**
  - Bitplanes AND/POPCOUNT: o veículo CUDA não existe aqui; a MATEMÁTICA já
    é a nossa (ADD/SUB ternário, sem multiplicação). Layout bitplane vs
    códigos-2-bit intercalados = mesmos bytes rearranjados — trocar o layout
    do v7 exige benchmark CPU antes (SESSION_412); HYPOTHESIS de baixa
    prioridade, sem mudança de formato por este estudo.
  - Warp-skipping 16×16: MORTO na matemática — com ~40% de zeros uniformes
    (medido: q0 40,8%), P(tile 16×16 todo-zero) = 0,4^256 ≈ 0; até 4×4 dá
    0,4^16 ≈ 4e-7. Zero-skip em nível de elemento JÁ existe (teste
    `W=0 SKIP`). Nada a adotar. Falsificação exata acima, sem código.
  - INT4 Tensor Cores: rejeitado (sem compute GPU; coerente com E08).
  - KV cache 1,58-bit: HYPOTHESIS genuína — mas é RUNTIME, não formato
    (zero impacto no v7!): KV INT8→ternário ≈ 5x menos RAM, trocando por
    escalas por token (overhead ~12,5% — contar o metadado, SESSION_414) e
    risco de recall. Pré-requisito inexistente: probe de in-context recall
    (não temos). Nosso KV já é INT8+paginado+H2O (0111); o ganho seria
    memória/ctx-longo, nunca tok/s.
  - Tabela 0,8–8x: números GPU-condicionais, nenhum transferível; valem como
    prova de que co-design layout/compute importa (nossos 3–4x CPU medidos).
- **Impacto no formato v7 deste estudo: NENHUM.**
- **Falsificação:** (i) tile-skip: feita acima, sem código; (ii) KV ternário:
    prototype no harness + probe de recall; degradação de recall ou ganho
    <2x sobre INT8 → REJEITADO.

### E10 — Cenário full-stack RTX 3050 6GB: Falcon3-10B a 150+ tok/s
- **Veredito:** REJEITADO como cenário (pilha CUDA inexistente aqui);
  aritmética de pesos corroborada pelo nosso artefato; um erro contábil
  no warp-skip.
- **Porquê, auditado número a número:**
  - Pesos 10B ~2,2 GB: CORROBORADO — nosso FALCON3_10B.V6 verificado tem
    corpo ternário de 2.236 MB (2.896 − 2×330 MB Q6K) ≈ 2,2 GB. A parte de
    pesos do cenário bate com o artefato medido.
  - "~40% zeros": CORROBORADO em ordem (q0 do 1B: 40,8% zeros medidos).
  - "2,2 GB → 1,3 GB por passada via skip": FALSO como declarado — para
    saber o que pular é preciso LER as máscaras, e as máscaras SÃO os
    2,2 GB (piso de banda = tamanho total). Skip economiza COMPUTE
    (MACs pulados), não tráfego de pesos em single-batch. E tile 16×16
    segue morto (E09: 0,4^256).
  - 150 tok/s e tabela 15x/40x/8x: aritmeticamente possíveis NO PAPEL
    (teto de banda 168 GB/s ÷ 2,2 GB ≈ 76 tok/s × 3–5 aceitos), mas
    empilham 4 máximos nominais sem perdas de interação (accept <100%,
    overhead de verificação, tráfego KV) e exigem a pilha CUDA inteira —
    inexistente no bare-metal. Inacessível E infalsificável aqui.
  - 0,1µs PCIe / energia / TCO-cloud: fora do nosso caminho de hardware.
- **O que transfere (já arquivado, sem dupla contagem):** draft em CPU
  (E06), sintaxe determinística (E07), pesos ternários (E08). Nada novo
  para o formato v7.
- **Falsificação:** nenhuma local possível (exigiria a GPU + stack); o
  cenário permanece hipótese externa até ferro+stack existirem.

### E14 — Além do BitNet trivial: atenção ternária, assimetria, MoTE, KV entrópico, spiking
- **Veredito:** MISTO com viés a rejeitar (4 de 5 exigem treino inexistente);
  dois takeaways honestos (política entrópica p/ E09; hooks v7 mínimos).
- **Porquê, ponto a ponto:**
  - Atenção ternária (popcount/XOR, sem softmax): REJEITADO — exige modelo
    retreinado (equivalente à trava MLA do E02); e atenção é ~2% do prefill
    aqui (SESSION_413), então o ganho seria desprezível mesmo funcionando.
    "100% livre de float" ignora embed/RMSNorm/RoPE/softmax do resto.
  - Sinapses assimétricas ({−α,0,+β}): HYPOTHESIS fraca — a analogia
    GABAérgica é retórica, não evidência; sem checkpoint assimétrico não há
    o que validar; "7B→14B" intestável. Único gancho v7 barato ANOTADO (não
    comprometido): slot de escala dupla (~500 B/arquivo) SE um checkpoint
    assimétrico um dia existir; dequant o absorve de graça.
  - MoTE (1024 micro-especialistas ternários, 100B em 15 GB): REJEITADO
    (nenhum artefato existe em lugar algum — intestável; roteador de bits
    desnecessário com 7 logits). A direção segue no Trinity (E03); experts
    como arquivos `.v6` já são possíveis hoje, sem mudança de formato.
  - KV entrópico adaptativo: UPGRADE do E09 com política concreta (baixa
    informação→1,58b, alta→INT8; entropia da atenção como sinal gratuito)
    — mas mantém os pré-requisitos (probe de recall PRIMEIRO, depois
    protótipo). Sequência honesta em 3 passos, não "fazer tudo".
    Números GPU-condicionais; aqui o ganho seria memória/ctx, nunca tok/s.
  - Spiking BitNet: REJEITADO como mecanismo (reetiquetar trits ≠ SNN;
    exigiria treino SNN do zero, mais pesado que QAT); a esparsidade
    temporal REAL já existe no nível do sistema (agentes EventDriven,
    VAD-gating, yields capados) — nada novo a adotar no peso.
- **Impacto no formato v7 deste estudo: ~NENHUM** (só a anotação do slot
  dual-scale condicional).
- **Meta-ponto (reforça E08):** 4 de 5 itens morrem na mesma trava —
  dependem de TREINO que não temos; validamos/convertemos, não treinamos
  (além de micro-experts). Cadeia de suprimento continua o risco mestre.
- **Falsificação:** atenção/MoTE/spiking — nenhuma (BLOCKED em treino);
  assimetria — checkpoint assimétrico; KV entrópico — recall probe, depois
  curva tradeoff memória×qualidade.

### E15 — Aprofundamento: atenção ternária com fórmula, MoTE tabelado, KV 3 faixas (refina E14)
- **Veredito:** PARCIAL-leaning-REJECT (matemática correta, aplicabilidade
  inalterada; dois refinamentos úteis + um erro de conta na tabela).
- **Porquê, ponto a ponto:**
  - Atenção ternária com cross-terms: a fórmula do produto escalar está
    CORRETA (acordos menos desacordos, 4 popcounts) — arquivar como SPEC
    para SE um checkpoint com atenção ternária um dia existir. Viabilidade:
    inalterada (rejeitada — exige retreino; atenção é 2% aqui). BST idem
    (trocar softmax post-hoc quebra modelo treinado com softmax).
  - MoTE tabelado: aritmeticamente consistente (10M×1,58bit ≈ 2 MB ✓),
    mas sem artefato existente — veredito inalterado. Nota menor: top-K
    sobre 1024 experts não é O(1) literal (é O(N log K) paralelizado).
  - KV 3 faixas com limiares: a POLÍTICA é aproveitável (1,5/3,8 bits como
    limiares explícitos — refina a hipótese E09/E14). Mas: (1) a tabela
    NÃO fecha para o config declarado — 32L×8KV×128×2 soma 65.536
    elementos/token, não 5.120 (os 81.920 bits implicam config ~12,8x
    menor; erro de conta, método preservado); p/ nosso 10B real
    (40L×4KV×128×2): 80 KB/token em FP16, não 10,2 KB. (2) Nossas
    entropias medidas (7–12 bits, distribuições planas) cairiam quase
    todas em "alta" — a hipótese 70/20/10 NÃO vale aqui; sem distribuições
    afiadas, a economia evapora. Continua valendo: recall probe primeiro.
- **Impacto no formato v7: NENHUM além do já arquivado.**
- **Falsificação:** herdada do E14; soma-se: recalcular qualquer tabela de
  capacidade contra o config declarado antes de citar (lição: métricas de
  doc derivam — SESSION_329).

### E16 — Simulação comparativa Phi-4/Llama-3.1/Falcon3 na 3050 (seleção de modelo)
- **Veredito:** cenário REJEITADO (GPU-condicional); seleção CORRIGIDA com
  artefatos verificados; um candidato novo acionável (BitNet-2B-4T).
- **Porquê, auditado com fontes (websearch 2026-10-13):**
  - Pesos (~2,76/1,58/1,97 GB): aritmética OK em empacotamento ideal 1,58
    (byte real dá ~25% a mais — nosso 10B: 2,24 GB corpo vs 1,97
    alegados). Tok/s e tabelas 15x/40x: GPU-condicionais, rejeitados.
  - Llama-3.1-8B p/ bare-metal: DUPLAMENTE furado — (1) NÃO existe ternário
    nativo 3.1; o mais próximo é HF1BitLLM/Llama3-8B-1.58 (v1, FINE-TUNE
    10–100B tokens, não from-scratch); (2) "tokenizador simples p/ MMIO"
    é erro de categoria (tokenizer roda em strings na CPU, não em
    registrador). E a família Falcon3 JÁ está na lista oficial do
    bitnet.cpp — o "padrão-ouro" alegado ignora o incumbente.
  - Phi-4-14B: NENHUM artefato nativo-ternário existe (só PTQ denso da
    comunidade) — pick inacionável. (Eco da 1ª pergunta da saga: Phi-4-mini
    é denso Q4_K, não ternário.)
  - Tabela do paper BitNet-2B4T (evidência externa forte): nativo-2B-4T
    (4T tokens) média 60,68 — melhor de todos; Falcon3-7B 50,76 ≈
    Llama3-8B-finetune 49,75; OLMo-Bitnet-1B NATIVO só 32,22 (TriviaQA
    0,00!). Lições: escala de treino (4T) domina; 1B-nativo pode ser RUIM
    — corrobora independente nosso veredito "1B fraco = capacidade, não
    pipeline". Nota fina: o paper classifica Falcon3-1.58bit como PTQ —
    refina E08 (vale checkpoint ternário LANÇADO-funcionando, QAT ou
    calibrado pelo vendor; PTQ ingênuo nosso continua destruindo).
  - "Multimodal nativo" p/ Falcon3-10B: enganoso (text-only + folga;
    E11 vale).
- **Menu corrigido (só artefatos existentes):** Falcon3-{1,3,7,10}B-1.58bit
  (temos os 4) + **BitNet-b1.58-2B-4T (CANDIDATO NOVO: nativo, 4T tokens,
  melhor benchmark, GGUF existe, ~0,4–0,8 GB — maior coerência-por-byte
  esperada; AVALIAR no rig)** + HF1BitLLM/Llama3-8B-1.58 (existe, números
  piores, fila atrás).
- **Impacto v7: seleção apenas, sem formato.**
- **Falsificação:** rodada BitNet-2B-4T no rig (download GGUF → suite);
  se limpar Q1–Q3, vira lab leve; Llama3-8B-finetune só se sobrar tempo.

### E17 — Binário puro 1.0-bit vs ternário 1.58-bit (o 0 fica)
- **Veredito:** REJEITADO o binário (o próprio estudo conclui pela retenção
  do 0); CONFIRMAÇÃO da escolha ternária com números nossos.
- **Porquê, auditado:**
  - Matemática XOR+POPCOUNT correta; tabela -36,5% confere (1/1,58).
  - Capacidade: consistente com literatura (b1.58 nasceu porque o 1-bit
    puro perdia) E com E16 (FBI binarizado fraco na tabela) E com medido
    aqui (30–41% zeros; destruir zeros = colapso — nosso 1B quebrado).
  - Warp-skip a 0%: discutível, pois tile-skip já é morto (E09) — mas a
    direção (zeros permitem skip em elemento, que existe) está certa.
  - Binário exigiria checkpoints binários treinados — inexistentes p/
    modelos modernos. BLOCKED em artefato, antes mesmo da qualidade.
- **Nota v7 (única novidade aproveitável):** "packing 5-in-8" (3^5=243<256,
  ~1,6 bits/trit vs nossos 2,0) como HYPOTHESIS de formato — economiza
  ~20% com decode por div/mod (mais caro que shifts em CPU soft-float).
  Medir decode+matmul antes (SESSION_412); sem número, sem formato.
- **Falsificação:** binário — BLOCKED (sem artefato); 5-in-8 — benchmark
  de decode no host.

### E18 — Fuzzy-BitNet (soft-ternary, T-normas, sem STE)
- **Veredito:** REJEITADO (resolve não-problemas aqui ou exige treino do
  zero com outro forward; tabela sem artefato nem número).
- **Porquê, ponto a ponto:**
  - Soft-ternary nas fronteiras: o problema (corte seco destruindo x=0,2)
    existe em quem quantiza denso→ternário — nós HERDAMOS pesos já
    ternários (nada para suavizar). O único ponto onde NÓS limiarizamos
    (absmean do lm_head) já foi eliminado via Q6K — consistente.
  - Max-min no lugar de matmul: NÃO é aproximação, é OUTRA arquitetura
    (álgebra tropical); checkpoint treinado com matmuls daria lixo.
    Exigiria treino do zero com esse forward — mais pesado que QAT.
    "1 ciclo" é GPU/SIMD-condicional (somos escalar soft-float).
  - Fim do STE: verdade em princípio, mas problema DO TREINADOR (TII),
    não nosso; nosso treino (micro-experts densos) não usa STE.
  - Execução fuzzy (pesos 1,58b + ativações INT2/4): o núcleo legítimo
    (quantização de ativações) JÁ existe em int8 no HF; o resto volta a
    exigir retreino. Regras SE-ENTÃO como compute = item 2.
  - Tabela ("superior <10B", "menos alucinações"): afirmações sem
    artefato, benchmark ou citação — contra nossa evidência (capacidade
    segue escala de treino, E16: 2B-4T forte, OLMo-1B fraco).
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** treino fuzzy-forward do zero (BLOCKED, mais pesado
  que QAT); sem isso, restam as aproximações que já usamos (absmean→Q6K,
  int8 de ativações no HF).

### E19 — Atenção fuzzy Max-Min (R∘V) com matemática completa (refina E18)
- **Veredito:** REJEITADO (matemática correta, inaplicável sem retreino;
  dois enquadramentos honestos a corrigir).
- **Porquê, auditado:**
  - A conta fecha: tripla de pertinência com partição, equivalência por
    T-norma de Gödel, agregação, composição max-min, centroide — teoria
    fuzzy padrão, bem formada. O que NÃO fecha é a ponte com o checkpoint:
    rodar pesos treinados com softmax/matmul por max-min produz lixo;
    exige treino do zero com esse forward (mesma trava do E18, agora com
    a spec exata arquivada para esse dia).
  - Enquadramento 1 a corrigir: "elimina O(N²·dk)" — elimina os FLOATS,
    não a quadrática (R∘V continua O(N²)); e atenção é ~2% do prefill
    aqui, então mesmo perfeita a economia seria residual.
  - Enquadramento 2 a corrigir: Q,K,V ∈ [-1,1] é premissa de entrada, não
    fato — nossas ativações não vivem nesse intervalo; sem a normalização
    exata do treino, as pertinências são indefinidas. Mais um acoplamento
    com treino, não um detalhe.
  - Tabela: "suave por partes" honesto; resto GPU/SIMD-condicional.
- **Impacto no formato v7: NENHUM** (mecanismo de runtime futuro, não pesos).
- **Falsificação:** treino com forward R∘V (BLOCKED); sem checkpoint, sem
  número — permanece spec arquivada.

### E20 — Fuzzy-BitNet: balanço ganhos vs perdas (recapitula E18/E19)
- **Veredito:** REJEITADO (ganhos já julgados; perdas trazem 2 objeções
  novas e locais, que fortalecem a rejeição).
- **Porquê — ganhos (nada novo):** fim do STE (problema do treinador),
  fronteira (nada a suavizar aqui; absmean→Q6K feito), sem-softmax
  (exige retreino; atenção 2%), robustez a ruído (afirmação sem artefato
  nem número — e nosso ruído real já é tratado por VAD/thresholds
  determinísticos).
- **Porquê — perdas (2 pontos novos, locais e fortes):**
  1. Divisão do centroide por elemento: em CPU soft-float (sem FPU),
    divisão por software custa dezenas de ciclos — nossa doutrina proíbe
    div em hot path (SESSION_331). O "pequeno gargalo" do texto é
    desproporcionalmente caro NO NOSSO alvo (na GPU do texto, irrelevante).
  2. 3x RAM de ativações: bate direto na nossa restrição nº 1 (janela heap
    ~2 GB; frente KV-INT8 da 0111 existe porque RAM manda). Propor 3x
    ativações p/ edge com pouca RAM é auto-contraditório no nosso perfil.
  3. Tensor Cores subutilizados / ecossistema: moot aqui (sem CUDA; aqui
    tudo já é from-scratch em `no_std` — custo normal, não incremental).
- **Impacto no formato v7: NENHUM** (terceira confirmação seguida).
- **Falsificação:** treino fuzzy (BLOCKED); sem ele, o balanço permanece
  teórico — e, no nosso hardware, negativo mesmo no cenário otimista.

### E36 — Código de implementação: cirurgia MoE, router Laya, kernel MTP
- **Veredito:** MISTO (bloco 1 devastador; bloco 2 o melhor da série;
  bloco 3 com bugs reais).
- **Porquê:**
  - §1 cirurgia MoE: a CONFIGURAÇÃO contradiz o texto em ~22x (código dá
    128×10,7M+44M ≈ 1,4B; texto alega 128×234M+1B ≈ 31B) e os DEFAULTS
    (hidden 4096, 8 experts) não são do Qwen3-30B (2048, 128 experts
    fundidos — as chaves `experts.{i}.*` dariam KeyError; Qwen3 usa
    tensores empilhados). `forward` NUNCA lê os experts (soma cópia do
    shared ×0,125 mágico — rascunho decorativo); scales descartadas
    (E24: sem elas, magnitudes erradas); "centroide" é slice (sem média);
    "empacota 4/byte" grava 1 byte/valor. Uma IDE executando isto produz
    modelo quebrado em silêncio (zero asserts).
  - §2 router Laya: MELHOR código da série (limpo, no_std-real, espelha
    nosso `difficulty_gate`) — padrão adotável. Ressalvas honestas:
    limiares 1,25/3,10 são o TERCEIRO conjunto não calibrado (nossas
    entropias planas pedem calibração, E15); entropia exige a distribuição
    JÁ softmaxeada (senão dobra o custo do decode); sem histerese, tau
    fronteiriço oscila modo por token.
  - §3 kernel+binder MTP: (1) t1 auto-aceito SEM verificação quebra o
    contrato especulativo (exatidão vs alvo) — Medusa/spec verificam tudo;
    (2) `__global__` chamado como função comum NÃO compila (launch exige
    `<<<>>>` ou `cudaLaunchKernel` — o launcher E13 fazia certo; regrediu);
    (3) threshold em 2 lugares (`struct` vence, `args` morto);
    (4) null-check em 2 de 8 ponteiros; batch negativo explode o grid.
- **Impacto no formato v7: NENHUM** (runtime/código).
- **Falsificação:** toda acima (papel/código); nada novo exige ferro.

### E27 — White paper Janus-3 v1.0.0 (compilação; blueprint p/ agentes)
- **Veredito:** REJEITADO como blueprint (pilha CUDA + modelos-fantasma);
  MAIS auditável da série (código e contas reais → 7 achados novos).
- **Porquê — código (§3/§5):**
  - `traits.rs`: corrigiu nosso E22-bug#1 (ponteiro-PARA-atômico ✓) —
    crédito registrado; restam mapeamento + fence (E22 #2/#3).
  - `dfa.rs`: `let _ = push` em buffer cheio = DESCARTE SILENCIOSO
    (fail-open; nossa doutrina: refuse com sinal). `new` não-const, sem
    `Default` — cosmético.
  - `ap_gemm.cu`: API `nvidia::mma::fragment` DEPRECIADA/removida nos
    toolkits atuais + `x_sum = 0` (hole: a correção exige a redução real;
    como escrito, calcula errado).
  - `janus-ffi`: `alloc` sem check-null (UB) + LEAK (sem `destroy`);
    `internal_state` stub. `bool` em repr(C) ok.
  - `astg_converter.py`: limiares ±0,33 CEGOS À ESCALA (pesos init 0,02;
    sem normalização tipo absmean, tudo colapsa p/ 0) + objeção E21 do σ
    mantida; envelope gaussiano ainda cria incentivo perverso pró-zero.
- **Porquê — contas (§4/§7):** componentes somam 6144, não 6082 (erro) e
  98,9% viola headroom; §7 republica E10/E25/E26 (27B/MoTE fantasmas).
- **Porquê — processo (§6):** prompts cimentam número não medido (<20ns)
  como requisito — specs p/ agentes carregam budgets MEDIDOS
  (nosso `preflight_wave`), nunca proclamados.
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** toda acima no papel/código; nada exige ferro novo
  para julgar — e nada autoriza gasto nele.

### E38 — System prompt v7.1 (absorção recorde + regressões herdadas)
- **Veredito:** MISTO-POSITIVO (maior absorção doutrinária; código com
  os mesmos bugs + 4 novos).
- **Porquê — absorvido de verdade:** regra anti-hardcode NL + SFT
  dinâmico + destilação KL (receita QAT correta na forma); probe
  generativo SEM asserts canned (espelha nossa suite!); Convention A,
  bfe, wrapper, entropia atrasada mantidos.
- **Porquê — herdado sem mexer:** stride K/2+sem-tx, granularidade,
  API depreciada, X-packing indefinido, type-39, MSb-first, ±0,33,
  2000-steps, workspace-test.
- **Porquê — NOVO e quebrado:**
  1. Exportador sem gate de domínio: soma/trunca o que vier (packed
    como denso = lixo; denso destruído) — faltou nossa gate de exatidão.
  2. Validação MTP sumiu: sem kernel verificador, o loop especulativo
    não tem aceite (pior que o E36 bugado — lá existia).
  3. Tau não chega às camadas (forward sem fiação declarada p/ modelo
    convertido) — treino roda sem anneal de fato.
  4. CLI documentado ≠ função mostrada (`--checkpoint/--out` vs dict);
    metadata GGUF pior que E29 (sem dims → descarregável nem em fork).
- **Impacto no formato v7: NENHUM** (runtime/treino).
- **Falsificação:** acima; padrão que se confirma: doutrina absorvida
  rápido, código com defasagem de uma versão.

### E37 — System prompt v7.1 (absorve E32/E33/E36; regressões novas)
- **Veredito:** MISTO-POSITIVO (maior absorção da série; 3 regressões +
  1 contradição de escopo).
- **Porquê — absorvido de verdade (crédito):** Convenção A + invariante
  com exceção; `bfe.u32` no lugar do lop3; wrapper-launcher em vez de
  `__global__` direto (E36); entropia ATRASADA H(t-1) — nova e boa
  (quebra o eco de feedback, lição E10 aplicada!).
- **Porquê — regressões novas no código "definitivo":**
  1. Stride K/2 com dados INT2 (K/4): lê o dobro do range (passa da
    linha + OOB no fim) — e SEM `+threadIdx.x` (E32 de novo).
  2. Granularidade: desempacota p/ BYTES e alimenta fragmento que lê
    NIBBLES (50% zeros intercalados + lanes trocados).
  3. Exportador confunde domínio: em checkpoint nativo-empacotado lê
    bytes como dense (lixo); em denso, ±0,33 fixo destrói (sem norma).
    Faltou o gate de integralidade exata (nossa correção E24!).
- **Porquê — segue pendente:** API fragment depreciada; X-packing
  indefinido; type-39; metadata hardcoded; pickle sem weights_only;
  xref quebrado (`convert_falcon_qat.py` inexistente); 2000 steps vs
  regra 10–50B deles mesmos; `cargo test --workspace` sem excludes
  (nossa SESSION_417: bins bare-metal não rodam no host).
- **Impacto no formato v7: NENHUM** (runtime/treino).
- **Falsificação:** acima; o ciclo que funciona: absorveu→reverificar
  (faltou a 2ª metade aqui).

### E35 — Qwen3-30B-A3B supremo: micro-MoE + System1/2 + MTP + INT2
- **Veredito:** REJEITADO (recompõe itens bloqueados + novidade que não é
  nova + contas que não fecham).
- **Porquê, auditado:**
  - A "fragmentação" 128 + compartilhado DESCREVE o Qwen3 nativo (128
    experts + shared nas release notes) — a novidade alegada é ternarizar,
    que exige QAT (BLOCKED; mesmo E08). Sem o ternário treinado, o resto
    é arquitetura alheia com outro nome.
  - 5,90 GB usa packing ideal; byte real dá 7,5 GB = OOM em 6 GB (E26 de
    novo). 4,8 GB "total" não fecha com os componentes (4,8 pesos + 0,6
    KV + 1,0 workspace = 6,4 > 6).
  - System1/2 por entropia morre nas NOSSAS distribuições planas (quase
    tudo cai no System 2 caro — E15) + mesmos bloqueios (E01/E34).
    "Superior ao FP16": extraordinário sem medida.
  - MTP: existe (DeepSeek), mas cabeças ternárias exigem treino (E04);
    3x presume aceite total (real ~1,8–2,2x); NVMe-offload custa ~94 ms
    só de fetch dos 8 ativos (470 MB a ~5 GB/s) — inviável p/ interativo.
  - lop3/INT2: E33 vale (não reposiciona; zero ganho vs nosso 2-bit).
  - Passos 1–4 fora de ordem (fragmentar antes do QAT que o viabiliza).
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** acima (arquitetura documentada, aritmética, latência);
  empírico BLOCKED em treino + artefato.

### E34 — Fronteiras v7: MX, Fuzzy-SSM, TMA/WGMMA, spiking (auditoria dura)
- **Veredito:** REJEITADO (incoerência interna de alvo + fantasmas +
  números à deriva; um padrão real reaproveitado).
- **Porquê, auditado:**
  - TMA/WGMMA exigem Hopper (sm_90+); o paper mira RTX 3050/4060
    (Ampere/Ada, sm_86/89) onde essas instruções NÃO EXISTEM (falha de
    launch). Alvo e kernel incompatíveis no mesmo documento.
  - OCP: E8M0 existe; "MX-INT2/MX-Ternary" como padrão NÃO (MX oficial
    para em 4-bit) — padrão real + extensão inventada.
  - O(1) do SSM é MEMÓRIA, não tempo (recorrência segue O(N) passos;
    1M tokens × ms/passo em CPU = horas). Latência ≠ pegada.
  - Kernel v7: lê byte de entrada como acumulador (`c_frag` calculado e
    descartado!); `x_sum = 0` de novo; `0.02f` não é representável em
    E8M0 (potências de 2!); regras sem condição de aplicabilidade
    (padrão E32 violado pelo próprio autor).
  - Números à deriva: 36,5%→43%, 150→600 t/s por versão; "27B" fantasma;
    spiking igual ao E14.
  - Aproveitável: OCP-E8M0 como vocabulário de escalas (não adotado sem
    HW) e o molde "provar por estágio" — o resto segue E02/E09/E14.
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** acima (ISA, padrões, aritmética); empírico BLOCKED
  em HW inexistente + checkpoints inexistentes.

### E33 — Next-gen: validação cruzada (XGrammar/SpinQuant/Diff) + INT2 + regras
- **Veredito:** REJEITADO como pacote; referências de existência
  VERIFICADAS (prateleira útil); 3 refutações exatas.
- **Porquê:**
  - Existência verificada (bom para a prateleira): XGrammar, Differential
    Transformer (MSFT 2024) e SpinQuant existem de verdade — mas o texto
    os usa torto: XGrammar opera VIA máscaras de logits (não "em vez de
    máscaras", como alegado); Hadamard exige pesos DENSOS p/ absorver a
    rotação (+ matmul extra por ativação — "sem parâmetros" esconde
    compute; e nossos lineares já são ternários, nada a absorver).
  - `lop3.b32` "desempacota em 1 ciclo": FALSO por semântica da ISA —
    lop3 é vertical por bit (sem mover bits entre posições); espalhar
    códigos 2-bit em nibbles exige shift/permute antes (≥2-3 ops). A LUT
    0x38 exibida faz AND-mask, não unpack com reposicionamento.
  - "INT2 economiza 36%": contra o PRÓPRIO uINT4 desperdiçado deles, não
    contra nosso 2-bit (que já é 4 pesos/byte) — ganho zero aqui.
  - Diff-Fuzzy/MLA-Fuzzy: precisam retreino (BLOCKED); regras `.cursorrules`
    sem condição de aplicabilidade (padrão E32: regra diz quando NÃO vale).
  - Tabela v5→v6 e conclusão: mesmos condicionantes julgados.
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** acima (ISA, existências, aritmética); o que sobra de
  útil é a prateleira de referências verificadas, não o pacote.

### E34 — W2A8-int8 no host + GPU-host: medido (não suposto)
- **Veredito:** W2A8-host REJEITADO (6x mais lento, vereditos intactos);
  GPU-host VIÁVEL-só-dev (oferta, não execução).
- **Porquê, medido:**
  - W2A8 (7 lineares, flag `chat_v6.W2A8`, int32 exato, self-test <1%):
    suite 1B.V7 com classes de veredito IDENTICAS ao fp (Q1✓Q6✓, resto
    mesmos fails — textos mudam, vereditos não: prova de robustez a
    ruído 1%). Porém tok/s 0,3–0,7 vs 2,6–3,6 fp32 (~6x PIOR): `_int_mm`
    sem fast-path + tráfego de conversão por step vs MKL fp32/AVX.
    Expectativa (1,2–1,5x ganho) refutada pela medição — fica registrado
    p/ não repetir. Default off mantido.
  - GPU-host: RTX 3050 6GB + drivers + 5,4 GB livres VERIFICADOS (corrige
    o lore GTX 1050 p/ esta máquina); torch é CPU-only → reinstalação
    ~2 GB + port de device; footprint 1B-int8 (~1–2 GB) cabe; estimativa
    honesta 5–10x nos loops de validação. Dev-velocity SOMENTE (kernel
    sem stack CUDA — vereditos E anteriores intactos). Oferta ao usuário,
    não executado sem OK (custo/risco de env).
  - Pulados com motivo (sub-limiar/incerto/ótimo): threads-tune (<10%,
    SESSION_413), jit (incerto), atenção/KV (2%), prefill (já em batch),
    1B (mínimo).
  - Kernel-W2A8 (ADR-0084): veredito SEPARADO e pendente — trade distinto
    (evitar libm soft-float); exige medição no lab QEMU; NADA aqui
    transfere para lá (nem a favor nem contra).
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** feita (self-test + suite + tok/s A/B); resto é oferta
  documentada, não promessa.

### E32 — Patch de harmonização (responde E21/E27/E28; Convenção A)
- **Veredito:** MISTO-POSITIVO (primeira absorção real de findings;
  com 2 bugs novos e 1 subespecificação).
- **Porquê — corrigido de verdade (crédito):** sentinel unificado
  (0xDEAD único); watchdog com contador+`Result` (virou instrumento
  funcional); fragments 16,8,64 casando a instrução (o mismatch da v1
  sumiu); Convenção A provada + regra `.cursorrules` COM condição de
  exceção declarada — modelo de como escrever regra (diz quando NÃO
  vale). O loop auditoria→fix→verificação FECHOU pela 1ª vez na série.
- **Porquê — bugs NOVOS no patch:**
  1. `w_offset`/`x_offset` perderam `+ threadIdx.x` (a v1 tinha!): as 16
    threads da linha leem o MESMO u32 e preenchem 16 slots com 16 cópias
    — 15/16 do tile é lixo duplicado. Regressão introduzida no fix.
  2. Convenção de packing do X indefinida: o kernel lê nibbles
    `(byte>>4, &0x0F)` com `-1`, mas o conversor da série (E29) empacota
    PESOS 4×2-bit — leitor e escritor combinam só se X for nibble-offset
    por decreto não escrito em lugar nenhum. Contrato leitor↔escritor
    ausente (a mesma classe que nos custou o bug transpose).
- **Porquê — segue pendente:** API fragment depreciada; ASTG/σ;
  limiares; modelos-fantasma; números GPU.
- **Impacto no formato v7: NENHUM** (runtime/CUDA).
- **Falsificação:** acima, no papel; o padrão a reter é o processo
  (achado arquivado → fix + regra de regressão + re-verificação).

### E31 — 5 protocolos p/ IDEs (rules, build.rs, FFI, self-heal, sanity)
- **Veredito:** MISTO-leaning-POSITIVO em processo (espelha o nosso, com
  achados nos exemplos); impacto v7 NENHUM.
- **Porquê, ponto a ponto:**
  - Rules file: padrão ADOTADO há muito (AGENTS.md + rules + doutrina) —
    e o nosso é mais forte (levels, cadeia de evidência, ficha anti-
    complexidade; aqui sem "prove it"). Regras 1–2 generalizáveis; 3–5
    Janus-específicas.
  - build.rs nvcc: padrão ADOTADO (nosso boot/build.rs gera ESP; o
    `rerun-if-changed` deles está certo). Defeitos: `-arch=sm_86`
    hardcoded contradiz o multi-arch alegado; `link-search` p/ PTX
    confunde link-time com runtime (PTX carrega via driver, não linka);
    `panic!` sem caminho diagnóstico.
  - Tabela ABI/alinhamento: disciplina CORRETA e portável (cada estrutura
    DMA/MMIO declara align+motivo — nosso `repr(C)` mandatório e
    `map_page_uc`); valores específicos GPU-only. "Qualquer desalinhamento
    = panic" calibra por domínio (x86 tolera com penalidade).
  - Self-heal flowchart: direção certa, IMATURO vs o nosso (observe→plan→
    act→verify→remember + anti-loop + HITL): sem contador de loop (fix
    errado retentado = outage), sem verificar se o fix resolveu, sem
    memória; e 1 sintoma→1 fix erra diagnóstico (IllegalAddress tem N
    causas). Nossa frente Recovery (ADR-0113) já cobre os 3.
  - Sanity MSE<1e-3: direção certa (nossa paridade golden/exact), limiar
    ingênuo (média esconde viés sistemático — precisa max+cos como os
    nossos). E o teste de drift é VACUOSO POR CONSTRUÇÃO: `mu_zero`
    definido como complemento força denominador = 1,0 — testa `x/1==x`,
    nunca falha, nem contra implementação quebrada (mesma classe da
    lição SESSION_446). Para testar o invariante, pertinências devem vir
    das funções independentes, não da definição complementar.
- **Meta-ponto:** processo que só lista passos sem "prove it" produz
  testes que não testam — o próprio método desta ADR (rig + paridade +
  mutação mental) é o que faltou lá.
- **Impacto no formato v7: NENHUM** (processo, não pesos).
- **Falsificação:** acima, no papel; o teste vacuo demonstra-se lendo
  (denominador identicamente 1 por construção).

### E30 — White paper v5 Master (delta v1/v2 + takeaway acionável)
- **Veredito:** REJEITADO como blueprint; UM takeaway acionável (escalas
  por linha) + auditoria que confirma o método.
- **Porquê — código novo auditado:**
  - Watchdog Rust: comparador sem política (quem conta stalls? quem age?
    `max_allowed_stalls` nunca lido) — heartbeat, não watchdog (nossa
    `EscalationState` tem política; isto não).
  - `check_watchdog_timeout`: `asm("exit;")` não é PTX válido (seria
    `trap;`/retorno cooperativo); função nunca chamada no loop; TERCEIRA
    mágica (skip/break/exit) sem protocolo entre elas. TDR real se evita
    com bound <2s por launch — não endereçado.
  - Kernel "industrial": MESMA API depreciada (E28 intacto) + indexação
    `threadIdx.y` como warpId (só 16 threads das 256 carregam — 1/16 do
    bloco trabalhando) + anterior segue valendo.
  - Prompts: PROMPT 4 recimenta <20ns (E28 de novo); roadmap FASE 2 põe
    "QAT 10B/27B em 1 semana" sem base 27B declarada (que modelo-base?
    inexistente) e sem ferro; aceite com números não medidos.
  - Tabela §7: mesmos números julgados (E10/E25/E26); título promete
    visão/áudio que o corpo NÃO contém (zero SigLIP/codec/projetor).
- **Porquê — o takeaway (único acionável):** escalas POR LINHA (γ por row,
  não por tensor): metadata ~1,6 MB/arquivo, prática padrão, e SÓ
  derivável onde temos denso (lm_head!). Protocolo honesto em 3 passos
  locais, sem treino: (1) MSE de reconstrução do lm_head; (2) delta na
  suite (Q2 é o canário!); (3) custo formato+kernel. SE passar, vira
  proposta v7 (vetor de scales) — com número, não com paper.
- **Meta-ponto (valida o método):** quanto mais específico o doc, mais
  bugs achamos (v1: 4, v2: +3, v5: +5) — auditabilidade escala com
  especificidade. Papers vagos rendem "intestável"; este rendeu lista.
- **Impacto no formato v7: NENHUM hoje** (escalas-por-linha em avaliação).
- **Falsificação:** itens de código acima, no papel; escalas-por-linha:
  MSE + suite (local, sem treino, sem GPU).

### E29 — Integração GGUF/llama.cpp/Ollama/LM Studio (Via A fork vs Via B server)
- **Veredito:** MISTO — Via A REJEITADA (derrota o próprio objetivo); Via B
  ADOTADA como padrão (e responde a pergunta original da saga).
- **Porquê:**
  - Via A (fork + `GGML_TYPE_J3_158=39`): um tipo de fork NUNCA carrega no
    LM Studio/Ollama stock (que embarcam upstream) — derrota o objetivo
    declarado. Upstream jamais aceitaria tipo vendor (já têm TQ1_0/TQ2_0
    p/ ternário!). Pesos como TQ existente = sem fork; o resto (affine,
    fuzzy, MoTE) exigiria o fork — que anula o propósito. Bônus: o pack do
    script é MSb-first, contradizendo a ordem LSb-first verificada (E24);
    metadata hardcoded (4096/32/11008/32) descreve modelo nenhum
    (SESSION_329); `torch.load` sem `weights_only` (pickle arbitrário);
    herda os bugs do kernel E28.
  - Via B (servidor OpenAI-compat): o padrão honesto — sem fork, funciona
    HOJE como custom provider (porta 1234, `num_gpu 99`, Modelfile: todos
    reais). O handler do exemplo é STUB (string canned, sem inferência);
    `Mutex` serializa (ok single-user). Aplicado à NOSSA pilha (`.v6` +
    servidor em volta do generate), responde "o LM Studio consegue ler
    eles?" com zero mudança de formato — candidato a próximo build.
  - Prompts p/ agentes: GIGO — reproduziriam os bugs auditados (E27/E28);
    validação de spec antes da implementação é o próprio trabalho desta ADR.
- **Impacto no formato v7: NENHUM** (Via B lê v6-feat8 como está).
- **Falsificação:** Via A — estrutural (upstream); Via B — handshake de
  custom provider no LM Studio (concreto e testável).

### E28 — White paper v2.0 (delta contra v1/E27)
- **Veredito:** REJEITADO como blueprint (mesmos fundamentos); o delta
  mostra que a série NÃO absorve achados arquivados — 3 correções novas,
  o resto intacto.
- **Porquê — o que mudou (crédito):** flags `bool`→`u32` (ABI limpo);
  ponteiro-PARA-atômico mantido; overhead W_sums contabilizado (boa
  higiene, linha SESSION_414).
- **Porquê — o que NÃO mudou apesar do "Refined":** silent-drop no DFA,
  alloc sem check + leak (sem destroy), mapeamento/fence ausentes,
  σ do ASTG desacoplado, limiares ±0,33 cegos, budgets <20ns cimentados
  nos prompts, modelos-fantasma, números GPU-condicionais.
- **Porquê — bugs NOVOS do v2:**
  1. Persistent worker sem acquire: lê o doorbell e consome o payload
    SEM fence entre um e outro (o `__threadfence_system` está DEPOIS,
    na direção errada) — mesma classe do E22, agora do lado GPU. Com
    payload novo + ID novo, o worker pode ler payload velho.
  2. Polling de host-pinned É tráfego PCIe por iteração (volatile
    derrota cache) — o loop "sem tráfego" gera ~1M transações/s; e
    `__nanosleep(20)` tem granularidade real de µs. Auto-contradição
    com a meta declarada (a alternativa MSI-X do E23 existe p/ isso).
  3. Scheduler ASTG definido e nunca chamado no snippet (fiação
    incompleta); treino sem tau annealado = QAT que nunca endurece.
- **Observação de processo:** vereditos arquivados (E27) voltam
  sem correção — series que não absorvem findings têm valor marginal
  decrescente; manter o ciclo medir-arquivar-exigir.
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** toda acima (papel/código); empírico segue BLOCKED
  na pilha inexistente.

### E25 — Pilha unificada Janus-3/F.A.S.T.-3 (27B em 6GB a >135 t/s)
- **Veredito:** REJEITADO como pilha (compõe premissas já julgadas +
  modelos-fantasma + deriva de números).
- **Porquê, auditado na composição:**
  - "Sem driver NVIDIA": CONTRADIÇÃO interna — GPU NVIDIA exige nvidia.ko
    /CUDA OU driver MMIO próprio + firmware assinado (E13); "sem driver"
    absoluto é falso (o nosso, aliás, ainda não existe: AWAITING_HW).
  - Modelos 27–30B: FANTASMAS — nenhum ternário-nativo 27B/30B existe
    publicamente (maiores reais: Falcon3-10B, BitNet-2B). Avaliar pilha
    sobre pesos inexistentes é erro de categoria; ">135 t/s (27B)" não
    tem referente. Números crescem à medida que os modelos ficam mais
    hipotéticos (150 → 230 → 350 t/s).
  - Deriva de números entre estudos: DFA <100µs virou <50µs sem mudança
    declarada; "10,2 KB/token" copiado sem corretagem (E15: não fecha).
    Métricas de doc derivam (SESSION_329) — inclusive entre docs da série.
  - Componentes: todos já julgados (E06/E09/E14/E15/E21/E23) — nada novo,
    só reempilhados com os mesmos condicionantes (CUDA ausente, treino
    ausente, medidas ausentes).
- **Aproveitável (a taxonomia, não a pilha):** os 3 gargalos universais
  (memória/compute/latência) portam bem como problem statement — nas
  NOSSAS unidades (janela heap, tok/s CPU, latência de tick).
- **Meta-observação de processo:** a série recicla 3 temas (drafts,
  ternário-compute, compressão-KV) com números crescentes. Valor marginal
  de mais estudos nessa veia ≈ 0. O próximo passo de valor é ATRAVESSAR
  para medição: protótipo grammar-draft (condição E06), BitNet-2B-4T no
  rig (E16), ou QEMU com os arquivos novos.
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** a pilha como um todo é infalsificável aqui (GPU +
  modelos inexistentes); sub-afirmações seguem os vereditos individuais.

### E26 — Teto 27B/MoTE-30B na 3050 6GB (auditoria decide no 1º número)
- **Veredito:** REJEITADO (modelos-fantasma + piso falso + overflow real +
  inconsistências internas).
- **Porquê, auditado:**
  - 27×0,1975 = 5,33 GB usa packing IDEAL; com byte packing real (o único
    que existe em arquivo: nosso 10B dá 0,224 B/param), 27B = 6,75 GB —
    OS PESOS SOZINHOS estouram os 6 GB. O cenário morre antes de qualquer
    outra conta. (Nosso artefato verificado como régua, de novo.)
  - 3,46 GB/passada repete o piso falso (E10/E25): máscaras precisam ser
    lidas; teto honesto ≈ 168/5,33 ≈ 31 t/s (ideal) ou 168/6,75 ≈ 25 t/s
    (real) — antes de qualquer perda.
  - 0,48 GB/token do MoTE não fecha p/ 3,8B ativos (3,8×0,1975 = 0,75 GB;
    0,48 implica ~2,4B); o ×1,2 não tem mecanismo declarado. MoTE segue
    sem artefato (E14).
  - 98,3% de ocupação contradiz o próprio E10 (2 GB livres) e viola
    doutrina de headroom (nossos gates 48/64/128 MB existem porque margem
    zero = OOM na primeira variação — aqui: display, fragmentação, driver).
  - TTFT/TDP/GPU: mesmos condicionantes rejeitados; modelos 27–30B
    ternários: fantasmas (E25).
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** feita acima (aritmética com packing real + headroom +
  artefatos inexistentes); nada a medir localmente.

### E24 — Hardware existente + QAT/distilação como via (sem treinar do zero)
- **Veredito:** MISTO-leaning-POSITIVO (fatos ok, via de treino honesta e
  precificada, com ressalva matemática pendente).
- **Porquê, auditado:**
  - Hardware (mma.sync u4 desde Ampere, vmin/vmax, cudaHostAlloc,
    AVX2/NEON): FATOS CORRETOS, mas inaplicáveis aqui (sem stack; e
    "AVX2 em no_std" confunde capacidade da linguagem com contrato do
    alvo — o nosso é soft-float sem intrinsics XMM, SESSION_336).
  - PTQ-colapsa: CONFIRMAÇÃO MÚTUA com E08 (eles por doutrina, nós por
    artefato quebrado). Opção A = nossa regra de conteúdo.
  - Opção B (QAT 10–50B tokens, sem do-zero): a PRIMEIRA via de treino
    honesta da série — com conta feita: 8B/10B-tokens ≈ 430 GPU-horas
    A100 (~$1k, dias em cluster pequeno); **1B/10B-tokens ≈ 2 dias-A100
    (~$200!)**. Não milhões; não local (GTX 1050 4 GB não cabe nem 1B
    em QAT). Ressalva pendente: ASTG como especificado não converge
    (E21 — corrigir o σ antes de confiar na receita).
- **Impacto no formato v7: NENHUM direto** (pipeline de treino, não pesos).
- **Falsificação (a mais barata e informativa da série):** piloto 1B-QAT
  em ferro alugado (~$200, ~1 semana) → suite+paridade vs nativo; se
  limpar Q2/Q3 onde o nativo-1B falha, reescreve E01/E08.

### E23 — "Telepatia de Dados": 5 antecipações (Janus-3)
- **Veredito:** MISTO-leaning-REJECT (2 erros técnicos diretos; resto
  GPU-condicional ou já-nosso).
- **Porquê, ponto a ponto:**
  - Pré-estágio L2 ("0ns"): overlap esconde stall, não elimina latência;
    e host-DMA NÃO injeta em L2 de GPU (writes PCIe vão p/ VRAM; L2 enche
    no read do SM — primeiro read dá miss de qualquer jeito). Princípio
    do overlap já é doutrina (async/SMP); "0ns" e "escrita direta em L2":
    FALSOS. `_mm256_stream` XMM vetado aqui (movnti GP ok, SESSION_335).
  - Pré-computar layers 0–8 em 2 ramos: PERDE em núcleo ocupado (16
    layer-evals p/ economizar 8 = 2x custo por ≤1,3x ganho mesmo com 100%
    de acerto). Nossos núcleos nunca estão ociosos; só faria sentido com
    SMs livres. A parte sã (gramática eleva hit rate) recai no E06.
  - Pre-gating MoTE por gramática: princípio (restringir candidatos cedo)
    JÁ existe (`vocab_shortlist`, `StructuredDecoder`); números MoTE/VRAM
    discutíveis; "elimina" ignora experts não-monotemáticos.
  - Delta-XOR (32→3-4 bits): MEIO-VERDADE — máscara/posição comprimem bem,
    mas token ID novo custa ~17 bits sozinho (piso real ~25-30 bits/nó,
    ~5x, não ~10x). E irrelevante aqui (payloads já em KBs; "barramento"
    local = chamada de função — YAGNI até tamanho importar).
  - MSI-X sem polling: princípio adotado há muito (agentes EventDriven,
    MSI-X no xHCI, IPI/SMP; anti-spin SESSION_316). <20ns cross-bus segue
    suspeito (E21); ns reais só intra-domínio coerente.
  - Tabela >300 tok/s: empilha E06/E09/E10/E21, todos GPU-condicionais.
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** feita acima no papel (semântica PCIe, aritmética de
  bits, economia de pre-compute); o único item empírico (razão do delta)
  mede-se em 5 linhas SE payload importar — hoje não importa.

### E21 — 4 inovações: AP-GEMM, partição-unidade, doorbell persistente, ASTG
- **Veredito:** MISTO-leaning-REJECT (uma matemática correta e inaplicável,
  uma correção válida ainda bloqueada, duas objeções exatas novas).
- **Porquê, auditado:**
  - AP-GEMM (offset): a conta fecha — (W+1)(X+1)−ΣW−ΣX−N = W·X ✓. Mas é
    veículo Tensor-Core (inexistente) e em CPU sem unidades INT4/SIMD
    (nosso alvo soft-float) o ADD/SUB atual já é a forma ótima. Sem ganho
    aqui; sem impacto v7.
  - Partição-unidade (fim da divisão): correção VÁLIDA (Σμ=1 ⇒ centroide
    vira subtração) — responde nossa objeção E20. Mas: (1) continua exigindo
    treino com a restrição (BLOCKED); (2) canais de 2 bits requantizam as
    pertinências (nova perda não quantificada); (3) denominador ≈1 com
    wobble de arredondamento — e nossa evidência Q2 mostra que wobbles
    ~1% viram top-1. 
  - Doorbell <20ns: FALSO através de PCIe (round-trip ~1µs; visibilidade
    host→device custa centenas de ns no melhor caso — o número confunde
    atômico on-device com visibilidade cross-bus, ~50x). O PADRÃO transfere
    núcleo-a-núcleo (coerência de cache: ns reais — nosso SMP já vive
    disso), nunca CPU→GPU. `_mm_stream` tem precedente aqui (movnti GP,
    SESSION_335), sem device para sinalizar.
  - ASTG: DUBIDOSO como especificado — tanh(x/τ)·exp(−x²/2σ²) com τ→0,001
    tende a sign(x)·gaussiana (morre p/ |x| grande), NÃO a {-1,0,+1};
    faltou o acoplamento σ→∞ (não declarado). E é treino de qualquer forma
    (BLOCKED, milhões em GPU).
  - Tabela: coluna bitnet.cpp é espantalho (tem kernels GPU + roda em CPU
    em qualquer máquina — chamar isso de "inexistente" inverte a realidade);
    ">200 tok/s" GPU-condicional, intestável aqui.
- **Impacto no formato v7: NENHUM.**
- **Falsificação:** AP-GEMM/partição/doorbell — feitas acima no papel;
  ASTG — refazer o limite com σ(τ) declarado; resto BLOCKED em treino.

### E22 — std/no_std por features (mesma base, dois alvos)
- **Veredito:** PARCIAL-tendência-REFERÊNCIA (padrão são e já nosso;
  exemplo com bugs reais; números não medidos).
- **Porquê:**
  - O padrão (núcleo matemático agnóstico + I/O por `cfg`) É nosso loop:
    host-teste (`cargo test` nas lib crates desde 08/2026, tools Python)
    + runtime `no_std` — o guia "qual abordagem" do texto recomenda
    exatamente o que fazemos. Adotar por continuação.
  - Código de exemplo com 3 bugs reais: (1) `AtomicU64` como CAMPO é
    atômico próprio, não compartilhado — a GPU/device nunca o vê;
    doorbell exige ponteiro PARA atômico em memória compartilhada;
    (2) `*mut u64` cru sem mapeamento de página dá #PF (aqui exige
    `map_page_uc` antes de qualquer MMIO — SESSION_285/299) e sem
    `Sync` não atravessa threads; (3) store `Release` em MMIO-WC sem
    `sfence` pode dormir no write-combining (SESSION_335; `sfence`≠flush
    de dispositivo, SESSION_252 F16) — ordenamento por tipo de memória,
    ausente no exemplo.
  - Tempos "<50µs/<1µs determinismo absoluto": não medidos; nosso
    determinismo é por budgets/TSC, não por proclamação de ciclo.
- **Impacto no formato v7: NENHUM** (organização de código, ortogonal).
- **Falsificação:** feita acima no papel; quem portar o padrão passa por
  checklist: atômico-compartilhado + mapeamento + fence por tipo.

### E11 — Multimodal no backbone 10B (reasoning + visão + áudio)
- **Veredito:** PARCIAL (voz: já implementado aqui além do estudo; visão:
  mecanismo são + único candidato genuíno a seção v7; reasoning-via-prompt
  rejeitado; números GPU-condicionais).
- **Porquê, por modalidade:**
  - Reasoning (`<think>`, R1-style no 10B): REJEITADO como capacidade — o
    Falcon3-10B não é reasoning-trained; tags por prompt dão roleplay, não
    raciocínio (mesma trava do E01: sem checkpoint reasoning + tok/s).
    Draft-para-lógica recai no E06 (medir).
  - Visão (SigLIP/CLIP → 256 tokens → backbone): MECANISMO SÃO e meio
    presente — o conversor tem mapeamento `vit` e existe `VISION.BIN`
    (33 MB) em disco; slots no ModelHub. Bloqueios honestos: (1) LLM
    text-only + encoder sem treino conjunto rende pouco — exige treino SÓ
    do projetor (estilo LLaVA, pequeno, factível fora de cluster);
    (2) fiação do VISION.BIN hoje = UNKNOWN (auditar antes de prometer);
    (3) custo do encoder em CPU (uma vez por imagem; medir, não assumir
    "400 MB/1 passada" de GPU). Números do estudo: GPU-condicionais.
  - Áudio (codecs, Whisper/Moonshine, vocoder, gramática em codebooks):
    DIREÇÃO ULTRAPASSADA AQUI — pipeline de voz real e medido (HDA,
    Piper VITS PT-BR, STT CTC, VAD, wake-word, barge-in, TTS streaming
    por frases). Áudio-token end-to-end é REJEITADO p/ v6→v7 (exigiria
    modelo áudio-nativo + treino; a cascata funciona e é medida).
  - Tabela VRAM 4,3/6 GB: pilha inexistente, mas o MOLDE do orçamento
    presta — refazer em RAM+janela heap+tok/s (nossas unidades reais).
    "Simultâneo" é discutível em single-user.
- **Impacto no formato v7 (único genuíno deste estudo):** SEÇÃO MULTIMODAL
  (matriz projetora + convenção de tokens visuais) — SOMENTE SE visão for
  perseguida, após projetor treinado + probe VQA. Reasoning: nenhum
  (nível prompt/template). Áudio: nenhum agora (condicional futuro: faixa
  de tokens de áudio + marcadores, só se tokens entrarem no contexto LLM).
- **Falsificação:** reasoning → condição do E01; visão → projetor treinado
  + probe VQA + custo CPU + auditoria VISION.BIN; áudio-tokens → já
  rejeitado (cascata medida vence).

### E12 — Rust std vs no_std para o draft engine
- **Veredito:** PARCIAL-tendência-REFERÊNCIA (metade no_std descreve nossa
  doutrina existente; metade std inaplicável; impacto v7 NULO).
- **Porquê, por metade:**
  - `no_std` (DFA estático, heapless, O(1), sem GC): É A NOSSA DOUTRINA —
    arrays fixos (`[DamageRect;16]`, `PEER_KEYS[16]`, `rtt_samples[32]`),
    statics, budgets com cap, checklist runtime-hygiene, ticket-lock,
    libm, `#[repr(C)]` nos descritores. Adotar por CONTINUAÇÃO, não por
    mudança. Pontos novos: crate `heapless` (DEFERIDO — YAGNI; arrays
    hand-rolled funcionam; reavaliar quando um grammar engine concreto
    precisar, comparando LOC); co-processador dedicado (sem esse HW —
    o ANÁLOGO existente são os APs como workers, ADR-0057).
  - `std` (cudarc, pinned, crossbeam/flume): padrões transferem
    (`repr(C)`, canais limitados ≈ nosso EventBus bounded), crates e
    GPU não transferem (std-only / sem stack CUDA).
- **Impacto no formato v7: NENHUM** — substrato de execução ≠ layout de
  dados; decisão ortogonal à migração.
- **Nota de segunda-opinião:** o estudo valida nossas escolhas de substrato
  (zero-dep, estático, limitado) por via independente.
- **Falsificação:** N/A (doutrina, não número); o único item mensurável
  futuro (heapless vs hand-rolled) decide-se por LOC + check quando
  existir o grammar engine.

### E13 — Driver NVIDIA bare-metal: MMIO, GSP, GPFIFO (v6→v7)
- **Veredito:** ADOTADO-COMO-CONFIRMAÇÃO (primeiro estudo amplamente
  adotável) + 4 correções no código de exemplo.
- **Porquê, ponto a ponto:**
  - Distinção driver-eliminável vs firmware-assinado-não-eliminável:
    CONFIRMADA (silício real; GSP desde Turing). Encaixa no pipeline
    existente (`firmware.rs`, FECS+GPCCS) — com um adendo: nossa GTX 1050
    (Pascal) NÃO precisa de GSP; notebooks Ampere precisariam de
    `gsp_ga10x.bin`, AUSENTE em `firmware/` → buscar via
    `download_firmware.py` (está no linux-firmware) antes de qualquer
    teste Ampere.
  - MMIO/BAR/volatile: doutrina existente (scan PCI, `read_bar_size`,
    `map_page_uc`, padrões e1000/xHCI); papéis de BAR por vendor
    (detectar por medição, lição AMD SESSION_252 §9).
  - Mecânica GPFIFO/pushbuffer/doorbell: adotável como DESENHO; offsets e
    encodings do texto REJEITADOS até fonte (envytools/open-gpu-kernel) +
    HW — mesma classe dos hex errados Intel (SESSION_355/392).
  - Pipeline 1–5: MAPEIA EXATAMENTE a escada ADR-0112
    (Off→Mapped→Streams→ComputeDevice); execução por estágio com canário.
  - Código de exemplo: 4 bugs reais — (1) `0xGSP_READY_MAGIC` nem compila;
    (2) polling sem timeout viola SESSION_354 (budget TSC + `false`, nunca
    loop eterno); (3) `submit` sempre na entry 0 sem avanço de anel +
    `put_index` sem teto (estoura `commands[1024]`); (4) BAR1 assumida como
    VRAM inteira (geralmente é janela/aperture — precisa windowing).
- **Impacto no formato v7: NENHUM** (driver é runtime/HAL; blobs ficam na
  FAT, não no `.v6`; o contrato compute — shapes/dtypes/scales — já é o
  que o arquivo carrega).
- **Falsificação:** canários por estágio em HW real (Pascal primeiro: sem
  GSP, blobs presentes; Ampere depois, com blob buscado). QEMU não valida
  (sem emulação NVIDIA) — AWAITING_HW como sempre.

## Como propor um estudo

Traga: o que muda, ganho esperado e **como falsificar** (comando do rig +
critério numérico). Sem critério falsificável, o estudo entra como
HYPOTHESIS e não toca código (ficha anti-complexidade, ADR-0113).
