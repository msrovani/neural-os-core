# SESSION_351 — OOM fail-closed pós-HeapAIOS + usb_hw full pack

**Sprint:** v1.9.99-s351 · **Bloco:** allocator / cortex forward / HW image · **Data:** 2026-09-15

## Goal

Fechar o OOM UI que ainda passava pelos guards s349/s350; validar WHPX tok/s; gerar `usb_hw.img` com artefatos.

## Evidência

| Run | Resultado |
|-----|-----------|
| s350 1º WHPX Window | Bench OK (~0.15 tok/s) mas UI `OOM agente=cortex_llm size≈4764806172` |
| Cap GlobalAlloc 256 MiB | **Falso positivo:** load LLM `size=330301440` (~315 MB) `agente=?` → hlt no parse; measure ficou em `runtime live` sem `LLM LOADED` |
| Cap só `size > bump_window` | Load OK; hit@112s **milli=159** (~0.159 tok/s); `HeapAIOS plan=ok tier=cheap ctx=64` |
| Pós-bench T+800 | `CTX user add` → **`capacity overflow`** (`raw_vec`) — aberto (path saudação LLM) |

## Feito

### Fail-closed alloc
- `tensor::f32_zeros` / `f32_zeros_2d` — `checked_mul` + `try_reserve_exact` (não `vec![0; a*b]`)
- `forward_with_kv` / `_all_logits` / `forward_hidden` / máscaras / rope: ctx_cap + `f32_zeros_*`
- `generate_speculative` usa `heap_aios::plan_for` (paridade InferQueue)
- `GlobalAlloc`: refuse **somente** se `layout.size() > bump_max_offset()` (~2 GB); stamp FB `ALLOC refuse`

### HW image
- Stub vazio `target1/FALCON3.V6` removido
- `PACK_LLM=all` + `--hw --unified --build-boot --size 8192`
- Artefato: `target/usb_hw.img` **8319 MB** (ESP + FAT32 8 GB)
- Pack: Falcon3-3B lab (~989 MB) + AGENT/RUSTCDR3/RERANKER/LEARNER/VISION + BGE/E5/PIPER/STT/BPE/HWEXPRT + firmware + LEGO + BOOT.LOG/NSGDB
- Ausentes no host: BITNET850/13/2B/3B, PIPER_EN

## Lições

1. **Guard em `Tensor::new` ≠ cobertura** — `vec![0.0f32; a*b]` e `to_vec` no loader ainda OOMam.
2. **Cap único-alloc baixo (256 MiB) mata load legítimo** — só recusar o impossível (acima da janela bump).
3. **`try_reserve` evita `alloc_error_handler`** — `vec!`/`resize` forçado com null ainda hlt.
4. **`capacity overflow` ≠ OOM/TALC** — panic Rust em mul de capacity; investigar path pós-`CTX user` (T+800).

## Aceite

- [x] WHPX measure hit + LLM LOADED sem OOM 315 MB / 4.7 GB no load
- [x] `usb_hw.img` regenerada (Rufus DD)
- [ ] UI chat sem `capacity overflow` / OOM após saudação (aberto)

## Próximo

- Bisectar `capacity overflow` em T+800 (prompt saudação / cortex_system_prompt / InferQueue)
- InferWorker `tick_in_progress` próprio
- HUD headroom vs “ample RAM”
