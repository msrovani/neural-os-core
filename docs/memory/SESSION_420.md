# SESSION_420 — Gate fail-closed de headroom no prefill (fecho do residual s419)

**Data:** 2026-09-29 · **Branch:** main · **Base:** 7c9b5aab (s419)

## Objetivo
Fechar o residual do s419: "prefill a2_proof + KV + reply cruza o teto de heap
~2030MB" — classe OOM-sob-teto (alloc NULL → deref → #PF → hlt no AP).

## Análise do gap
Os gates críticos do s417 (`heap_headroom_critical()`, 64MB) disparam no topo
do `poll_slice` — **entre** slices. Mas o auto-grow acontece **dentro** do slice
(KV/mask/logits em `apply_one_layer`): o heap cruza o teto no meio do slice
antes do piso de 64MB ser re-checado. Faltava um piso **proativo** checado no
INÍCIO de cada slice pesado.

## Implementação
1. **`k_nano::allocator`:** `HEAP_PREFILL_HEADROOM_MB = 128` + `heap_headroom_low()`
   — piso proativo (128MB = margem de ~1 slice de grow de 256MB) acima do
   crítico (64MB).
2. **`infer_queue::run_prefill_step`:** gate no topo — sob `headroom_low`,
   recusa honesta: slog warn + `log_quiet` BOOT.LOG
   (`prefill refuse headroom_low id=N layer=N headroom_mb=M`) + `a2_refuse` +
   `finish_job("[heap: headroom baixo no prefill — fail-closed HITL]").
3. **`infer_queue::run_decode_one`:** mesma classe (o `forward_with_kv` anexa
   KV → cresce o bump dentro do slice); termina com payload parcial.
4. **Teste de contrato:** `headroom_low_gate_above_critical` (low > critical;
   low ⊇ critical).

## Validação (QEMU WHPX 8GB/6c, logs/boot_whpx_20260929_194717.txt)
- `prefill_done id=1` (greeting) ok; a2_proof id=2: **completo** (SmpMmGuard do
  s419 segurou o barrier — `done id=2 toks=1 prefill_us=77019339`).
- **Grow máximo = 2030MB (teto), antes cruzava** — need 1792 < janela.
- **Gate disparou em produção:** `prefill refuse headroom_low id=4 layer=0
  used=1913MB headroom=117MB` → job real recusado honesto, `done id=4 len=54`
  (payload escalate), sem #PF, boot vivo (heartbeat + SGDB remember pós-done).
- Gates: check release 0 erros; cortex 106 (105+1), k-nano 232.

## Lições
- **Gate entre slices não cobre o que cresce dentro do slice:** piso proativo
  (1 slice de margem) checado no início de cada slice pesado termina o job
  honesto ANTES do heap esgotar — recusa custa um job, OOM custa um core.
- **A classe coberta confirma o padrão s415-417:** claim (48MB) → in-job crítico
  (64MB) → slice proativo (128MB) → o OOM não tem mais janela entre checks.

## Residual
- Teto 2030MB ainda é atingido (need 1792 do Falcon3-3B); a cura estrutural é a
  ADR-0112 (pesos em VRAM via BAR) — o gate só garante que cruzar não vira #PF.
- n-sgdb: SESSION_419/420, lições e IDEA #623 pendentes de registro (MCP fora
  desta thread).
