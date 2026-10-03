# SESSION_440 — Falcon3-3B 1.58bit: fim do gibberish (slim 32 + bypass Falcon + BPE 131k) + wedge terminal do InferQ

**Data:** 2026-10-03
**Máquina:** QEMU 8c/6GB WHPX (lab) — `logs/boot_whpx_20261003_182030_8c_or2.txt`
**Continuação de:** SESSION_437 (que só trocou o cue 6→8 tokens) e SESSION_438/439.

> A saída sem sentido do Falcon3-3B 1.58bit (`andsfaqt...`, determinística) **não
> era** o tokenizer — era o **slim do prompt** colapsando o input a 8 tokens e o
> **path Falcon** caindo em heurísticas de score/coerência que não se aplicam a
> ByteLevel 131k. Esta sessão fecha a cadeia inteira e o wedge terminal do InferQ.

## Causa-raiz (fecha o residual do #631)

`encode_chat_frame` usava cue Llama-3 fixo de 6 tokens → prompt de 581B colapsava
para `prompt_len=6`. O fix s437 (`is_falcon_bytelevel` + `encode_falcon_chat`) só
trocou 6→8 porque:

1. `MAX_CHAT=8` truncava o frame Instruct (26 tokens no tokenizer HF) para 8; e
2. o **duplo-slim** (`slim_prompt_tokens_for_heavy` + o próprio `MAX_CHAT`) cortava
   de novo.

Resultado: o modelo recebia 8 tokens de um template de 26 → contexto degenerado →
gibberish determinístico. O tokenizer estava certo; o **pipeline de prompt** não.

## Fixes aplicados

### 1. Slim 8→32 + bypass Falcon (`crates/cortex/src/cortex.rs`)
- `MAX_CHAT` 8→**32** (cabe o frame Instruct real).
- `slim_prompt_tokens_for_heavy` **bypassa** o path Falcon (não re-corta o frame já
  montado por `encode_falcon_chat`).

### 2. Vocab 131072 (`crates/cortex/src/bpe.rs`, `cortex.rs`)
- `min(128000)` → `cols` (o vocab real do Falcon3 é 131072, não 128000).
- `recent: Vec<u16>` → `Vec<u32>` (131072 não cabe em u16).
- Removido o filtro `t >= 128000` que descartava tokens válidos do Falcon3.

### 3. Argmax puro no path Falcon (`crates/cortex/src/cortex.rs`)
- Sem `score_piece`/`weather`/`coherence` — heurísticas de desempate que não fazem
  sentido para ByteLevel 131k e enviesavam a escolha. Falcon = argmax puro.

### 4. Contrato `gibberish_stop` (`crates/cortex/src/cortex.rs`)
- Detecta degeneração: `rep 4-gram > 0.6` **||** `distinct-2 < 0.2` **||**
  `piece_len < 3` → `stop=gibberish`.
- **Nunca vai a TTS** (honestidade: melhor silêncio que fala degenerada).

### 5. `MachineCtx{intent_id, slots, ctx_ids}` (`crates/cortex/src/infer_queue.rs`)
- Fala-máquina ao lado do texto, no mesmo job; o **path texto permanece
  byte-igual** (zero regressão para quem só consome texto).

### 6. Wedge terminal do InferQ (`crates/cortex/src/infer_queue.rs`)
- Evidência: `slice_stall id=2 elapsed_us=30003962 (budget=500000us) — wedge,
  terminal` no prefill de 512 toks (matmul 512×3072×3072 ~14s).
- Fix: **chunked prefill 16 toks** + yield + budget **hv-gated**
  (`slice_budget hv=WHPX budget_us=120000000 sandbox=1`).
- Stall persistente **aborta o JOB** (`stop=slice_budget`), **nunca a fila**.
- `set_prefill_chunk_toks` tunável (clamp 1..64, default 16).

## Adjudicação do plano de 5 camadas (ora-2)

| Camada | Veredicto | Razão |
|---|---|---|
| 1A chunk tunável | **adaptado** | chunked prefill 16 toks (fix 6) |
| 1B AVX2 unpack | **rejeitado** | AVX2 já existe em `bitnet_w2a8.rs`, host-only (S412) |
| 1C pinning/budget 50ms | **rejeitado** | budget hv-gated cobre o caso |
| 2A RO 2MB loader + `get_or_mmap_expert` | **adaptado** | implementado + wired |
| 2B arena 512MB/agente | **rejeitado** | custo/benefício |
| 3A flags VirtIO lab-only | **adaptado** | lab-only |
| 3B IP estático 192.168.100.50 | **rejeitado** | config externa, não constante |
| 4 flush oportunista TICKV | **adaptado** | `flush_idle` + high_water |
| 5 skill cache WASM pré-LLM | **adotado** | implementado + wired |

### Implementado + wired
- **2A** `map_loader_region_ro` (main.rs boot scan) + `get_or_mmap_expert`.
- **4** `flush_idle` (main.rs idle closure) + high_water.
- **5** `skill_cache_try_register` (agents.rs pré-LLM — antes era DCE'd).

## BPE canônico por modelo

`models/bpe_vocab.bin` / `target1/bpe_vocab.bin` eram **SP32 32K**
(`vocab_n=32002`) — **errado** para o Falcon3 (131072). Gerado
`target/falcon_bpe.bin` via `tools/export_bpe_bin.py target1/falcon3/tokenizer.json`
(`vocab_n=131072 bos=10 merges=128810`) e copiado para `target/bpe_vocab.bin`
(canônico do `find_bpe()`).

## Verificação

- `cargo check --release` **0 erros**.
- Testes: cortex **126/126**, k-nano **251/251**, hermes **287 + 1 falha
  PRÉ-EXISTENTE** (`permission_gate::test_risk_level_classify`, provado via stash:
  282/1 sem o diff).
- **QEMU 8c/6GB WHPX** (`logs/boot_whpx_20261003_182030_8c_or2.txt`):
  - `loader RO ... 1024x2MB ro skip=0` (2A wired);
  - `skill_cache miss name=hw_pnp_pci_bridge...` (5 wired);
  - `prefill_chunk 1/32→3/32` **sem wedge** (fix 6);
  - BPE **131072** (vocab correto).
- **Imagem HW:** `PACK_LLM=all python tools/build_image.py --hw --unified --size 15360`
  → `target/usb_hw.img` (~14GB): 4 Falcon ×2 aliases + BGE/E5/RERANKER/RUSTCDR3/
  AGENT/LEARNER/PIPER/STT/VISION/HWEXPRT + firmware + BPE 131072.

## Lições

- **Prompt descartado em silêncio = resposta determinística.** Medir `prompt_len`
  vs tamanho do prompt é o teste decisivo (reforço S437).
- **Duplo-slim é armadilha:** dois truncamentos no mesmo caminho (MAX_CHAT +
  slim) colapsam o frame sem log. Um só ponto de corte, explícito.
- **Vocab é contrato do modelo, não constante global:** `min(128000)` e `Vec<u16>`
  eram premissas de Llama; Falcon3 = 131072 → `cols` + `Vec<u32>`.
- **Heurística de score não se aplica a ByteLevel 131k:** argmax puro no path
  Falcon; `score_piece`/`weather`/`coherence` enviesavam a escolha.
- **Wedge de slice ≠ fila morta:** stall persistente aborta o JOB
  (`stop=slice_budget`), a fila segue viva; budget hv-gated (WHPX 120s) evita
  falso-positivo no prefill longo.
- **BPE canônico por modelo:** o `find_bpe()` apontava para um SP32 32K genérico;
  cada modelo ByteLevel exige o seu `bpe_vocab.bin` (131072 p/ Falcon3).

## Arquivos de código
`crates/cortex/src/cortex.rs`, `crates/cortex/src/bpe.rs`,
`crates/cortex/src/infer_queue.rs`, `crates/neural-kernel/src/main.rs`,
`crates/hermes/src/agents.rs`, `tools/export_bpe_bin.py`.
