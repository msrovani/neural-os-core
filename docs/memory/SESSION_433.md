# SESSION_433 — HUB triage: proposta via LLM (heurística = fallback) + gates HITL (s433)

## Origem
- Evolução do hub_triage (s432/#629): sair da heurística worst-state para o LLM GERAR a
  proposta acionável — premissa máxima ADR-0088 (a IA decide, o humano aprova via HITL).

## Implementação (`crates/hermes/src/hub_triage.rs`)
- **Fluxo:** `Propose` (heurística) → gate duplo (`should_try_llm` = `model_is_loaded` +
  `!heap_headroom_low`) → `infer_queue::submit(prompt, Plain, TOPIC_HUB_TRIAGE_LLM)` →
  prompt único (`TRIAGE_PROMPT_HEADER` + snapshot JSON; instrução: responda
  `{"title":..,"action":..}` ou `{}`) → resposta do modelo no reply_topic →
  `decide_llm_reply` → Publish/Decline/Fallback.
- **Parser sem serde** (`json_string_field`): scanner minimalista com escape `\"`/`\\`,
  whitespace, `from_utf8_lossy` (acentos PT-BR sobrevivem), CAP 256B/campo + limite
  title 64 / action 256 (resposta de modelo não-confiável).
- **Decisões:** `Publish` → `publish_proposal_hitl("LLM", ...)`; `{}` → `Decline`
  (observe-only, sem toast); `[cancelled]`/`[heap escalate]`/gibberish/vazio →
  `Fallback(reason)` → `publish_proposal_hitl("fallback", título/ação heurísticos)`.
- **Gates:** `publish_proposal_hitl` recusa com `heap_headroom_low` (fail-closed s430);
  snapshot FRESCO no momento da publicação (não o do submit).
- **Anti-loop:** fp heurístico anotado NO SUBMIT (reserva — 1 ciclo/min não re-submete
  job em voo); fp da ação GERADA anotado na publicação; cooldown 10min inalterado.
- **Agente:** `llm_receiver` drena `HUB_TRIAGE_LLM` em qualquer tick;
  `has_pending` inclui receiver pendente + timeout (lost-wakeup s411);
  `LLM_REPLY_TIMEOUT_TICKS=3600` (60s) → fallback com `publish_proposal_hitl("fallback-timeout")`.
- Contadores novos: `HUB_TRIAGE_LLM_SUBMITTED`, `HUB_TRIAGE_LLM_FALLBACKS`.

## Validação
- hermes 273/273 (-t1; 16 testes hub_triage, 7 novos: parser, escapes, declínio,
  marcadores, gates, prompt, timeout); `cargo nk` 0 erros (45s); build boot 1m16s +
  build_image; strings provadas no uefi.img (`HUB_TRIAGE_LLM`,
  `Voce e a IA de auto-diagnostico`).
- **QEMU 8GB/8c (`logs/boot_whpx_20261001_132152.txt`):** triagem ok 5× (T+677..21571) →
  `observe: sched lag` (T+25314) → **`proposta via LLM submitted id=4` (T+29433)** →
  **fallback-timeout honesto (T+33118, 3600 ticks)** → toast + intent HITL → Jarbas →
  Hermes → MoE → LLM (T+33860, snapshot JSON completo no intent). `dedupe (cooldown)`
  provado (T+35110). Modelo stub = gibberish (esperado em QEMU).
- **Residual agravado (3ª sessão):** stall silencioso pós-`OOM/TALC size=146
  agente=infer_worker` — log congela em T+33860 com QEMU vivo (RAM 4.3GB, serial muda,
  sem #PF novo). O stall "CPU 100% sem log" da s419, agora com evidência melhor.

## Lições
- **Fallback de timeout deve reservar o fp do veredito heurístico** (o job id=4 ficou
  atrás da cauda a2_proof de 60s e estourou 3600 ticks — com a reserva, o timeout caiu
  no dedupe como previsto).
- **Resposta do LLM pode ser "[heap escalate]..."** — os marcadores de controle do
  InferQueue são conteúdo no reply_topic; o parser precisa distingui-los (senão o
  fail-closed do heap vira "proposta").

## Residuais → ideias
- #630 (novo): instrumentar path de overflow do TALC (`TALC_OVERFLOW_NULL` /
  `PF_DIAG_*` / talc feature `counters`) — causa do OOM `infer_worker` + stall
  silencioso desconhecida há 3 sessões.
- Proposta LLM e2e com modelo real (Falcon3 1B/3B) — depende de tokens legíveis.
