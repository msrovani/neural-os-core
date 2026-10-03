# SESSION_437 — Telemetria honesta (recipe/pressure) + resposta degenerada do LLM Falcon3

## Origem
- Log de runtime QEMU 8GB/8c (HUB triage ativo) levantou 3 sinais:
  1. `[slog][warn] sev desconhecida 'recipe' — total=30` (linha invisível).
  2. `{"heap":{"pct":98,"pressure":0,...}}` — pressão não reagia ao heap crítico.
  3. `[LLM] Resposta: " andsfaqtfaqfaqsfaqtfaq..."` — resposta repetitiva/gibberish.

## Fix 1 — telemetria (commit `966edc0f`)
- **`slog::Sev::from_sub`**: `recipe` → `Sev::Ok`. O emissor é
  `k_hal/offer.rs::gate_bind_class` ALLOW (device_recipe promovida = sucesso de
  bind, SESSION_360). Estava em `_ => Sev::Trace` (dmurado) + warn de sev
  desconhecida (ADR-0092). Teste `s410_subs_are_ok` estendido.
- **`allocator::heap_observe` pressure**: o warn proativo usava
  `headroom_bump < 256MB && talc_cap == 0`. A guarda `talc_cap == 0` era do s431
  (TALC fixo); com o TALC sempre claimado **nunca mais disparava** → `pressure`
  só subia via `note_alloc_refused` (2). Fix: `headroom < 256MB` (bump + free
  TALC medido no s435) — o mesmo headroom de `heap_headroom_bytes()`. Honesto:
  no snapshot real (headroom ~6943MB) segue 0; o bug era a métrica **não poder**
  subir mesmo com o TALC esgotado.

## Fix 2 — tokenizer Falcon3 (causa da resposta degenerada)
- **Diagnóstico**: o run tem o **Falcon3-3B real** (`file=989MB`, 22L,
  `bpe=LOADED vocab_n=131072 sp32=0`) — a nota "modelo stub = gibberish" dos
  SESSION_432/433 está **desatualizada** para este run.
- **Causa-raiz**: `bpe::encode` para não-SP32 caía em `encode_chat_frame`, que
  para prompt não-saudação devolve um **frame-cue Llama-3 fixo de 6 tokens**
  (`[bos, 1919, eot, 128006, 78191, 128007]`) e **NUNCA tokeniza o texto**.
  Prova: `a2_proof setup ... prompt_len=6` para prompt de **581B**; saída
  **idêntica** entre prompts/runs (`boot_mon.txt:3708`, `boot_a2proof_bpe.txt:2724`).
- **Fix** (`crates/cortex/src/bpe.rs`):
  - `is_falcon_bytelevel()` = `!is_sp32() && bos < 1000` (Falcon3 `bos=<|startoftext|>=10`
    vs Llama-3 `bos=128000`).
  - `encode_falcon_chat()` = `BOS + encode_bytelevel("<|user|>\n{prompt}\n<|assistant|>\n")`
    (template Instruct real do `tokenizer_config.json`; validado contra o tokenizer
    HF: o frame dá **26 tokens**, não 6).
  - `encode` roteia Falcon3 para lá (greetings incluídos).
- **Bug secundário** (`crates/cortex/src/cortex.rs`): `slim_prompt_tokens_for_heavy`
  (branch BPE) usava `truncate(8)` = **cabeça**, contradizendo o doc ("keep only
  last few tokens") e o branch não-BPE → descartava o `<|assistant|>` final.
  Fix: mantém a **cauda** (últimos 8).

## Validação
- `cortex` **118/118** testes (`-p cortex --lib -- --test-threads=1`); 2 novos:
  `falcon_chat_encodes_prompt_not_cue_frame`, `slim_prompt_keeps_tail_not_head`.
- Tokenizer HF real (`target1/falcon3/tokenizer.json`): frame → 26 tokens e
  decode round-trip correto.
- `cargo check --release` → **0 erros**; `k-nano` release check 0 erros.

## Lições
- **Prompt descartado em silêncio = resposta determinística.** Se todo job recebe
  o mesmo input, a saída é constante — o sintoma "gibberish" era a assinatura de
  um `encode` que não tokeniza (frame-cue heurístico), não de um modelo ruim.
  Medir `prompt_len` vs tamanho do prompt é o teste decisivo.
- **Frame-cue de outro tokenizer é contaminação silenciosa.** O `encode_chat_frame`
  (Llama-3 128k) era aplicado ao Falcon3 131k; IDs 128006/78191/128007 existem no
  vocab Falcon3 como peças arbitrárias → nenhum erro, só lixo.
- **Guarda de transição vira código morto depois da mudança de arquitetura.**
  `talc_cap == 0` (s431) sobreviveu ao claim do budget completo → warn proativo
  inalcançável; medir a métrica que ela deveria produzir (pressure nunca 1)
  revelou o gate morto.
- **Doc de sessão envelhece.** "Stub esperado em QEMU" (s432/433) contradizia o
  próprio log (`file=989MB`). Conferir o artefato carregado antes de culpar o
  modelo.

## Residuais → ideias
- `MAX_CHAT=8` limita o contexto do prompt (prefill ~8,7s/token medido) — subir
  é tradeoff de perf; o HUB triage (timeout 60s) ainda estoura por lentidão.
- WIP s437 **não commitado** (event-bus bounded lock, `bei` ptr-validate,
  allocator shrink-copy) pertence a outra frente; não tocado aqui.
