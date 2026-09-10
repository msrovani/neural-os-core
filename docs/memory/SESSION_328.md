# SESSION 328 — Full Infer D+B+C (Falcon3 off BSP)

**Data:** 2026-09-09  
**Escopo:** Inferência não-bloqueante — fila + Compute + yield/stream/TTS.  
**ADR:** 0057 WS-H.

## Premissa

BSP (Display, voz, input, mixer, Hermes orquestração) **nunca** chama `generate_*`
dentro do tick. Falcon3 roda em slices na InferQueue; UI respira a cada token.

## Camadas

| Camada | O quê |
|--------|--------|
| **D** | `cortex::infer_queue` — submit/claim/cancel; CortexAgent submit-only |
| **B** | `infer_worker` ring1 + `ap_idle_loop` → `poll_slice` sem `AGENT_TICK_BUSY`; matmul AP via `ap_pollable` |
| **C** | 1 token/slice; `LLM_STREAM` MSG_DELTA; `INFER_TTS_PARTIAL` por frase; barge-in cancela |

## Arquivos

- `crates/cortex/src/infer_queue.rs` (novo)
- `crates/hermes/src/agents.rs` — CortexAgent + InferWorker
- `crates/k_nano/src/smp/{mod,ap_work}.rs` — `install_infer_poll_fn` / `notify_idle_wake`
- `crates/jarbas/src/audio/{jarvis,settings}.rs` — TTS parcial + cancel
- `crates/neural-kernel/src/{main,agents}.rs` — register + affinity

## Aceite

1. `cargo check -p cortex -p hermes -p jarbas -p neural-kernel --release` 0 erros
2. Host: `cargo test -p cortex infer_queue`
3. QEMU/metal: orb anima durante generate; TTS parcial antes do `LLM_RESPONSE`

## Residual + decisão (2026-09-09 — maintainer OK)

Pergunta: alguma residual entra agora, GPU/NPU em especial?

| Residual | Incluir agora? | Por quê |
|----------|----------------|---------|
| **Prefill layer-by-layer (AirLLM)** | Não neste entregável; **sim como sprint seguinte** | Decode já yielda 1 token/slice; prefill ainda é 1 slice grande → hiccup no início do prompt. Fatiar exige resume por layer em `forward_with_kv`. Útil e honestamente próximo — **não** é aceleração GPU. |
| **GPU/NPU W2A8 no mesmo InferJob (WS-D/E)** | **Não** | Problema ≠ D+B+C. Fila resolve **agendamento UI**; GPU/NPU resolve **throughput**. `dispatch_ternary` já prefere NPU→GPU→CPU se registrados — o InferJob **já** passa pelo dispatcher. Falta backend: canário GPU `Ready` + KernelPack assinado; NPU = driver/firmware (hoje só PCI+veredito). “Ligar agora” = seam/log **sem ganho**. Aceite metal de UI **não** prova aceleração. Sprint Layer S/HW própria. |
| **`agent_tick_offload_safe = true`** | **Não** | Lock global `AGENT_TICK_BUSY`: offload genérico de agents reabre freeze Display. Só InferQueue/`poll_slice` fora do lock longo. Exige lock por-agent antes de flip. |

### Confusão crítica — dois W2A8

- **CPU W2A8** (`cortex::bitnet_w2a8`, ADR-0084 F4): matmul AVX2 int8×ternário **na CPU**; gated `GENERATION_GAPS_RESOLVED` + stub no `x86_64-unknown-none` (SSSE3/LLVM). Ligar ≠ WS-D.
- **GPU W2A8** (ADR-0057 WS-D): kernel **no device** + KernelPack. AWAITING silício/toolchain.

### Aceite metal (escopo real)

orb/mic vivos durante Falcon3 + 1ª frase TTS antes de `LLM_RESPONSE` — **não** exige prefill zerado, nem GPU/NPU, nem RQ genérica.

## Roadmap sugerido pós-s328 (priorizado)

1. **Aceite metal** Alienware — evidência orb/mic/TTS parcial **antes** de features novas.
2. **Prefill layer-yield** (AirLLM) — sprint seguinte se hiccup doer (não é GPU).
3. **Micro-opts:** telemetria `prefill_ms`/`tok/s`/`worker`/`queue_depth`; budget TSC por slice; prioridade fila chat>healing; confirmar AP faz `poll_slice` (`ap_pollable`).
4. **Produto:** barge-in E2E; orb “thinking” em `LLM_STREAM`.
5. **Layer S depois:** GPU/NPU Ready+pack; `agent_tick_offload_safe` só com lock por-agent.

Ordem: aceite metal → prefill yield → métricas/budget → Layer S.
