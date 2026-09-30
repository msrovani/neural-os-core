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

---

## Addendum s422 — Intel VMD binder (2026-09-30, base 78f2302f)

**Motivação:** o boot do lab (foto) mostrou `storage 8086:a77f sem driver` e o
`E:/BOOT.LOG` do pendrive voltou ZERADO — o kernel não tinha NVMe (o disco do
notebook vive atrás do VMD, invisível ao CF8/CFC) nem USB-MSC a tempo, então
não havia canal de persistência canônico. O BOOT.LOG persistente via NVMe é
também o caminho de evidência que faltou para o H3-revisit.

**Implementação (`k_nano/src/vmd.rs`, 5 testes):**
- Binder R0 do domínio PCI secundário do VMD (layout: linux `vmd.c`, sem código
  copiado): CFGBAR (BAR0 64-bit, ≥1MB) = ECAM dos filhos; `busn_start` via
  VMCAP(0x40).BUS_RESTRICT + VMCONFIG(0x44)[9:8] → 0/128/224; ECAM offset
  `(bus−busn_start)<<20 | dev<<15 | fn<<12 | reg`.
- **Offsets SHDW** (vendor-cap magic "SHDW"): nativo ⇒ MEMBAR_cpu == host_phys
  ⇒ offset 0 ⇒ **DMA do filho é endereço físico direto** — os bounce buffers do
  driver NVMe funcionam sem tradução. Não-nativo (visão guest) = abort honesto
  (sem tradução DMA implementada).
- Scan flat de todos os buses do range (bridges 06:04 cobertas pelo range) +
  enable MEM/BUSMASTER dos filhos via config MMIO com readback (CF8/CFC não
  alcança o domínio). D0 antes de tocar BARs (SESSION_260). DIDs do vmd.c
  (14 DIDs; 28C1 USE_BIOS_INFO abortado honesto).
- **Wire:** `NvmeDriver::probe_at_mmio(bar0)` extraído (mínimo diff);
  `probe_storage_drivers` → sem NVMe nativo → `vmd::init()` + `probe_vmd_nvme()`
  → MESMO global `NVME_DRIVER` (DiskAgent/StorageBus/boot_logger/boot_ckpt
  pegam de graça; bin intocado — regra emagrecer).
- Honestidade: VMD ausente = normal (log info); fail-closed por passo (CFGBAR
  <1MB, bus offset desconhecido, readback divergente).

**Gates:** check release 0 erros; k-nano 237 (232+5). **HW lab pendente:** boot
no notebook — esperado `nvme ok=true via=vmd`, BOOT.LOG persistindo no NVMe e o
card de HW deixando de listar o a77f como `sem driver`.

**Lições:** (1) "8086:a77f sem driver" não era card errado — era o VMD real;
a ausência de driver de PONTE mascarava o storage inteiro. (2) O domínio VMD
não é descoberto por scan convencional: ECAM via CFGBAR é o único caminho —
probe de storage precisa de fallback de domínio, não de mais retentativas no
mesmo domínio. (3) DMA físico direto (SHDW=0) é o que torna o port barato: o
driver NVMe existente funciona sem saber do VMD.

---

## Addendum s423 — Power-on dGPU D3→D0 via PMCSR (2026-09-30)

**Motivação:** SESSION_260 provou que a dGPU dorme em D3 no boot (BARs visíveis,
tocar VRAM = hang de barramento) e os gates do `init_vram_tier`/`nvidia::probe`
recusavam honestamente — matando o H3 e o lane VRAM em TODO notebook. O wake
por software é seguro se feito com prova de vida; recusa incondicional era
honestidade demais que bloqueava a feature inteira no laptop.

**Implementação (`k_hal/src/gpu/gpu_power.rs`, 3 testes):**
- `wake_to_d0(&mut GpuInfo)`: no-op em D0–D2 e sem PM cap; **prova de vida
  ANTES do write** (vendor ID ≠ 0xFFFF — D3cold/ausente NÃO recebe write às
  cegas, evitando o freeze que o gate original evitava); PMCSR via
  `pci_power_on_d0` (readback); poll do D-state com budget TSC 10ms (spec §5.4);
  **re-scan persistido** no `GpuInfo` — os gates existentes passam a ver D0.
- `wake_all` no boot logo após `detect_all()` (ponto canônico único, antes do
  `plan_assignment`); `init_vram_tier` e `nvidia::probe` chamam `wake_to_d0`
  como backstop idempotente.
- Honestidade: wake falho (Stuck/NoLife) mantém o comportamento fail-closed de
  hoje (skip + slog warn) — nunca hang.

**Gates:** check release 0 erros (rebuild real 2m16s); k-hal 71 (68+3).
**HW lab pendente:** boot no notebook → `GPUPWR ... woke` + HINT stage Resident
(`hints on vram=3KB fwd<100µs`) + lane VRAM `vram_served=true` pós-wake.

**Lições:** (1) "recusa honesta" e "capability" se confundem: o gate anti-hang
da SESSION_260 era seguro mas congelava a feature no metal — a correção não é
remover o gate, é atacar a CAUSA (D3) com o mesmo rigor de segurança (prova de
vida antes do write). (2) Re-scan pós-wake persistido no struct = gates
existentes não mudam de contrato (single source of truth do D-state continua
sendo o `GpuInfo`).

---

## Addendum s424 — BOOT.LOG de fallback na ESP (2026-09-30)

**Motivação:** o lab do s420/s422 perdeu a evidência canônica duas vezes: o
`E:/BOOT.LOG` (volume de dados) voltou ZERADO e só sobrou o stage-0 do
logwriter-efi em `D:/NEURAL/BOOT.LOG` (49B, subdiretório que o walker FAT do
kernel não enxerga). O fallback de backend existente já considerava a ESP na
ordem, mas (1) um `IoFail` na partição de dados abortava a varredura ANTES de
chegar à ESP e (2) a ESP não tinha BOOT.LOG raiz pré-alocado (kernel não cria
arquivo FAT — só sobrescreve chain existente).

**Implementação:**
- Kernel (`boot_logger.rs`): falha de I/O em qualquer etapa de qualquer
  partição marca `saw_io` e CONTINUA para a próxima (ESP incluída); resultado
  tipado decidido no fim — contrato `OverwriteResult` intacto p/ self-heal.
- Build (`mk_esp_fat.py`): BOOT.LOG raiz pré-alocado 256KB na ESP (chain
  válida, conteúdo zero). O kernel grava com o MESMO mecanismo do volume de
  dados — zero código novo de FS.
- Fix de corretude: o write do dirent de tamanho usava `sector_idx*bps` como
  offset de byte (bug latente que só não explodiu porque o dirent do BOOT.LOG
  caía no setor 0 do cluster); agora calcula o setor real do `entry`.

**Validação:** ESP de teste 128MB — dirent BOOT.LOG na raiz (262144B, chain
legível, zero preenchido, BPB compatível com o parser: root_entries=0).
k-nano 237/237; check release 0 erros (rebuild real 1m50s).

**Lições:** (1) fallback em lista ordenada só é fallback se o erro de UM item
não aborta o loop — revisar `return` no meio de varreduras multi-backend.
(2) "kernel não cria arquivo FAT" é uma restrição real do design (sem alocador
de clusters no writer) — contornar no BUILD (pré-alocar), não no runtime.
(3) Evidência de boot em HW precisa de >= 2 caminhos independentes de mídia
(ESP + volume de dados); ambos pré-alocados no build.

---

## Addendum s425 — Linha `bootlog` no HUB HEALTH (2026-09-30)

**Motivação:** regra s352 (topico novo precisa consumidor) aplicada ao
BOOT.LOG de evidência: o kernel tentava 5 backends e, em HW sem serial, o
técnico via apenas SILENCIO — "sem backend" (o cenário do lab: MSC não
enumerou + VMD sem driver) era indistinguível de "funcionando". A falha agora
aparece na UI com a razão tipada.

**Implementação:**
- `boot_logger::hub_log_line()` — 1 linha ≤36 chars: `ok n<N>` / `fail
  <backend> <razão> [sk<mask>] x<streak>` / `fail sem-backend msc=N ata=N` /
  `pre-fat (buffer)` / `n/a`. Statics `LAST_FAIL_KIND`/`LAST_TRY_BACKEND`
  anotados nos paths de falha do persist_now (não é derivado — é o que
  aconteceu de verdade; `n/a ≠ 0`).
- Linha `bootlog` no HUB HEALTH (row 19, HUB_ROWS 20) — Warn quando fail.
- SysInfoAgent: o mesmo string no slog `persist pending` (serial e UI
  compartilham a fonte — sem segunda implementação).

**Gates:** 0 erros; k-nano 237, jarbas hub 7/7, hermes 257 (t=1). Crashes do
harness host (jarbas damage_tests UB-check; k-nano tickv compact flaky em run
paralelo) provados pré-existentes via stash.

**Lições:** (1) "sem backend" e "funcionando" não podem render a mesma UI —
fail-closed honesto inclui DIZER por que falhou. (2) A string da UI e a do
slog nasceram da mesma função (hub_log_line) — diagnóstico duplicado com duas
implementações diverge na 1ª mudança (lição dual-truth s410h).

---

## Addendum s426 — Card HUD "Hints Neurais (H3)" (2026-09-30)

**Motivação:** validar o H3 no lab exige ver `HINT_FORWARD_US`/`HINTS_GENERATED`
ENQUANTO o sistema roda — extrair BOOT.LOG (pendrive, reboot, seek em raw)
torna o ciclo de debug de UI lento demais. O card lê os statics direto (fonte
única) e mostra o critério de aceite com affordance visual.

**Implementação:**
- `jarbas::cards::hints_card` (ID 8003): `stage` (off/ready/resident/device),
  `fwd <N>us (alvo <100)` (verde só com stage≥2 E fwd∈(0,100)), `hints n`,
  `vram <N>B resid.` — Host sem BAR = `off (sem BAR)` + unknown (não finge).
- F11 toggle (`ToggleHintsCard` + `close_card_by_id` no compositor — primeiro
  close por id da UiDeclaration); refresh vivo 2 Hz no tick (spawn_or_update
  idempotente por id); help (H) atualizado.

**Gates:** 0 erros (1m07s); jarbas 123/123 (damage_tests host crash
pré-existente via stash, skipado), k-hal 71.

**Lições:** (1) Replace por string em braço de match duplicado trocou o braço
errado (ToggleHubHealth) — restaurar SEMPRE conferindo o match inteiro quando
há dois matches do mesmo enum no arquivo. (2) Conta de altura do card tem que
espelhar o builder (divisor 8px, linha 13px) — teste pegou na 1ª rodada.

---

## Addendum s427 — Loader-VRAM FAT→BAR sem heap (ADR-0112 residual, fase 1)

**Motivação:** o ADR-0112 resolveu O MECANISMO (pesos residem na BAR, GEMV
host lê via BAR) mas a rota de entrada ainda era: FAT → heap bump → cópia
heap→BAR. O heap paga o blob INTEIRO (~256MB packed do 1B) e o bump nunca
libera. O loader-VRAM lê o FAT por clusters e copia cada pedaço direto pra
aperture — o blob do packed nunca precisa existir contíguo em RAM.

**Implementação:**
- `read_root_file_dev_chunked` (k_nano): leitor FAT32 por callback — cada
  cluster vai pro caller (bounce único), zero Vec do blob. Mesma validação
  do leitor inline (BPB/anti-OOB/chain cíclica).
- `loader_vram` (k_hal): probe (512B → `parse_model_header`) → layout por
  layer derivado do header (shapes + stride + tensor_off) → `layers_off`
  derivado do próprio blob (tok_len real — sem chutes) → pré-checagem
  INTEIRA (`reserve_vram_span`, alinhado ao cursor do hint via
  `NEXT_WEIGHT_CURSOR`) → stream por janelas (interseção chunk×janela →
  `write_vram_bytes`) → registro `register_loader_resident` (mesmo contrato
  SEQ_MATS/RESIDENT/UPLOADED). Stream incompleto = SEM registro (lane off,
  fluxo legado roda — parcial é proibido).
- Wire: boot chama `load_from_fat_to_vram("FALCON1B.BIN"/.V6)` após
  `init_bar_compute`; `on_model_loaded` vira no-op quando `loader_resident()`
  (não copiar 2× do heap).

**Honestidade de escopo:** o `load_llm_v6` AINDA copia os packed pro heap no
parse — o ganho desta fase é a rota BAR independente + no-op do upload duplo
(precisamente: os pesos que a GEMV lê vêm do FAT, não da cópia). A liberação
REAL do heap exige o stub dos packed no parser quando `loader_resident()`
(residual documentado no módulo).

**Gates:** 0 erros (1m15s); k-hal 74 (71+3: shapes/total/packed), k-nano 237.
**HW lab:** `LOADER-VRAM ... streaming FAT→BAR` + `loader-VRAM: pesos
FAT→BAR sem heap (126 matrizes, 256MB)` no BOOT.LOG; 1B cabe em BAR1 256MB
com folga ~0 (ajustar expectativa: pré-checagem pode recusar honesto se o
cursor do hint tiver consumido o alinhamento — futura imagem 3B=AWAITING ReBAR).

**Lições:** (1) Calcule o total do modelo pela fórmula, não pelo ADR: 1B =
256.5MB (não 369MB — o número do ADR era pré-medida e errado; SESSION_392-401
de novo). (2) Primeira implementação do orquestrador nasceu com um Streamer
estado-cinco-campos — matar e reescrever por JANELAS (ranges pré-computados)
foi 3× menor e testável. (3) Format-do-parser é contrato: `layers_off`
derivado do header REAL lido do arquivo (tok_len no blob), nunca hardcoded.

---

## Addendum s428 — a2_proof 120s + watchdog por slice (2026-09-30)

Fecha o residual (2) do addendum s421: "aperta do a2_proof timeout 300s→~120s
(idle us=160ms/slice × 22 layers ≈ 8s — o timeout pega stall real, não lento)".

**Implementação:**
- Deadline 300s→**120s** (piso: 8s mensurado × 15). O deadline global pega
  wedge; não precisa mais de 300s de espera para declarar.
- **Watchdog por slice:** `A2_SLICE_T0_US` marcado dentro do latch (início do
  slice em curso), limpo no retorno do `poll_slice`. O check roda no TOPO do
  `poll_slice` (fora do latch — roda mesmo com SLICE_BUSY preso noutro core):
  slice em curso > **10s** (20× budget 500ms) = **wedge** (lost wakeup/lock
  preso/#PF silencioso) → terminal honesto IMEDIATO da prova.
- Os dois fenômenos agora têm caminhos distintos: **stall real** (poll_slice
  não voltou — watchdog por slice) vs **slice lento** (voltou com us > budget
  — `A2_SLOW_SLICES`, abort no 3º como antes).

**Testes:** constantes + ordem (stall < deadline); wedge simulado → terminal
imediato com deadline global no futuro; slice lento (1s em curso) NÃO terminal.
cortex 110; 0 erros.

**Lições:** (1) O watchdog que só reporta pós-retorno (SESSION_419) não pega
stall — este checa o slice EM CURSO no topo do próximo poll; mas se o wedge
for TOTAL (nenhum poll roda), só o deadline global pega — as duas camadas são
complementares, não redundantes. (2) T0 do slice é estado do latch: marcar
DENTRO (nunca antes — lição s413 TSC-depois-de-log) e limpar no retorno; T0
velho sem slice = falso wedge.

---

## Addendum s429 — VMD visão guest: tradução MMIO de offsets SHDW não-nativos (2026-09-30)

Pedido: "implementar tradução DMA de offsets SHDW não-nativos no driver VMD
para suportar visão guest". A hipótese de partida (traduzir endereços DMA do
NVMe host→bus por janela SHDW) foi REFUTADA pela leitura canônica do `vmd.c`
(busca ativa antes de implementar):

1. `pci_add_resource_offset(&resources, res, offset)` com
   `offset = MEMBAR_cpu − host_phys` aplica o offset a RECURSOS/BARs:
   `bus = cpu − offset` ⇒ a janela BUS do domínio começa em `host_phys`.
2. Grep completo do vmd.c: NENHUM offset aplicado a DMA de RAM — só
   `dma_set_mask_and_coherent`. DMA upstream do filho sobe UNTRANSLATED
   (identidade) TAMBÉM no guest.
3. `vmd_in_guest = offset[0] || offset[1]` só exige MSI remap — irrelevante
   aqui (driver NVMe é polling).

**O que de fato traduz no guest é o DOWNSTREAM MMIO:** o BAR do filho é BUS
addr dentro da janela [host_phys, +size) e a CPU só o alcança via MEMBAR
mapeada UC: `cpu = bus + offset`.

**Implementação:**
- `vmd.rs`: `MemWindow{host_base,len,offset}` em `VMD_DMA_WINS`;
  `translate_bar(bus,...)` puro (`Some(cpu) = bus + offset` dentro da janela,
  `None` fora); `child_bar_cpu(bar0) -> (phys, needs_map)` — guest: dentro da
  MEMBAR já mapeada UC (needs_map=false); nativo: phys host (needs_map=true).
  `init()` não aborta mais em offset≠0: valida MEMBAR size (0 = abort honesto
  fail-closed), mapeia UC as MEMBARs, registra janelas, slog `GUEST: traducao
  MMIO ativa (... DMA identidade)`. `guest_mode()` expõe o estado.
- `nvme.rs`: `probe_at_mmio_va(mmio)` — bring-up sobre VA pré-resolvido (o VMD
  guest passa o VA da MEMBAR UC); `probe_at_mmio` mapeia UC e delega. DMA de
  RAM do NVMe idêntico nos dois modos (evidência do item 2).

**Testes:** tradução com offset real (+0x10_0000_0000), bordas da janela,
fora = None; nativo identidade; detecção guest `off1||off2` com offset
negativo. k-nano 240 (237+3); check release 0 erros (1m12s).

**Lições:** (1) "Tradução DMA" no VMD guest é MMIO downstream, não DMA — o
nome do pedido era armadilha; o vmd.c decide, não a intuição do bus master.
(2) Bus window = [host, +size) porque bus = cpu − offset — inverter a direção
do offset inverte a base da janela e o BAR do filho cai fora. (3) Confirmação
diária da regra: hipótese de oracle é hipótese (SESSION_411) — fetch do vmd.c
resolveu em 1 passada o que teriam sido dias de debug em HW.

**Limites honestos / AWAITING_HW:** guest só faz sentido com SHDW≠0 + MEMBARs
pré-configuradas pelo BIOS; xHCI/HDA fora do domínio VMD não são afetados;
validação final no notebook (nvme ok=true via=vmd guest no BOOT.LOG).

## Addendum s430 — Lab QEMU 8GB/8c: 5min na UI sem freeze + estabilidade estendida (2026-09-30)

**Goal:** 5 minutos na interface (8GB/8c, WHPX, todo HW possível) sem
travamento/freeze/hang. **Resultado: goal batido** — rodada 10 rodou
14,8min com UI/scheduler vivos (613 ticks SCHED), 0 faltas até T+28050
(7,8min), único evento = 1 storm CONTIDO por park (BSP/UI seguiram +5min),
no fim OOM reportado honestamente (`OOM/TALC agente=audio_input`) sem #PF
fantasma nem kernel panic.

**Cadeia de fixes do lab (rodadas 1→10):**

1. **Watchdog de slice 10s→30s** (`infer_queue.rs` `A2_SLICE_STALL_US`):
   decode m=1 em 8c/WHPX demora ~11s LEGITIMAMENTE (T+4526→T+5183) — o
   falso positivo mostrou matmuls progredindo depois. Piso tem que ser
   medido no alvo (lição SESSION_412 de tiling, agora de watchdog).
2. **Deadline global virou NO-PROGRESS:** 120s wall-clock abortou prova
   que completou em 124s (`a2_proof done` veio DEPOIS do timeout).
   `A2_PROOF_DEADLINE_AT_US` agora é refreshado no fim de cada poll_slice
   quando `did && a2_proof_pending()` — o que pega WEDGE (nenhum progresso)
   e deixa lentidão honesta terminar.
3. **`emotion::analyze` alloc-free** (`hermes/src/emotion.rs`): o
   `to_lowercase()` (aloca String) deref NULL sob teto = #PF cr2=0x28
   (`hermes::emotion::EmotionAnalyzer::analyze`). Fix: buffer de stack 512B
   + `contains_ci` (windows + eq_ignore_ascii_case). Limitação honesta:
   keywords acentuadas em MAIÚSCULO não casam (ASCII-fold).
4. **Contenção de #PF storm** (`interrupts_ext.rs` page_fault_handler):
   statics locais `LAST_PF_IP`/`LAST_PF_STREAK` — mesmo IP refaltando ≥3× =
   **park do core (hlt) fail-closed** com slog `[EXC] #PF storm ip=...
   park core`. O SISTEMA SEGUE nos outros cores (provado 3×: matmuls,
   prefill e UI continuaram após cada park). Um #PF num AP não é mais
   fatal para o boot inteiro. CUIDADO: literais `b"..."` não podem ter
   UTF-8 (E0425 non-ASCII em byte string).
5. **Gates de headroom 48→128MB no InferQueue** (submit + claim re-check):
   48MB não cobre "janela quase cheia" (teto ~2030MB); 128MB = piso
   `heap_headroom_low`, mesmo gate dos slices pesados (SESSION_420).
6. **Escalada de saúde observe-only p/ I5:boot_log** + gate `mesh_frag_pressure`
   movido para o CALLER `ingest_health_issue` (TOTAL_RAM_MB host-poluído;
   função pura preservada p/ testes). Fecha a cadeia: HEALTH_ISSUE:I5 →
   intent "diagnostique e corrija" → churn council/wasm → panic wasmi.
7. **Logger no-op p/ crate `log`** (`neural-kernel/Cargo.toml` + main.rs):
   `log` 0.4 entra na árvore via virtio-drivers 0.13 (`use log::info` em
   blk.rs/transport), wasmi e cranelift — e o kernel NUNCA chamava
   `log::set_logger`: LOGGER fica NULL em BSS e macro third-party que
   passa do gate de nível deref LOGGER+0x18 = #PF cr2=0x18 durante
   probe USB-MSC (virtio_drivers debug). Fix: `NopLogger` +
   `set_max_level(Off)` no início de `kernel_boot` — macros viram leitura
   atômica barata, nunca tocam LOGGER.
8. **`BeiState::tick` guards** (`hermes/src/bei.rs`): (a) piso low_mem
   48→128MB; (b) PromoteSkill (wasmi alloc, cmpxchg em obj NULL = cr2=0x10)
   re-checa `heap_headroom_low()` antes de promover; (c) supervisor tick
   re-checa bounded no meio do tick (o prefill concorrente come o headroom
   ENTRE o snapshot do início e o lock — iterar BTreeMap<String,
   DomainConfidence> com nós demand-paged sob teto = #PF cr2=0x702a00a8
   em `LazyLeafRange::next_unchecked`).

**Lições (memorizar):**
- **Dep third-party + crate global de infra = contrato escondido.** Toda
  crate que puxa `log`/`getrandom`/etc. assume que o host inicializou o
  global. Bare-metal tem que registrar o no-op CEDO (ou feature off) — o
  bug só aparece quando o macro passa do gate de nível.
- **Watchdog medido no alvo:** 10s de stall em 8c/WHPX era legítimo (decode
  11s). Piso de watchdog é constante EMPÍRICA, não estética.
- **Deadline wall-clock mata trabalho lento-honesto; deadline no-progress
  só mata wedge** (nenhum `did` desde o último poll).
- **Storm park = degradação graciosa por core:** #PF em loop num AP não
  derruba o sistema — park (hlt) + slog + sistema segue. Fail-closed de
  core, não de kernel.
- **`to_lowercase`/alloc dentro de hot path de R2/R3 é proibido sob
  pressão** — buffers de stack + fold ASCII (2º caso: decode_harness s429,
  emotion s430).
- **Re-check bounded DENTRO de blocos longos** (supervisor tick): o gate
  do início do tick não cobre o que o prefill concorrente come no meio
  (mesma classe do gate entre slices, SESSION_420).
- **OOM honesto no fim ≠ falha do goal:** 14,8min após o objetivo, o
  OOM/TALC reportou agente e size sem #PF fantasma — o fail-closed fez
  exatamente o contrato.

**Gates:** check release 0 erros; hermes 257/257 (-t1), cortex 110,
k-nano 240 (flaky tickv compact_batch_guard passa isolado).
Logs: logs/boot_whpx_20260930_{193855,195127,200959}.txt (rodadas 8-10).
