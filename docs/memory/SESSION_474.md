# SESSION_474 — Paralelas #692-695 (host experiments)

Data: 2026-10-13. Quatro experimentos host paralelos concluídos.

## #692 Rep-penalty kernel confirm
- **Fix**: `sample_token_coherence()` em `cortex.rs` adiciona `is_forbidden_id(id)` check (linha 4832)
- **Trace**: `argmax_row_hf_vocab()` já aplicava `repetition_penalty()` (1.15) + `is_forbidden_id()` mask; `generate_speculative()`, `infer_queue::run_fine()`, Medusa verification usam este sampler
- **Gap**: `sample_token_coherence()` (COHERENCE_ENABLED, default OFF) usava só `is_special_id` — corrigido
- **Hermes/CortexAgent**: usa kernel sampler via `infer_queue::submit()` → `run_fine()` — sem sampler próprio

## #693 E30 Escalas por linha (Q6_K unembed)
- **bitnet_writer.py**: `write_q6k(vals, rows, cols, per_row=False)` — se `per_row=True`, codifica cada linha independentemente com seus próprios blocos Q6_K (scales por linha)
- **convert_falcon3_to_v6.py**: flag `--per-row-scales` passa `per_row=True` para unembed
- **Formato**: cada linha → blocos de 256, d/scale por bloco da linha (não global)
- **Overhead**: ~16 scales/linha (Q6_K) vs 1 scale global — testar coerência antes de adotar

## #694 E09 Recall probe KV ternário
- **chat_v6.py**: `quantize_kv_ternary()` (absmean, 2-bit trits: -1/0/+1, 4 trits/byte) + `dequantize_kv_ternary()` + `quantize_kv_int8()` (absmax per-tensor)
- **bench_kv_ternary.py**: needle-in-haystack benchmark (500 tokens contexto, 36 perguntas factuais)
- **Métricas**: exact match rate, F1, perplexity, memory bytes, tempo
- **Modos**: fp16 (baseline) vs int8 vs ternary
- **Output**: JSON comparativo com memory ratio vs baseline

## #695 E06 Grammar-draft prototype
- **chat_v6.py**: flag `--draft-verify` + JSON schema state machine
- **Schema exemplo**: `{"type": "object", "properties": {"name": {"type": "string"}, "value": {"type": "number"}}, "required": ["name", "value"]}`
- **Draft**: gera tokens constrained pelo schema (state machine: KEY → COLON → STRING/VALUE → COMMA/CLOSE)
- **Verify**: roda forward completo nos tokens draft, aceita/rejeita por threshold de probabilidade
- **Métricas**: acceptance rate, speedup vs unconstrained, JSON validity rate

## Netstack fix (pre-existing)
- **netstack.rs**: DNS cache TTL (BTreeMap bounded 64 entries, 5min TTL via TSC ticks)
- **Fix**: `crate::tsc` → `k_nano::tsc` + type annotation

## Próximos
- Rodar bench_kv_ternary.py nos 3 modos (fp16/int8/ternary) com Falcon3-1B
- Testar --per-row-scales na conversão 1B/3B e rodar suite Q1-Q6
- Testar --draft-verify com schema JSON
- #696/#697 aguardam ferro alugado/HW maior