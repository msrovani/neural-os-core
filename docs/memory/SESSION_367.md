# SESSION_367 — Gate de resposta LLM + labs mesh/LINJ

## Feito

### Testes host (contrato submit ≠ resposta)
- `cortex::llm_response_gate` — funções puras: classifica serial, `LlmEvidence::verdict()`
  - 7 testes: intent/submit, prefill/decode/done, submit sozinho=FAIL, prefill_done sem decode=FAIL, pipeline PASS, prompts ASCII≤240, MSG_DELTA conta
- `cortex::infer_queue::tests` — 5 testes: submit/cancel, fila cheia, topics, prompts LINJ
  - `TEST_LOCK` + `drain_infer_queue_statics` (statics partilhados → flake em paralelo)

### Lab QEMU
- `tools/lab-llm-response.ps1` — LINJ + Falcon3_1B; PASS só com `prefill_done` + (`decode_tok/s=` | `done id=` | `MSG_DELTA`)
- `tools/lab-mesh2-*.ps1`, `run-mesh2-audio.ps1`, `run-mesh6-lab.ps1`, `pack_nkp_lab.ps1`
- `hermes::lab_inject` — blob LINJ @0x2100000 → `USER_INTENT` pós `model_is_loaded`

### WIP acompanhante (mesmo working tree)
- Áudio/STT/wakeword/mixer honesty; mesh FRAG/udp; GPU KernelPack/ADR-0105 notes; heap/allocator notes

## Lições (aprenda)
1. **`InferQueue submit` ≠ resposta** — lab clima mesh passou com prefill_done e zero decode/done → PASS falso. Aceite = decode | done | stream.
2. **Statics de InferQueue + `cargo test` paralelo** — testes sem mutex corrompem HEAD/TAIL/PENDING; flake `queue_pending()>=1` / overflow. Fix: `spin::Mutex` + drain.
3. **Verdict Last-N no serial** — LINJ cedo some do tail; scan full-log (ou gate Rust) evita FAIL falso.
4. **Host freeGB** — dual 8G/6c estoura; SingleNode/RamGB=5 ou 1B lab para gate de resposta.

## Testes
```
cargo test -p cortex --lib llm_response_gate -- --test-threads=1   # 7/7
cargo test -p cortex --lib infer_queue::tests -- --test-threads=1  # 5/5
```

## Docs
- IDEA #590; STATE s367; INDEX; CHANGELOG
