# SESSION_412 — ternary_worker SMP: sweep do tile de colunas (1,9x) + AVX2 refutado

**Motivo:** fechar a alavanca do prefill/decode do lab Falcon3 — o worker ternario SMP (`parallel_matmul.rs`). A s411 deixou "prefill 3B ~7 s/layer (SMP)" como residual e o bulk u8 (commit `8a769078`, 3,23x) como ultimo ganho medido. Esta sessao separa as duas variaveis que faltavam: o **tile de colunas** e o **unpack SIMD**.

## Bancada (canonica)
`tools/lab-llm-response.ps1 -SkipBuild -NoDisplay` — LINJ @0x2100000 + `target1/FALCON3_1B.V6` (k=2048 n=8192 = FFN do 1B), QEMU WHPX 6G/4c, prefill m=8 (18 camadas).
Metricas: `matmul exit m=8 k=2048 n=8192 ... worker_max_us=` e `LayerDiag sum` (1a linha = forward de prefill).
**Controle:** o baseline reproduz o numero do maintainer (202 ms / 20,9 s vs 203,7 ms / 21,72 s) -> bancada validada, A/B legitimo.

## Sweep do piso do tile de colunas (o ganho)

| piso | worker | prefill |
|---|---|---|
| 8 (baseline) | 202 / 220 ms | 20,9 s |
| 16 | 154 ms | 19,8 s |
| **32** | **105 ms** | **12,7 s** |
| 64 | 130 ms | 17,3 s |
| 128 | 126 / 128 ms | 18,3 s |

**Causa-raiz:** `matmul_tile_rows(k,n)` e formula de tile de **LINHAS** aplicada a **COLUNAS** — devolvia 4 e o `clamp(8,..)` fixava 8 colunas = **2 B usados por linha de cache (64 B)**. Com 32, 8 B/linha; e o strip packed (`k/4` B por coluna = 16 KB) cabe no L1 **junto** do `x` (8 KB) = 24 KB de 32. 16 amortece pouco o load de `x`; 64 ja estoura o L1 (32 KB + 8 KB); 128 vai pro L2.

**Fix:** `clamp(8,256)` -> `clamp(32,256)` (1 linha). **1,9x no worker / 1,65x no prefill.** Curva em U com minimo em 32 — nao e "quanto maior melhor", e fit.

## AVX2 unpack 2-bit — refutado (e 12x PIOR)
Hipotese herdada da s411: depois do bulk u8 o gargalo seria a ALU escalar de descompactacao (`>>`+`&`+`match` por peso), atacavel so com SIMD. Implementado `tern_tile_avx2` (`#[target_feature(enable="avx2")]`, 32 colunas = 8 bytes, extracao 4-grupos, acumulador em registrador). Compila para o alvo e passa teste host — mas no alvo: **2536 ms** (12x PIOR que o baseline) e prefill **191 s**.

**Causa-raiz:** `bitnet_avx2.rs:8-12` ja documentava — AVX2 e **host-only** (`not(target_os="none")`); bare-metal delega ao SSE2. `#[target_feature(enable="avx2")]` no alvo soft-float **nao emite AVX2**: o `rustc-LLVM ERROR: Do not know how to split the result of this operator!` (vpermd) era a pista — 256-bit cai em escalar e cada `f32` vira **libcall**. **Nao reintroduzir.** (Patch do codigo em `%TEMP%\avx2-unpack.patch`.)

## Sync SMP e custo FIXO (~4,2 ms por dispatch)
Nos DOIS caminhos — ternario (`workers=4 ... sync_us=42xx`) e f32 (`matmul exit ok=true us=42xx`) — o `sync_us` fica em **4,1-4,3 ms** de `k=2048 n=1024` a `n=131072` (onde e 0,4%). No decode (`m=1 k=2048 n=1024`: worker 2,2 ms) o sync e **2x o trabalho**. 14x `m=8 k=8 n=256` a ~4,2 ms = ~60 ms de dispatch puro para 16K MACs. O tile fix ja atacou esse custo no prefill (sync 4,2 -> 1,4 ms).

**Decisao (ponytail):** NAO implementar "SMP Threshold Bypass" no ternario medio. Os workers dividem **colunas**, entao BSP-solo ~ **4x `worker_max_us`** — em `k=2048 n=1024` isso da ~8,8 ms solo vs 6,4 ms total = **regressao**. Se o sync incomodar, o alvo e um **threshold** nos shapes tiny do f32 (onde o solo e microssegundos), medido — nao um bypass generico.

## Verificacao
- `cargo build --release -p boot` OK; `cargo check --release` **0 erros**; `cargo test -p cortex --lib` OK.
- Lab: 6 runs, todos `VERDICT: PASS: resposta LLM + TTS` (baseline, 16, 32, 64, 128 e o AVX2).
- Diff final: `crates/cortex/src/parallel_matmul.rs` (7+/1-).

## Licoes (registradas em AGENTS.md)
1. Tile de **colunas** nao aceita formula de tile de **linhas** — medir o fit de cache, nao confiar na constante.
2. AVX2 nao emite no alvo soft-float (`target_feature` compila mas nao vetoriza) — bare-metal usa SSE2.
3. `sync_us` do SMP e custo fixo (~4,2 ms): bypass so ganha se o solo for vetorizado; senao, ~4x o worker.

## Residual
- Piso 32 e o otimo do shape medido (1B); shapes muito diferentes (k/FFN 7B, 10B) merecem re-sweep se o ganho importar.
- Threshold nos tiny f32 (k=8 n=256) — nao implementado (decisao acima).
- `nsgdb_bridge.rs` do lane s410m segue WIP (nao tocado).
