# SESSION_414 — ADR-0111 P0-P3: KV-INT8/paginado implementado (3,76x)

**Motivo:** implementar o ADR-0111 (KV-INT8/paginado, IDEA #613) — a unica alavanca estrutural desbloqueada (nao depende de HW). Fases P0-P3 do ADR.

## P0 — instrumento + a evidencia
- `pub const fn kv_bytes_f32(num_layers, ctx, kv_dim)` + `KvCache::bytes_used()/bytes_allocated()`.
- Teste `kv_size_1b_ctx4096_f32` asserta **603.979.776 B** (`18L x 4096 x 1024 x 2 x 4`) = o KV do Falcon3-1B em ctx 4096 em f32 — **maior que o modelo** (`FALCON3_1B.V6` = 544 MB). No 3B (22L) ~738 MB.

## P1 — KvCache INT8
- `k`/`v` de `Vec<Vec<f32>>` -> `Vec<Vec<i8>>` + `k_scale`/`v_scale: Vec<Vec<f32>>` (uma escala por bloco de `KV_BLOCK = 64`).
- Quant simetrica (`scale = max|x|/127`, `q = round(x/scale).clamp(-127,127)`), dequant `x = q * scale`.
- `k_all`/`v_all` dequantizam com a **MESMA assinatura** e o **MESMO guard SESSION_351** (nunca unwrap; mismatch -> pad + `slog` + `Tensor::zero`).
- `h2o_evict` adaptado (norma do K dequantizado; re-quantiza ao compactar) — **assinatura intacta**.
- **3,76x menor**, nao 4x: a escala f32 (4 B) por 64 valores conta (4096/1088).

## P2 — storage paginado
- `pub const KV_PAGE: usize = 4096` valores i8 (64 blocos) por pagina; `KvPageList { pages: Vec<Box<[i8; KV_PAGE]>>, used }`.
- Crescimento **anexa pagina nova so quando a atual enche** — paginas existentes nunca realocam (evita o `memcpy` de ate ~152 MB no crescimento do `Vec`).
- `bytes_allocated` = `page_count() * KV_PAGE` + escalas (sem capacidade sobrando). `.len()` segue = **contagem de valores** (o `soft_stride` depende disso).

## P3 — integracao + instrumento de runtime
- `infer_queue.rs`: no fim do prefill, log `KV ok kv_mem id=.. ctx=.. int8_used=..KB int8_alloc=..KB f32_would_be=..KB` (1x/job) — o numero REAL em runtime (nao estimado).

## Verificacao
- `cargo test -p cortex --lib kv` -> **6/6** (`kv_size_1b_ctx4096_f32`, `kv_int8_roundtrip_parity`, `kv_int8_bytes_4x_smaller`, `kv_pages_cross_boundary_roundtrip`, + os 2 `bughunt_s353`).
- `bughunt_s353::kv_k_all_mismatch_no_panic_pads` — **corrigido**: assertava exatidao f32 (`< 1e-5`), impossivel com INT8 (o bloco `[1,2,3,4]` tem escala `4/127` -> `1.0` volta `1.00787`); a INTENCAO (sem panic, shape, pad zero) preservada com a tolerancia do bloco.
- `cargo check --release` -> **0 erros**.

## Residual
- Medicao no lab: `attn` por ctx (1K/2K/4K) + o `kv_mem` real no QEMU (nao e metrica de perf -> host carregado nao invalida).
- O `KvPages` de `kv_h2o.rs` segue como helper de contagem; o storage e o `KvPageList` (o ADR dizia "integrar"; a contagem e o que o `KvPages` fazia).
- Ganho e de **memoria/contexto longo** (a atencao e ~2% do prefill) — nao de tok/s.
