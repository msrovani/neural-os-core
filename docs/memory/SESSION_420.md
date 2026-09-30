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

## Addendum s420b — train_hint_mlp.py + loader HINT.BIN

**`tools/train_hint_mlp.py`** (novo): treina o MLP de hints 64→128→16 e exporta
`target/HINT.BIN` no pack W2A8 do contrato `hint_render.rs`. Modos: `--self-check`
(round-trip pack bit-a-bit vs hint_render.rs), treino padrão (professor sintético
+ QAT), `--validate <bin>` (acordo contra o professor).

Pipeline do treino:
1. **Professor sintético por regra** (lição SESSION_346: dataset tem que ser a
   modalidade real — não existe dataset de "hints corretos"). Política HMI §6.4
   como combinação LINEAR NÃO-NEGATIVA das 6 features vivas do ui_state —
   restrição de representabilidade: o forward do kernel é h=ReLU(x·W1ᵀ); y=h·W2ᵀ
   SEM bias e SEM termo constante → aluno não representa constantes/produtos
   (matiz 200 fixo era irreproduzível). Idle = y=0 = modo clássico (§6.4 correto).
2. **QAT (quantization-aware training, STE)** com escalas c1/c2 APRENDIDAS —
   PTQ pós-treino teto em 87,6% de acordo (W2 ternário não reproduz pesos
   precisos); QAT → **100% de acordo** (MAE energia 5,5/255, matiz 1,8/255,
   holdout disjunto, densidade w1=22% w2=55%). Métrica de validação = acordo
   perceptual (bucket OU erro relativo), não loss.
3. **Export v1** (3160 B: header 24 + w1 2048 + w2 512 + b1/b2 informativos) —
   pack 2 bits low→high idêntico ao bitnet_writer (round-trip provado).

**Wire no kernel** (`hint_render.rs` + main.rs): `parse_hint_bin`/`load_packed`/
`try_load_from_fat` — HINT.BIN lido do root do volume (FAT32/exFAT via
`fat_assets::read_root_file`) logo após `init_bar_compute` no boot; fail-closed
honesto (dims/magic/tamanho divergentes = log fail + Ready pass-through clássico,
nunca erro). Teste host `parse_hint_bin_v1_roundtrip_tool` replica o export do
tool byte-a-byte. `mkfat32.py` embute o HINT.BIN (se existir) na imagem.

Validação: QEMU 8G/6c boot limpo (VirtIO-GPU sem aperture → loader não roda,
honesto; zero #PF); k-hal 67 (66+1); check release 0 erros. No metal com
aperture real (GTX 1050 lab), o caminho é: train → mkfat32 → boot →
`HINT.BIN carregado bytes=3160 upload=true` → stage 2 (Resident).

## Addendum s420c — Consumidor do tópico HINTS (tint neural em orb/dock/cards)

**`jarbas::display::hint_tint`** (novo): consumidor canônico do tópico `HINTS` —
fecha a regra s352 ("tópico novo só está feito quando tem consumidor").
- **Snapshot:** 8 regiões × (energia, matiz) + idade TSC; `accept_payload`
  valida 24 B exatos — payload torto NUNCA substitui o snapshot vigente.
- **Freshness com decaimento:** energia decai linearmente até 0 na borda de
  `STALE_US` (2.5 s = 5 feeds de 2 Hz perdidos) → modo clássico sem pulso.
- **`hue_to_rgb` glass:** roda de cor 6 setores inteiros (zero f32) +
  dessaturação 40% mix com base azulada — compatível com a paleta JARVIS.

**Tint aumentativo (§6.4 — só soma, nunca substitui tema):**
- **Orb (região 0):** lerp do accent na energia do hint (`soul_mirror.rs`) —
  nunca entra no corpo; energia 0/stale = accent clássico (estado/papel mesh).
- **Dock (região 2):** borda superior + indicador running tingidos
  (`dock.rs`), mix na energia.
- **Cards (regiões 3..6 por `id % 4`):** barra de título mix com C_TITLE_BG
  (`card.rs`), teto de mix 200/256 (título sempre legível).

**Wire:** DisplayAgent lazy subscribe + drain `HINTS` (mesmo padrão
mesh/fleet_health). Zero alloc no hot path (`sample` = leitura de static +
comparação de idade; cor computada 1× por paint, não por pixel).

Validação: check release 0 erros; jarbas 130 (126+4: payload inválido, freshness,
sem feed, hue determinístico — TEST_LOCK + reset_for_test, lição s346); k-hal 67.
QEMU 8G: boot limpo, zero #PF, 60Hz — em QEMU o produtor não publica (sem
aperture → stage 0/1) e a UI segue 100% clássica (honesto). O caminho completo
(produtor + consumidor) só acorda no lab GTX 1050 com aperture real.

Residual: visual é perceptual (matiz/energia não têm ground truth) — validar no
lab que o tint é visível e agradável; ajustar `STALE_US`/mix se necessário.

## Addendum s421 — Lane VRAM ADR-0112 por sequência + correções de corretude

**Auditoria do s418 (pré-requisito da ativação) achou 3 problemas de corretude
que impediam o lane de servir compute sem corromper logits:**
1. **SHAPE_INDEX por família**: sobia 1 matriz por shape (layer 0) e servia
   TODAS as layers por shape — q/k/v/o têm a MESMA shape (h,h) e não são
   distinguíveis por shape (e colidiria com experts do MoE).
2. **Layout do GEMV**: `gemv_from_vram` lia `idx=j*k+t` (coluna-major) mas o
   upload copia o pack row-major do heap — bits do peso errado no unpack.
3. **Escala única**: `si_acc` sobrescrito por linha e usado para todas —
   quantização errada para m>1.

**Correções (k_hal/bar_compute.rs + cortex/compute.rs + cortex.rs):**
- **Dispatch POR SEQUÊNCIA (layer-major)**: `dispatch_vram_seq(slot=layer*7+
  [q,k,v,o,gate,up,down], w, x)` — o apply_one_layer conhece a posição da
  matriz; `vram_ternary_seq` valida shape contra o residente do slot
  (divergiu = sequência dessincronizada = None honesto → CPU ladder).
- **Upload sem dedupe + pré-checagem de capacidade INTEIRA**: need total
  (1B≈369MB, 3B≈675MB) ≤ aperture − ring − já-residente (hint) ou lane off
  honesto — parcial é proibido (peso errado é silencioso).
- **Cursor persistente de weights**: hint (boot, antes do LLM) e LLM
  (set_model, depois) dividem a aperture; `SEQ_MATS` só no on_model_loaded.
- **GEMV**: row-major `idx=t*n+j` (mesmo pack do heap), `si_row[8]` por linha
  (m ≤ 8 do RingPlan), prefetch por coluna de linhas.
- `dispatch_ternary` genérico NÃO mais toca o slot VRAM (ABI seq-only).

**Honestidade estrutural:** o heap NÃO encolhe com o lane ativo — o bump não
tem free e os pesos já foram alocados no boot pelo loader. A liberação REAL de
heap exige loader-VRAM (pesos jamais passam pelo bump — residual da ADR-0112,
lab). O lane corrigido entrega o caminho de leitura via BAR correto e o
contrato para o upgrade; o ganho de RAM hoje é só quando o loader muda.

**Validação QEMU 8G/6c (VirtIO-GPU sem aperture → lane off honesto):** boot
limpo, zero #PF real, grow máx 1536MB (abaixo do teto 2030), prefill id=1
completo (22 slices, 91s), a2_proof id=2 timeout honesto 300s (fail-closed,
sistema vivo matmulando depois — matmul exit ok pós-timeout). CPU ladder
intacta: sem aperture, `vram_served=false` e todo matmul segue SMP.

**Gates:** check release 0 erros; cortex 107 (106+1 vram_seq fechado sem
registro), k-hal 68 (67+1 seq sem residente), k-nano 232.

Residual: (1) loader-VRAM para liberar heap de verdade (exige skibiboot/Limine
colocando o modelo na BAR — lab); (2) aperta do a2_proof timeout 300s→~120s
(idle us=160ms/slice × 22 layers ≈ 8s — o timeout pega stall real, não lento).
