# SESSION_468 — Falcon3 .V7 + suite Q1-Q6 + bench + kernel v7-reader

Data: 2026-10-13. Saga: converter `FALCON3*.safetensors` em `.V7` BitNet 1.58b com saídas viáveis nos testes de coerência.

## Artefatos

- `target1/FALCON3_1B.V7` 693MB, `FALCON3_BASE.V7` ~1.2GB, `FALCON3_7B.V7` 2219MB, `FALCON3_10B.V7` 2899MB (bytes exatos, 0 buracos).
- Formato v7 = v6-feat8 + provenance (fonte HF, modelo, hashes, comando) + tokenizer BPB1 embutido (merges 128810); readers do kernel aceitam versão 6|7.
- Rig host em `tools/`: `chat_v6.py`, `test_v6_suite.py` (Q1-Q6), `bench_v6.py`, `diag_v6.py`, `triage_v6.py`, `parity_*`, `ref_transformers.py`.

## Bugs raiz do conversor (6, todos provados por paridade)

1. Chaves retangulares zeradas (`q` vs `q_proj`, `o` vs `o_proj`).
2. Scale ignorada; 3. branch float frouxo (unembed ia 100% p/ ternário); 4. scale invertida (BitNet divide: `w = q*scale/127`).
3. Unpack consecutivo vs estridado real (a docstring mentia); 6. unembed ternário → Q6K (MATCH 5/5, 75KB/layer).

## Validação

- Pesos 7/7 idênticos à referência (cos 1,0); forward MATCH; Q6K == denso (fp32==q6k, maxdiff 0).
- Suite 1B.V7: Q1 passa, Q6 passa; Q2/Q3/Q5 parciais (greedy flipa `eos` 0,43 vs `+` 0,29); Q4 falha.
- Política shippável: greedy + `repetition_penalty=1.15` + máscara 2137 ids especiais (determinístico, sem pesos novos).
- Bench 1B CPU: fonte 0,7-1,0 tok/s vs v6/v7 2,2-3,6 (~3-4x); v6 == v7 token a token; W2A8-host ~6x pior (refutado no CPU).
- Kernel: v7-reader + `unembed_q6k` (host torch 5/5); `cargo check --release` 0 erros (1 warning conhecido).

## Decisões

- Phi-4-mini Q4_K denso rejeitado como drop-in (ternarizar destrói; RoPE parcial/LongRoPE; vocab cap; embed 2,46GB OOM).
- ADRs 0114 (TensorFold), 0115 (v7 ideal), 0116 (E01-E38 julgados; whitelist E06/E09/E16/E24/E30; resto DEAD_WEIGHT).
- 7B/10B = HYPOTHESIS (9/11GB > ferro local); 27B/30B eram fantasmas; treino/QAT bloqueado sem ferro alugado.

## Pendente (IDEAs #691-#698)

Boot QEMU com .V7; BitNet-2B-4T no rig; rep+mask no kernel; BPE embutido no kernel; escalas-por-linha; recall probe; grammar prototype; piloto 1B-QAT; inferência 7B/10B em ferro maior.
