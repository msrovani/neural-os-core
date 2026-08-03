# ADR-0083: Gap Camada de IA — Auditoria 7.x, Correções e Dívida Remanescente

**Data:** 2026-08-03  
**Fonte:** Auditoria Técnica externa (seção 7 — camada de inteligência artificial)  
**Status:** Accepted — gap documentado formalmente; correções desta sessão registradas; dívida remanescente priorizada

---

## 1. Contexto

A auditoria técnica (seção 7) concluiu que a **infraestrutura de inferência é real e boa**, mas que a
**inteligência que ela deveria servir ainda não existe** — os componentes "neurais" que decidem
(roteador MoE, treinamento on-device, memória semântica, respostas de demo) eram, na configuração
default, ruído com aparência de modelo:

> "Hoje, se o senhor removesse toda a camada neural, o sistema se comportaria quase igual — porque quem decide são as palavras-chave."

Esta ADR registra:
1. O diagnóstico preciso por componente (o que é real vs. o que é teatro)
2. As correções aplicadas nesta sessão (fecho da auditoria 7.2 e 7.5 parcial)
3. A dívida remanescente com caminho de implementação
4. A decisão de política: **nunca logar como "treinado" o que não foi treinado**

---

## 2. Diagnóstico por Componente (antes desta sessão)

| Componente | Situação pré-correção | Evidência |
|---|---|---|
| **Passe adiante transformer ternário** (GQA, RoPE, KV cache, RMSNorm, FFN ternária) | **Real** | `cortex::cortex::TransformerModel::forward_with_kv` |
| **Matmul ternário + SIMD** (ADD/SUB, 4 pesos/byte, AVX2) | **Real** | `cortex::tensor`, `bitnet_avx2.rs` |
| **Carregador GGUF** (Q4_K/Q6_K) | **Real** | `cortex::gguf` |
| **Tokenizador BPE** | **Real** (requer vocab externo) | `cortex::bpe` |
| **Roteador MoE "treinável"** | **Ruído LCG seed=42** | `trinity::generate_router_weights` — embedding uniforme + pesos ternários aleatórios determinísticos; log anunciava "Router MoE loaded" |
| **Especialistas MoE** | **Rótulos de dispatch** — sem redes especializadas | `Expert { kind, name, description }` |
| **Treinamento on-device** | **Brinquedo** — regressão linear escalar de 64 pesos, sem retropropagação; `train_task` com inputs=targets=1.0 | `k_ai::cognitive::BitNetTrainer` |
| **Aprendizado federado** | **Brinquedo** — deltas sobre os mesmos 64 pesos de ruído | `k_ai::fl_trainer` |
| **RAG / memória semântica** | **Pseudo-hash FNV-1a** por default (BGE ausente) | `k_ai::memory_systems::pseudo_embed` — honesto no log (`emb=pseudo`) |
| **Respostas demo (saudação)** | **Efetivamente CANNED** — pool de tokens fixo + bias posicional/bigram de até 8.0 | `bpe.rs` `GREETING_BIAS_IDS`, `greeting_step_candidates`, `greeting_position_bias`, `greeting_bigram_bias` |

---

## 3. Correções Aplicadas (esta sessão)

### 3.1 Roteador MoE — pesos carregáveis de arquivo (ADR-0083 §3.1)

`cortex/src/trinity.rs`:
- Novo `load_router_from_file(data: &[u8]) -> bool` — lê `.bitnet` v3+ com tensores nomeados
  `router_embed` (VOCAB×HIDDEN f32) e `router_weight` (HIDDEN×MAX_EXPERTS i8 ternário); valida
  `hidden == ROUTER_HIDDEN`; armazena em statics `ROUTER_EMBED`/`ROUTER_WEIGHT`.
- Novo `init_router_weights(num_experts)` — consome os statics se carregados; senão cai no
  LCG determinístico (fallback explicitamente documentado).

`neural-kernel/src/main.rs`:
- Boot tenta `ROUTER.BITNET` via NVMe → AHCI → ATA → USB-MSC antes do fallback LCG.

**Resultado:** com `ROUTER.BITNET` no FAT, o router usa pesos reais; sem ele, o fallback LCG
permanece mas o log deixa de ser enganoso? **Não** — ver §5 item 1 (política de log ainda pendente
de aplicar ao load_router).

### 3.2 BGE — verificação do wiring de carga (ADR-0083 §3.2)

Auditoria completa: `load_bge` já tenta `BGE.BIN` em NVMe → AHCI → ATA → USB-MSC + scan
QEMU-loader (`0x100000000..0x180000000`). O gap é **falta do asset no FAT**, não de código.
Documentado no README/scripts de imagem como requisito (`BGE.BIN` na partição de dados).
Sem mudança de código necessária — registro aqui para não re-investigar.

### 3.3 Treinamento on-device — esqueleto de backprop (ADR-0083 §3.3)

`k_ai/src/cognitive.rs`:
- `TransformerTrainer` — API completa de treinamento: `forward(model, tokens) -> (Tensor, TransformerCache)`,
  `backward(model, cache, targets) -> TransformerGradients`, `update_weights(model, grads)`.
- `TransformerCache` — guarda `KvCache` + slots documentados para ativações intermediárias
  (post-attention residual, post-FFN residual, RMSNorm in/out, Q/K/V/scores, FFN gate/up/down).
- `TransformerGradients` / `LayerGradients` — estrutura de gradientes para todos os parâmetros
  (embed, unembed, rms_final, por camada: q/k/v/o/gate/up/down/rms_*).
- `backward`/`update_weights` emitem `slog_kai! warn "NOT YET IMPLEMENTED - skeleton only"` —
  **honestidade no log**: nunca anunciar aprendizado que não aconteceu.

**Decisão:** o esqueleto é a espinha dorsal do backprop real; a implementação matemática fica
gated como trabalho futuro (ver §5 item 2). O `BitNetTrainer` (regressão linear) foi mantido como
utilitário simples — mas seu `train_task` com dados constantes foi identificado como anti-padrão.

### 3.4 Saudação — remoção do pool canado (ADR-0083 §3.4)

`cortex/src/cortex.rs`:
- Removido `argmax_row_greeting_only` (pool fixo + bias posicional + bias bigram).
- Caminho de saudação agora usa `argmax_row_hf_vocab` — **argmax real do modelo**, sem pool.

`cortex/src/bpe.rs`:
- Removidos `GREETING_BIAS_IDS`, `greeting_candidate_ids`, `greeting_step_candidates`,
  `greeting_position_bias`, `greeting_bigram_bias`.

**Resultado:** a saudação é gerada pelo modelo com os mesmos logits de qualquer outro prompt.
O `is_greeting` continua controlando apenas `max_gen` (8 tokens) e o early-exit
`text_is_greetingish` — comportamentos de UX legítimos, sem viés de conteúdo.

**Mantido:** o constrained decode de **clima** (`argmax_row_weather_only`, `WEATHER_BIAS_IDS`)
— é saída estruturada (formato fixo de relatório), categoria diferente de saudação livre.

---

## 4. Estado Pós-Correção

| Componente | Status |
|---|---|
| Passe adiante transformer | Real (inalterado) |
| Roteador MoE | Carregável de arquivo (`ROUTER.BITNET`); fallback LCG documentado |
| Treinamento on-device | Esqueleto de backprop honesto (warn no log); regressão linear mantida como utilitário |
| Memória semântica | BGE real se `BGE.BIN` presente; pseudo-hash honesto no log (inalterado — asset é o gate) |
| Saudação demo | Argmax real do modelo — pool canado removido |

`cargo check --release` — 0 erros (só warnings conhecidos pré-existentes).

---

## 5. Dívida Remanescente (priorizada)

### 5.1 Política de log honesto — roteador não treinado não é "loaded" (P0)

**Problema:** `TrinityRouter::load_router` loga `"Router MoE loaded: {} dim, {} experts"` mesmo
quando os pesos vêm do LCG de semente 42 (ruído). A auditoria 7.2 chamou isso de "linhas de log
que atestam algo que não aconteceu".

**Fix:** `load_router` aceita origem (`trained` | `deterministic_fallback`) e loga distinto:
`"Router MoE loaded (trained): ..."` vs `"Router MoE weights: DETERMINISTIC FALLBACK (seed=42, untrained)"`.

### 5.2 Backprop real no transformer (P1) — ADR-0083 §3.3 estrutura pronta

Implementar em `TransformerTrainer::backward`/`update_weights`:
1. Cross-entropy gradiente nos logits
2. Gradientes de unembed/embed e rms_final
3. Por camada: atenção (Q/K/V/O) + FFN (gate/up/down) + RMSNorms
4. Atualização ternária via straight-through estimator (`ternary_update` já existe em k_ai)
5. Gate: exigir `TransformerCache` populado por um `forward` que salve ativações (hoje `forward`
   reusa `forward_with_kv` sem capturar ativações intermediárias)

**Critério de aceite:** um passo de treino sobre uma sequência sintética diminui o loss de
cross-entropy medido (não apenas o log); `train_task` com dados constantes deixa de existir
(substituído por fixture com informação real).

### 5.3 Router treinado de verdade (P1)

- Pipeline host: `tools/train_router.py` — dataset rotulado (ex: prompts conhecidos → expert
  esperado), treina embedding+matriz ternária, exporta `ROUTER.BITNET` no formato §3.1.
- Gate de review: acurácia de roteamento >80% em holdout antes de aceitar o asset no repo.
- O fallback LCG permanece como bootstrap apenas (com log honesto §5.1).

### 5.4 Replay buffer MoE (P2)

`cortex::r3::update_with_replay` já existe; conectar `classify_intent_with_trace` ao buffer de
replay para futuras rodadas de treino §5.3 (auditoria 7.2 citou `unpack_router_weights` como base).

### 5.5 Assets no FAT como requisito (P2)

`BGE.BIN` e `ROUTER.BITNET` são opcionais hoje; o boot degrada com log honesto. Documentar nos
scripts de imagem (`tools/build_image.py`) como payloads opcionais com impacto na capacidade
semântica — sem fingir que pseudo-hash é BGE.

---

## 6. Decisões de Política

1. **Nunca logar como "treinado"/"loaded" o que não foi treinado** — o padrão
   "formato + seed + log otimista" (auditoria 7.2) é o anti-padrão a evitar. Todo componente
   neural deve reportar sua origem real: `trained` | `deterministic_fallback` | `pseudo`.
2. **Constrained decode é legítimo para saída estruturada** (clima, JSON, shell), não para
   linguagem livre (saudação removida nesta sessão).
3. **A camada neural não decide quando o dado não existe** — keyword routing é o fallback
   explícito e deve permanecer, sem pretensão neural. A promessa "o OS é a IA" fica adiada até
   o roteador treinado (§5.3) e o backprop (§5.2) entregarem acurácia medida.

---

## 7. Referências

- Auditoria Técnica seção 7 (7.1–7.6)
- `crates/cortex/src/trinity.rs` (router)
- `crates/cortex/src/cortex.rs` (generate/argmax)
- `crates/cortex/src/bpe.rs` (tokenizador)
- `crates/k_ai/src/cognitive.rs` (BitNetTrainer, TransformerTrainer)
- `crates/k_ai/src/memory_systems.rs` (BGE/pseudo)
- ADR-0033 (on-device micro-learning) — superseded parcialmente por esta ADR quanto à honestidade
- ADR-0047 (latent space AI-OS), ADR-0019 (neural cortex bitnet LLM)
