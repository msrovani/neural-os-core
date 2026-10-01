# SESSION_432 — WHPX 6-lane hardening (log 213145) + triagem IA do HUB (s432)

## Origem
- Log `logs/boot_whpx_20260930_213145.txt` (3851 linhas, BOOT SCORE ok, 8 SMP, 9216MB).
- Medido no log: tick lento 27x; auto-grow 8x (512→1536); NESTED 262x; posture FAIL 11x
  (`a2 x0 e6`); prefill 78.47s/6 toks (`mlp=46984664`, 75%); matmul enter 288/exit 295;
  ap-gated 70x bounded; soft-18Hz 1x (T+4710); OOM/TALC 1x (infer_worker); 1x #PF storm
  park-core fail-closed; AT10K timeout 1x; MSC FAIL 2x (early, esperado); AUDIO 12x
  NO_GO→GO; `decode_tok/s=0`.

## 6 lanes (arquivo:linha)
1. `interrupts.rs:83-105` — hlt gate: sob MicrosoftHv, `hlt` sem wake vira stall;
   troca por pause-spin gateado por hypervisor.
2. `capture.rs` caps (4ev/8fr) + `hub_health.rs:144-161` — drains com cap 8: yields nos
   produtores de áudio/HUB sob heap pressionado (NESTED-serial-divert não cobre alloc
   aninhado).
3. `allocator.rs:137-159,284-306` — refuse-in-slice derrama p/ TALC spill (serviço
   efetivo, sem deferred-runner); huge-2MB gate OFF (`:160-281`) até validação tok/s.
4. `hda.rs:298-383` — LPIB-frozen: estimate por `dt×48000` quando o LPIB congela.
5. `agents.rs:371,482,538-542` — submit-only proof + ps1 `-ModelKind 1b|3b`.
6. `decision.rs:555` — posture MIN 4→8 + piso absoluto esc>=8 (`:559-575`); fim do
   FAIL/flap periódico com 1 escalate (`a2 x0 e6`).

## HUB triage IA (premissa máxima ADR-0088, ideia #629)
- `hub_triage.rs` novo: snapshot `HUB\0` 60s, worst-state puro, propose HITL com dedupe
  FNV (10min) + `lib.rs` mod + `security.rs` LAST_SAFETY + `main.rs` registro x2.
- Compila: check geral 0 erros com os arquivos.
- **Validação QEMU 8GB/8c (log `logs/boot_whpx_20260930_224429.txt`, 13,9min / T+49937):**
  `Tier 1 ready size=6911 MB` (claim s431); HubTriage `T+2338 ok`, 4× `observe: posture
  FAIL (sintoma de carga)` (anti-loop); **proposta HITL real `T+36581`: Heap critico +
  arena livre — mover conversao/context-window p/ arena Cortex**; `T+36923` Jarbas
  processou o intent com snapshot JSON completo (heap 1834/2030, arena 0/256, decide
  sev=2, machine UNKNOWN) → Hermes → MoE `disk_diag score=0.975` → **LLM invocado**
  (Trust ok; output gibberish = modelo stub esperado em QEMU). `T+40901+` propose dedupe
  (cooldown) — anti-loop s410d provado. Auto-grow 1792→2030MB sem OOM de bump.
- **Residual aberto:** 2× `[OOM/TALC] ... agente=infer_worker` (size=10624 @T+48653,
  size=73 @T+49937) com span de 6911MB — motivo do null do TALC desconhecido
  (hipóteses: alloc_physical_frame None / estado interno / map_page_direct). 1× #PF
  storm park (linha 3680, sistema seguiu). → instrumentar path de overflow
  (`TALC_OVERFLOW_NULL` / `PF_DIAG_*`) antes da próxima sprint de heap.

## Verificação
- `cargo check --release`: 0 erros (1m41s).
- Per-crate checks + testes: capture 3/3, allocator 7/7, hda 5/5, decision 6/6.

## Residuais → ideias
- #626 huge-page flip; #627 seam per-CPU infer-stamp; #628 HOWTO 1B doc line.
