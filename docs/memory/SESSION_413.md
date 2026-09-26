# SESSION_413 — plano de testes de perf (worker ternario / decode) + T2 em ataque

**Motivo:** a s412 fechou o tile de colunas (1,9x, commit `62899c0f`) e refutou o AVX2 (12x pior). O que sobra e **overhead** — o `sync_us` do SMP (~4,2 ms fixo por dispatch) e os tiny f32 (14x `m=8 k=8 n=256` a ~4,2 ms para 16K MACs) — nao a ALU. Este doc e o plano; os resultados entram conforme as runs.

## Bancada (fixa)
- `tools/lab-llm-response.ps1 -SkipBuild -NoDisplay` — LINJ @0x2100000 + `target1/FALCON3_1B.V6`, QEMU WHPX 6G/4c, prefill **m=8** (18 camadas), decode **m=1**.
- Metricas: `matmul exit m=8 k=2048 n=8192 ... worker_max_us=/sync_us=`, `matmul exit m=1 ...`, `LayerDiag sum` (1a linha = prefill), `prefill_step exit ... us=`.
- **Controle obrigatorio:** baseline v1.9.99-s412 = worker **105 ms** / prefill **12,7 s** / sync **1,4 ms**. Se nao reproduzir (203 ms / 20,9 s = pre-s412), a run nao conta.
- Higiene: 1 variavel por run; matar QEMU orfao ao fim; >=5 GB livres; `cargo check --release` 0 erros antes de buildar.

## Retirado do plano (ja medido - nao repetir)
| Ja testado | Resultado | Ref |
|---|---|---|
| loads 1/peso -> 1/4 (u8 bulk) | **3,23x** OK | `8a769078` |
| u64 bulk (1 load/32) | 0% | s412 |
| LUT f32 (+1 load) | **0,48x** (pior) | s412 |
| `#[target_feature(sse2)]` no kernel escalar | 0% | s412 |
| t-externo (localidade) | 1,20x | s412 |
| **tile de colunas 8->32** | **1,9x** OK | `62899c0f` |
| tile 16 / 64 / 128 | 154 / 130 / 127 ms (pior que 32) | s412 |
| **AVX2 unpack 2-bit** | **12x pior** (nao emite no alvo) | s412 |
| `_mm256_maddubs_epi16` | nao compila (LLVM split) | s249b |
| AVX2/AVX-512 em bare-metal | host-only por design | `bitnet_avx2.rs:8-12` |
| threadpool lock-free / barreira atomica | **ja existe** (`ap_work.rs`) | s411 |
| premissa "decode e bandwidth-bound" | refutada: ~200 MB/s vs ~60 GB/s | s412 |

## Matriz de testes (ordem = valor esperado)
| ID | Hipotese | Mudanca | Metrica | Aceite | Custo |
|---|---|---|---|---|---|
| **T1** | o sync fixo ~4,2 ms e dominado pelo **wake do AP** (hlt->IPI), nao pelo spin | instrumentar TSC no `send_ipi_reschedule` -> 1o worker; comparar hlt vs spin | `sync_us` + TSC por fase | componente identificado; sync < 2 ms | 1 run |
| **T2** | os 14x `m=8 k=8 n=256` (~4,2 ms cada = ~60 ms de dispatch puro p/ 16K MACs) somem com **threshold** | guard de tamanho no `parallel_matmul` (f32): bypass SMP se `m*k*n < 1e6` | `us` dos tiny + LayerDiag | tiny < 0,5 ms | 1 run |
| **T3** | **prefetch** ajuda o stream packed (stride n/4) | `_mm_prefetch(T0)` +2 linhas em `ternary_worker` | `worker_max_us` | >=1,05x | 1 run |
| **T4** | o **RMW de `acc[]`** e o que sobra no bulk u8 | manter o bloco de 4 pesos em **registradores** (sem tocar a stack por peso) | `worker_max_us` | >=1,05x | 1 run |
| **T5** | a **fusao SwiGLU** corta o elementwise do MLP (o repo materializa `gate_t`/`up_t`) | fundir gate/up/silu em `cortex.rs:508/615` | `LayerDiag mlp` | mlp -5% | 1 run |
| **T6** | o **m=1 (decode)** merece tiling proprio | tile/loop dedicado p/ m=1 | `matmul exit m=1 ... worker_max_us` | -10% | 1 run |
| **T7** | **SSE2 vetorizado** (add/sub/skip) no worker SMP ajuda | usar `sse2_ternary_matmul_fill` dentro do worker | `worker_max_us` | >=1,05x | 1 run - **expectativa baixa** (o teste `sse2` deu 0%) |

## Fora deste plano (trilha propria, nao micro-bench)
- **KV-INT8/paginado (#613)** — prereq do Bonsai; mudanca grande, exige desenho + ADR.
- **W2A8 (ADR-0105) / GPU-NPU** — gated / AWAITING_HW; so com HW real.
- **Ceiling de banda** — regua, nao alvo: hoje ~200 MB/s efetivos vs ~60 GB/s -> ~300x de folga.

## Regra de parada
Se T3-T7 derem 0% em sequencia, o worker escalar esta no otimo local **e** o gargalo nao e o worker -> parar e atacar so **T1 (sync)** e **T2 (tiny)**, que sao overhead puro, nao ALU.

## Log de execucao
| ID | Data | Mudanca | Resultado | Veredito |
|---|---|---|---|---|
| T2 | 2026-09-26 | threshold no `parallel_matmul` f32 (`work < 1<<17`) | **NEUTRO (< variancia)** — guard verificado (`tiny_enters=0`, os `m=8 k=8 n=256` sumiram). A/B back-to-back: controle **153 ms / 22,6 s** (`sync_us` 4,3 ms = host carregado) vs T2 **98 ms / 12,3 s** (`sync_us` 1,4 ms = host leve). Na MESMA carga (`sync_us` 1,4 ms): baseline s412 12,7 s vs T2 12,3 s — e o worker (que o T2 nao toca) variou 105->98 ms sozinho. Efeito (<3%) < variancia do host (7-8%). Economia real ~0,5% do prefill (os tiny sao ~14, nao ~280). **Revertido.** | revertido |

| T1 | 2026-09-26 | TSC por fase (`setup`/`bsp`/`bar`) no `parallel_ternary_matmul` + TSC **depois** do log | **PREMISSA REFUTADA** — o "sync fixo 4,2 ms" era o **log de entrada** (o `t_mm0` ficava antes do `slog`, e o split so loga 1/20 -> toda amostra pagava ~1,4 ms de serial). Com o TSC depois do log: `setup` (clear+enq+IPI) = **~60 us** constante (k=2048 n=1024..8192), `sync_us` = **~60-214 us**, e o que sobra e a **cauda do AP** (`bar` = 1 us a 7,4 ms, shape-dependente). Dispatch SMP e **barato**. Mantido o fix do TSC (metrica honesta) + o split no log existente. | mantido (fix) |

## Licoes desta sessao
- **TSC sempre DEPOIS do log (s413, corrige s412).** O `t_mm0` antes do `slog` de entrada fazia o `sync_us` medir o **serial do proprio instrumento** (~1,4 ms) — ~95% do "4,2 ms". Com o TSC depois: `setup` (clear+enq+IPI) = **~60 us**, `sync_us` = **~60-214 us**. Instrumento no caminho de medida mede o instrumento.
- **O que sobra e a cauda do AP, nao o dispatch.** `bar` (= wmax - bsp) = 1 us (k=2048 n=8192) a 7,4 ms (k=2048 n=2048) — imbalanco/wake, so pesa nos matmuls curtos. `setup` e **constante** em ~60 us de k=2048 n=1024 a n=8192: o dispatch nao escala com o shape.
- **Termometro de carga do host:** o `sync_us` antigo distinguia limpo (1,4) de carregado (4,3) — mas porque o **log** e host-sensivel. Depois do fix, o termometro honesto e o **worker ternario** (98 <-> 153 ms) ou a media de n runs.
- **Variancia do host > efeito buscado.** Run-to-run e 7-8% (o worker, que o T2 nao toca, variou 105->98 ms). Efeitos <10% nao sao mediveis sem host controlado ou **n repeticoes (media)** — 1 run nao decide.
- **Absolutos da s412 (105 ms / 12,7 s) foram um momento de host leve.** Nao portar entre sessoes; so as **razoes intra-sessao** (tile 8/16/32/64/128 = 204/154/105/130/127 ms) sao validas.
- **Hipotese "tiny f32 = microssegundos" refutada como ganho:** a economia e ~0,5% (nao os ~8% que a extrapolacao 20x dos logs sugeria). O dispatch tiny existe, mas e pequeno.
- Nao guardar mudanca nao-medida: T2 revertido (3 linhas, re-aplicavel se um dia houver host controlado + n repeticoes).
