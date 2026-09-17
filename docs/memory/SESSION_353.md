# SESSION_353 — Bughunt heap/infer (deep harden)

## Goal
Fechar HIGH/MED/LOW do bughunt T+800 + **refazer em profundidade**: um único caminho de layer seguro, matmul fail-closed, zero unwrap no hot path de inferência.

## Onda 1 (pontual)
1. `Tensor::new` / `is_valid` — f32_zeros fail → `(0,0)`
2. `embed_for_kv` / `apply_one_layer` — mask nunca `(1,1)`; InferQueue escalate
3. `parallel_matmul` → `f32_zeros_2d`
4. soft_stride pad KV; `k_all`/`v_all` sem unwrap
5. GLOBAL_KV reuse real; OOM stamp `infer_worker`

## Onda 2 (profunda)
1. **`forward_with_kv` / `forward_with_kv_all_logits`** → `apply_one_layer` (fonte única).
2. `embed_lookup` / `rms_norm_tensor` — try_reserve + sem unwrap.
3. Dispatch ternário (`bitnet_avx2`/`sse`/`w2a8`) valida `is_valid`; `get_weight` bounds-safe.
4. `predict_hw_v4` / Medusa / federated — Option; capacity checked.
5. Fallbacks Medusa `(1,1)` → `(0,0)`.

## Verify
- `cargo check -p cortex -p hermes -p agent-core --release` — 0 erros
- `cargo test -p cortex --lib --release bughunt_s353` — **12/12 ok**
- relacionados: heap_aios last_ctx_cap, infer_queue submit_claim, difficulty_gate — **6/6 ok**

## Lições
- Patch paralelo em `forward_with_kv` + `apply_one_layer` diverge; unificar.
- Soft-stride sem pad KV = panic latente em `k_all.unwrap`.
- `(0,0)` vazio é válido; shape cheio + data curta não.

## Aberto
Re-teste WHPX saudação T+800 (não rodado nesta sessão).
