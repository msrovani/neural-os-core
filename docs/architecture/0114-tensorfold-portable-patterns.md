# ADR-0114: Padrões portáveis do TensorFold — quantização affine4, contratos de exatidão e aprendizado persistente

- **Status:** Accepted (registro de padrões de REFERÊNCIA — não implementa código; ADR-0113 governa)
- **Lifecycle:** `fazendo` (doutrina de referência a partir de 2026-10-10)
- **Origem:** análise externa do repo `ashhart/TensorFold` via @librarian (2026-10-10) + diretriz do maintainer; IDEA_BANK #682–#686
- **Fonte:** https://github.com/ashhart/TensorFold (Apache-2.0, v1.0.4, 1.2k stars, 1.080 commits, ativo em 2026-10-10)

## Contexto

TensorFold é um engine de inferência LLM em **Zig 0.17** (~200K LOC + ~1.75MB de
shaders Metal) para Metal/CUDA/Vulkan — binário userspace autocontido com **zero
dependências**, API compatível com OpenAI/Anthropic. Lê checkpoints MLX affine
4/6-bit (safetensors), faz speculative decoding (DFlash2, MTP head) e serve MoE
(Nemotron 30B-A3B).

**Não é utilizável como dependência:** userspace (threads, alloc dinâmico,
filesystem, linkage de runtime GPU), assume OS com driver stack — não compilaria
para `x86_64-unknown-none`. O valor para o `neural-os-core` é exclusivamente de
**padrões portáveis**, no estilo ecosystem-analysis.

## Decisão

Adotar como **padrões de referência** (não código) os padrões abaixo, mapeados
à premissa AIOS (ADR-0088), à missão (ADR-0113) e ao substrato nsgdb. Todos os
padrões entram na taxonomia ADR-0113 como `REFERENCE`/`HYPOTHESIS` — **nenhum
é `IMPLEMENTED`**. Qualquer adoção futura passa pela ficha anti-complexidade
(ADR-0113) + sweep medido antes de tocar o kernel.

### 1. Affine4 quantization + lane-tiling de GEMV quantizado (IDEA #682)

- **O que é:** quantização affine 4-bit por grupo (64/32) com `affine4_lane` —
  lane-split do GEMV quantizado.
- **Aderência AIOS:** mesmo espaço do BitNet ternário/W2A8 (ADR-0112). A lição
  SESSION_412 aplica-se diretamente: constante de tiling vale para o eixo que
  ela descreve — medir o fit (L1), não reusar a do outro eixo. O overhead de
  escala por grupo é o mesmo trade-off do KV INT8 (SESSION_414: contar o
  metadado, não só os valores — 4096/1088 = 3,76x, não 4x).
- **nsgdb:** registrar como `mom/pattern` (lane-tiling de GEMV quantizado).
- **Status:** `HYPOTHESIS` — estudar `affine4_lane` para informar o tiling do
  `ternary_worker`/W2A8. Nenhum código agora.
- **Como falsificar/validar:** sweep de tiling no QEMU (padrão SESSION_412:
  Falcon3-1B, medir ms por tile) antes de qualquer mudança; ganho <10% não é
  mensurável com 1 run (SESSION_413).

### 2. EXL3 format parseado in-engine (IDEA #683)

- **O que é:** formato de quantização de terceiros (EXL3) parseado dentro do
  engine, sem dependências externas.
- **Aderência AIOS:** reforça a doutrina de contrato de loader — GGUF type IDs
  são contrato ggml, não enum livre (SESSION_309/358); header autodescritivo
  (`v6_file_size`); "formato de export é contrato com o loader" (SESSION_247).
- **nsgdb:** `mom/pattern` (formato externo parseado in-engine).
- **Status:** `REFERENCE` — referência para suportar formatos de quantização
  externos no Cortex. Só com demanda real (YAGNI — ADR-0113 §3); BitNet
  ternário v6 permanece canônico.

### 3. Contrato de decodificação exata: resumed == fresh, concurrent == solo (IDEA #684)

- **O que é:** todo draft especulativo aceito deve igualar o que o mesmo engine
  produziria sem draft; requests retomados/concorrentes devem igualar execução
  fresh/solo. Formalizado em suites de teste (`kernel_exact`, `qwen35_exact`).
- **Aderência AIOS:** análogo formal aos predicados E1/E4 (ADR-0113) —
  `benchmark_sem_reprodutibilidade=UNKNOWN`; determinismo como propriedade
  demonstrável, não acidental.
- **nsgdb:** frente SGDB canônica — `behavior(t2)=f(recalled_state)`; memória
  restaurada (checkpoint/Tickv) NÃO pode mudar o resultado do decode. O
  contrato de exatidão é o teste que prova isso.
- **Status:** `REFERENCE` — contrato de determinismo para o decode do Cortex
  (Medusa/MTP) + testes de paridade. Não temos speculative decoding em
  produção — nada a implementar agora; adotar o contrato QUANDO entrar.

### 4. Living Weights — fatos aprendidos reescritos no checkpoint em disco (IDEA #685)

- **O que é:** ensina fatos ao modelo em execução reescrevendo os arquivos de
  checkpoint no disco — fatos sobrevivem a restart e são portáveis entre
  máquinas servindo a mesma pasta.
- **Aderência AIOS:** premissa 3 (memorização, aprendizado, versionamento) e
  frente SGDB (`behavior(t2)=f(recalled_state)`; `memory_exists ≠ aprendizado`).
- **nsgdb:** complemento, não substituto — o SGDB registra proveniência/audit
  do aprendizado; Living Weights é o MEIO de persistir no modelo. Aprendizado
  sem proveniência = violação fantasma (SESSION_410: I3 judge antes do dado é
  violação fantasma — o registro de proveniência vem ANTES da mutação).
- **Status:** `HYPOTHESIS` — estudar como alternativa/complemento para
  aprendizado persistente de experts. **Conflito a resolver antes de qualquer
  adoção:** reescrever checkpoint = mutação de artefato versionado (o hash do
  modelo muda) — exige versionamento do checkpoint + trilha de auditoria nsgdb
  + HITL para mutação de pesos (segurança primeiro, ADR-0088). Nunca mutar o
  artefato que a imagem bootada carrega sem registro observável.
- **Como falsificar/validar:** ablação — boot 1 aprende fato → power cycle →
  boot 2 verifica o fato E o hash versionado; sem trilha de auditoria = não
  adotar.

### 5. Fix de race DMA — upload pageable retorna antes dos bytes pousarem (IDEA #686)

- **O que é:** último commit do repo (2026-10-10): `DeviceBuffer.upload` passa a
  retornar só quando os bytes pousaram (race achado por teste flaky 8/80
  falhando).
- **Aderência AIOS:** **VALIDAÇÃO** das lições existentes — `sfence` (CPU) ≠
  flush de dispositivo (SESSION_252); frame allocator/DMA sobrescreve memória
  viva (SESSION_463); timeout que retorna sucesso é pior que hang (SESSION_354).
  Terceira ocorrência independente do mesmo padrão em engines diferentes.
- **nsgdb:** `mom/anti-pattern` (I/O que reporta conclusão antes do efeito
  pousar).
- **Status:** `OBSERVED` (alinhamento) — nada a implementar.

### 6. Disciplina zero-dependência + checkpoint/mmap (referência transversal)

- **O que é:** engine inteiro (HTTP server, tokenizer, runtimes) em Zig puro
  sem package deps; `paced_read`/`direct_io`/`fence` no I/O de checkpoint; split
  `checkpoint_host` vs `checkpoint_metal`.
- **Aderência AIOS:** política de higiene no-new-deps; dual-truth de storage =
  segunda implementação que diverge (SESSION_410); I/O fail-closed (SESSION_354).
- **nsgdb:** `mom/pattern` (self-contained engine; dual-truth evitada por split
  explícito host/device).
- **Status:** `REFERENCE`.

### 7. O que NÃO é adotado

- **O engine em si** (Zig userspace, threads, GPU driver stack) — incompatível
  com `no_std` bare-metal.
- **Kernels Metal/CUDA** — nosso caminho de compute é SSE2 soft-float (bare-metal)
  e W2A8 via BAR (ADR-0112); AVX2 refutado no alvo (SESSION_412).
- **EXL3/affine4 como formato de pesos do projeto** — BitNet ternário v6
  permanece canônico; formatos externos só como leitura sob demanda, nunca
  como formato de export.

## Evidência e honestidade

- Todos os padrões = `REFERENCE`/`HYPOTHESIS`/`OBSERVED` (taxonomia ADR-0113) —
  **nenhum `IMPLEMENTED`**; este ADR não adiciona código.
- Stats do repo medidos em 2026-10-10 via @librarian (GitHub API); re-medir
  antes de citar números em comunicações futuras (lição SESSION_329/392: métrica
  de doc é cache).
- Falsificação de qualquer adoção futura: ficha anti-complexidade (ADR-0113) +
  sweep medido no alvo + não-regressão declarada.

## Registrada no n-sgdb

`scope=project/neural-os-core`, entities `adr-0114` + `mom/decision`; padrões
individuais como `mom/pattern`/`mom/anti-pattern` (IDEA_BANK #682–#686 como
`mom/fact`).
