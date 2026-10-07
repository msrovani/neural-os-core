# 📋 TODO — neural-os-core

**Versão:** v1.9.99-s451 TEST
**Data:** 2026-10-04
**Fonte:** SESSION_451 / SESSION_450 / SESSION_449 / SESSION_448 / STATE.md + ADRs 0081/0088-0113
**Retomada:** `docs/memory/RESUME.md` (runbook único — comandos do F1.5, decisões D1–D5, mapa de donos da working tree, armadilhas medidas, UNKNOWN)
**Legenda:** ✅ feito | 🟡 em andamento | `[~]` parcial | 🔴 bloqueado | ⏳ agendado | ▶️ AWAITING_HW | `[ ]` pendente

---

## 🔥 s449 — Consolidação do working tree: E1–E4 (ADR-0113) + F1 + storage AION + cleanup dead-weight

- [x] Commit de consolidação dos 41 modificados + untracked desde a s441 (E1–E4 + F1 + AION-storage + dead-weight + s441–s445)
- [x] E1–E4 (ADR-0113): `mesh::p99_index`+Kani; `trust.rs` caps + `CAP_GENERATION`; `event-bus/stamp.rs` + `register_audit_hooks(sha256)`; `bench_stats.rs` + `BENCH` markers (IDEA #642)
- [x] Cleanup dead-weight: findings 1,2,3,5,6 (`matrix_learn.rs` deletado; `Oneshot`+receiver-gate; sem heartbeat falso; sem USB poll) (IDEA #643)
- [x] `cargo build --release -p boot` = 0 erros; testes isolados só com 3 pré-existentes (permission_gate, sgdb bench abort, tq2_0_gguf fixture)
- [ ] **Kani/E1/E2**: provas `#[cfg(kani)]` compilam mas não rodam (`x86_64-unknown-none` fora do guide; WSL ausente) — verificação formal = UNKNOWN (IDEA #639)
- [ ] **Validar em boot de QEMU** as frentes E1–E4/F1/storage/cleanup — o gate desta sessão foi host (`HOST PASS != RUNTIME PASS`)
- [ ] `tq2_0_gguf_load` (IDEA #644): isolar fixture vs dequant (pré-existente)
- [ ] Sync n-sgdb de SESSION_449 + IDEA #642–#644

## 🔴 s447 — Fechamento: fórum encerrado, F1.5 com carimbo §14, ablação ausente

- [x] §14: sidecar `.imgid` no launcher + parser fail-closed sem ele (teste do carimbo EXIT=0, fixtures 22/22)
- [x] §14b: identidade comparada no parse (`bytes`+`epoch`+`sha`) — o carimbo parou de passar sobre imagem reconstruida depois do boot (IDEA #640); boot 1 real saiu de PASS para FALSIFIED
- [x] Gerador de fixtures versionado em `tools/gen_f15_fixtures.py` (vivia em `target/`, gitignored — um `git clean` apagava a suíte) (IDEA #641)
- [x] Reclassificação explícita do "boot1 PASS" → `BOOT1_EXIT=1`/`BOOT2_EXIT=1` (medido: boot 1 reprova por `§14b`, boot 2 por `not_found` + sem sidecar)
- [x] Confirmar que `uefi.img` **não** está stale (7/7 literais do AION na imagem)
- [x] Gate `tem_substancia` no watcher (4/4) + supressão contada
- [x] Encerramento: watcher parado (sem lock órfão), launcher + QEMU parados, log do boot cortado preservado
- [x] Runbook de retomada: `docs/memory/RESUME.md` + ponteiros em STATE/SESSION_INDEX/TODO/CHANGELOG
- [x] **Braco de ablação (§7)** — `tools/f15_pristine.py` + bloco `[7]` no launcher (restore opt-in; no boot 2 é a **condição de controle**: run permitido, veredito reprovado; scan sempre) + `restore=`/`disk_lab_state_before=` no sidecar + 3 regras fail-closed; fixtures 26/26, ablação 17/17, stamp exit 0 (IDEA #638, s448)
- [x] **QEMU com `-RestorePristine`** - ablacao EXECUTADA s451 (disco pristine -> `escalate=reuse reason=not_found`, comportamento reverte). D2 fechado.
- [x] `TICKV backend=file` no QEMU + os 2 boots com veredito (sidecar + identidade 14b) - DONE s451: `backend=file dev=virtio`, F1.5 PASSOU.
- [ ] Perguntar ao maintainer se o loop do OPMUSE (PID 9908) continua — é a única coisa que ainda escreve no fórum encerrado
- [ ] Sync n-sgdb de SESSION_447 + IDEA #638/#639/#640/#641 (servidor vivo, sem tool MCP nesta sessão)
- [x] `cargo build --release -p boot` desta arvore - 0 erros (s451).

## 🟡 s445 — Forum CURAIX: recover no wipe + reload pos-Tickv (F1.5 aberto)

- [x] Preservar `recover_count` no wipe de `append` (magic `NEURDONE`)
- [x] 2a chamada `reload_persisted_wasm_skills` depois de `tickv_smoke`
- [x] Hook `skill_lab` + `tools/run-f15.ps1` + blobs 256 B
- [x] Host: `boot_ramlog` 3/3 e `skill_loader` 4/4
- [ ] Patch `RELOAD_RETRIED` antes do reload + campo `durable_unknown=` (slog `durable=` permanece)
- [ ] Gerador LSK1 com 512 bytes zerados
- [ ] Warn unico se 4 bytes != 0 e != `LSK1` (nao setar `CONSUMED`; pagina zero continua muda)
- [ ] Parser que junta os dois logs (hash = FNV do WASM gravado, nao do texto)
- [ ] `cargo check --release` desta arvore + testes `skill_lab`
- [ ] QEMU: serial vivo, `TICKV backend=file`, `[RECOVER]` no BOOT.LOG, string nova no `uefi.img`
- [ ] Ack humano (`from=HUMAN`) nas decisoes `accepted` do forum

## ✅ s444 — A lane anti-fragmentação usa o histograma (Dust x Benign)

- [x] `anti_frag::FragKind` + `frag_kind(hist)` (puro): `Dust` (>=60% de gaps <8KB), `Benign` (<=20%), `Mixed` (sem dominante **ou** sem histograma)
- [x] Fragmentação **benigna** vira observe-only **antes** de chamar a IA — não gasta job LLM nem HITL humano em ação sem efeito (loop de feedback da s429-lab)
- [x] Slog passa a declarar a forma: `fragmentacao Dust detectada (...)`
- [x] 6 testes novos (28 no total) + **mutation 13/13** (M9..M13); `limiares_sao_exatos` fixa 60%/20%/30% exatos
- [x] Validação: anti_frag **28/28**, hermes **315/316** (pré-existente), `cargo nk` 0 erros; restauração pós-mutation conferida por SHA-256

## ✅ s443 — 2º consumer de longa vida no TALC (`ContextWindow`) + histograma de fragmentação

- [x] `k_nano::allocator::TalcBuf`: buffer POSSUÍDO com `Drop` que devolve o chunk ao TALC (free real), fail-closed (`None`/`false`, conteúdo intacto), `Send` sem `Sync`
- [x] `grow` **sem `realloc`** (lição s434b: `HybridAllocator::realloc` em chunk TALC-resident chama `oom()` = derruba o kernel) → alloc+copy+free, NULL honesto
- [x] `ContextWindow` migrado (`role`/`content`/`system_prompt`): fecha o **vazamento monotônico** do bump (churn de compactação nunca devolvia memória); `add()` → `bool`; `talc_bytes()`
- [x] **Histograma de fragmentação**: `talc_walk_bins` conta gaps por faixa (`TALC_HIST_BOUNDS`), sai no slog (`hist=a/b/c/d/e/f`), no snapshot JSON e em `HeapObserve`
- [x] `tools/talc_frag_report.py`: lê log real, classifica dust-dominant x benigno, exporta CSV (testado ponta a ponta, `EXIT=0`)
- [x] Testes: k-nano **276/276** (+11), k_ai context_window **5/5**, `cargo nk` 0 erros; `hist=`/`TalcBuf` no kernel.elf
- [x] `tools/test_talc_frag_report.py` **3/3**: teste de contrato que monta a linha a partir do template real do slog em `hub_triage.rs` (formato é contrato, SESSION_411) — inclui log pré-s443 (sem hist) e log sem telemetria (`!= 0`, não finge relatório)
- [ ] **QEMU:** comprovar em runtime que a janela está no TALC (`[CTX] COMPACT` + queda de `hist[0]`) e **quantificar** os bytes de bump que o roteamento devolve

## ✅ s442 — Anti-fragmentação do TALC: IA-observa → IA-age (vocabulário fechado + HITL + efeito medido)

- [x] `hermes::anti_frag` com `AntiFragCmd` (vocabulário FECHADO): `evict_kv` / `drop_kv` / `reset_moe_cache` / `no_action` — texto livre do LLM é rótulo, nunca comando
- [x] Seams reais em `cortex`: `kv_h2o_evict_global` (H2O no KV global), `kv_global_pages`, `kv_global_len` (`None` = KV em voo → no-op honesto, fail-closed)
- [x] HITL `Escalate` no `ApprovalGate` (`/approve <id>` / `/deny <id>`), TTL de 10 min, `try_lock` com limite (nunca `lock()` atravessando publicação no bus)
- [x] `verify(before, after)`: só é `Improved` se o maior gap cresceu; `Worse` se uso subiu; `partial=1` não afirma nada; **delay de 3 s antes de medir** (cache 2 Hz — medir cedo compara a amostra com ela mesma); cooldown de 5 min após agir
- [x] Gates: mesma régua de fragmentação da s435 (uma só verdade), sem claim/amostra parcial/sem modelo/sem headroom → observe-only
- [x] 21 testes host + **mutation 8/8**
- [x] Validação: anti_frag 22/22, hermes 309/310 (pré-existente de `permission_gate`), cortex 126/126, `cargo nk` 0 erros; strings no kernel.elf
- [ ] **QEMU:** boot com TALC fragmentado de verdade → `fragmentacao detectada` → `HITL #` → `/approve` → `efeito:`

## ✅ s441 — Fuzz host de `talc_walk_bins` (gap-lists corrompidas, guards de memória)

- [x] Módulo `talc_walk_fuzz` (k_nano, 13 testes): scratch `[span | redzone]` com `size=32` no redzone (detecta leitura fora do span sem página de guarda)
- [x] Classes cobertas: self-loop, ciclo de 2, 128 bins em loop (CAP global), node fora do span (abaixo/acme/`usize::MAX`), `size` gigante/saturante/zero, 4000 gaps sobrepostos (soma > span), desalinhado, bins nulo, span vazio
- [x] **G1 (achado):** `node` perto de `acme` lia `size` FORA do span (→ `#PF` num span demand-paged) — agora exige `node + 24B (MIN_CHUNK_SIZE) <= acme`
- [x] **G2 (achado):** `next` desalinhado = `read_volatile` desalinhado (UB) — agora exige `node % align_of::<usize>() == 0`
- [x] Fuzz dirigido 400 casos + cauda 2000 casos, xorshift determinístico, budget de 2s por walk (prova de "não pende")
- [x] Contrapeso: `walk_over_real_talc_is_never_partial` — `Talc` real (`claim`/`malloc`/`free`) tem que dar `partial=0` (guards não rejeitam o allocator de produção)
- [x] **Mutation testing 5/5:** sem footprint → `STATUS_ACCESS_VIOLATION`; sem align → UB check abort; CAP inflado → self-loop FAILED; sem bounds → ACCESS_VIOLATION; sem saturating → passou (**inalcançável**, documentado no código)
- [x] Validação: k-nano **265/265** `-t1`, cortex 126/126 `-t1`, `cargo nk` 0 erros (hermes 287/288 — falha pré-existente de `permission_gate`, provada com `git show HEAD`)

## ✅ s435 — Telemetria de uso REAL do TALC (HUB HEALTH + hub_triage, idea #630 residual)

- [x] `talc_walk_bins`: walk dos gap-nodes dos bins (layout confirmado no fonte talc 4.4.3; CAP 4096 + bounds-check → partial=1 honesto; zero alloc)
- [x] Cache 2 Hz (`talc_refresh_usage`/`talc_usage`/`talc_usage_samples`) + seed pós-claim; walk nunca no caminho de alloc
- [x] Headroom honesto: `heap_headroom_bytes`/`heap_observe` somam o FREE medido (span inteiro aposentado); HeapObserve +5 campos talc_*
- [x] HUB HEALTH linha `talc` (21ª): `u{}M/{}M lg{}M g{}`, Warn de fragmentação (free≥256MB e largest×4<free), `partial g{}`, `n/a` sem amostra
- [x] hub_triage: snapshot JSON com talc_used/free/largest/gaps/partial + vereditos Observe (fragmentado / metadata parcial) + slog `ok talc u...f...lg...g...`
- [x] Validação: hermes 275/275, k-nano 244/244, jarbas 134/134 (-t1); QEMU 8GB/8c log 182016: `ok talc u0M f6911M lg6911M g1` telemetria viva, zero OOM
- [x] Lab longo (2 boots 8GB/8c, logs 183000/185325): telemetria estável 1/min, zero OOM/TALC em ~23,4k ticks — MAS **novo stall silencioso determinístico** (2/2 boots, T+23334/T+23437, sem OOM): log congela no cleanup pós-`a2_proof done` (1º decode ok, out_len=4) com QEMU vivo ~1 core em spin; correlação temporal com o grow p/ teto 2030MB (T+23058/23148) e slow_slice n=22 nos 2 boots. Última linha comum: `[Log] [JARBAS] JARBAS:  and` → suspeito: serial/log lock segurado no path do echo do Jarbas pós-job.
- [x] s436a: watchdog de silêncio `[SILENCE]` (`k_nano::silence_watchdog`) — BOOT.LOG parado + timer IRQ vivo → dump de stamps de todos os cores (`irq:`/`prog: cN=`) via `puts` lock-free, observe-only, rate-limit 1/10s; choke points: dispatch_bytes (normal+NESTED) + buffer_log; progress por core no ap_idle_loop + heartbeat do bin. k-nano 249/249 (-t1, 5 novos); cargo nk 0 erros; strings provadas no uefi.img; QEMU 8GB/8c log 110255: vivo T+83156 (~20min), zero falso-positivo, zero OOM (stall do lab s435 NÃO reproduziu; disparo real pendente de stall ao vivo)
- [ ] 🔴 s436b: bughunt do stall pós-a2_proof-done (id 2305, out_len=4) — carimbar enter/exit de JarbasAgent::tick + slog no path do eco; distinguir deadlock de serial/log vs spin infer vs hlt de AP; replicar sem carregar modelo (a2_proof-only); quando reproduzir, o `[SILENCE]` entrega o `prog: cN=` do core preso
- [ ] Residual: observação de longo prazo (runtime de horas) da linha `talc` p/ calibrar o limiar largest×4<free; consolidar `talc_capacity_mb`/`talc_capacity_bytes` se 3ª cópia surgir

## ✅ s434 — Overflow TALC: causa-raiz + fail-closed de classe (idea #630)

- [x] Instrumentação: TALC_PF_OUTSIDE_SPAN, bins_avail snapshot, PF_DIAG_PT_ALLOC_FAIL, pmm counters, OOM-DIAG no handler
- [x] Fail-closed de classe: oom() saiu do `loop { hlt() }` → spin + heartbeat `[OOM-HALT]` 10s (quebrou o stall silencioso; revelou N cores parkados)
- [x] Gap 1 (s434b): Talck::realloc chama malloc interno — NULL de chunk TALC-residente sem counter → snapshot + oom() no path
- [x] Gap 2 = causa-raiz (s434c): realloc bump-residente usava default realloc (alloc puro do bump, sem overflow) → morte cega com TALC 6911MB livre → realloc bump passa pelo híbrido
- [x] Validação QEMU 8GB/8c (log 144435): **0× OOM/TALC em ~16min (T+57771, recorde; morria em T+34k)**, bump no teto com scheduler/matmuls/HubTriage vivos, stall silencioso sumiu
- [x] Residual: fragmentação de longo prazo do TALC → **s435 implementou a telemetria** (linha `talc` no HUB + veredito Observe no triage)
- [ ] alloc_zeroed bump-residente verificar no próximo bughunt

## ✅ s433 — HUB triage: proposta via LLM (heurística = fallback)

- [x] Propose → job LLM via InferQueue (reply HUB_TRIAGE_LLM) com prompt único + snapshot JSON
- [x] Parser sem serde (scanner JSON minimalista, escapes, UTF-8 lossy, CAP 256B/campo)
- [x] Decisões: Publish (HITL) / Decline `{}` (observe) / Fallback (marcadores InferQueue, gibberish, timeout 60s)
- [x] Gates headroom: should_try_llm no submit + publish_proposal_hitl recusa com heap_headroom_low
- [x] Anti-loop: fp heurístico reservado NO SUBMIT; fp da ação gerada na publicação; cooldown 10min
- [x] Agente drena replies em qualquer tick (has_pending inclui receiver + timeout — lost-wakeup s411)
- [x] QEMU 8GB/8c (log 132152): submitted id=4 → fallback-timeout honesto → HITL → Jarbas → MoE → LLM; dedupe cooldown provado
- [ ] Residual 🔴: stall silencioso pós-OOM/TALC infer_worker (3ª sessão; log congela T+33860, QEMU vivo) → idea #630 instrumentar path de overflow

## ✅ s432 — WHPX 6-lane hardening (log 213145) + triagem IA do HUB

- [x] Lane 1: hlt gate WHPX — pause-spin sob MicrosoftHv (`interrupts.rs:83-105`; hlt sem wake = stall silencioso)
- [x] Lane 2: yields audio/HUB sob heap pressionado — capture caps 4ev/8fr + drains cap 8 (`hub_health.rs:144-161`)
- [x] Lane 3: refuse-in-slice → TALC spill como serviço efetivo; huge-2MB gate OFF até validação tok/s (#626)
- [x] Lane 4: HDA LPIB-frozen estimate (`dt×48000` quando o LPIB congela)
- [x] Lane 5: submit-only proof + ps1 `-ModelKind 1b|3b`
- [x] Lane 6: posture MIN 4→8 + piso absoluto esc>=8 (fim do FAIL com 1 escalate, 11× no log 213145)
- [x] **Triagem IA do HUB (premissa máxima ADR-0088, #629):** `hub_triage.rs` — snapshot `HUB\0`+JSON 1/min, triagem worst-state pura, proposta HITL via toast + USER_INTENT, dedupe FNV + cooldown 10min
- [x] Validação QEMU 8GB/8c (log 224429, 13,9min): proposta HITL real T+36581 → intent → MoE → LLM; 4 Observe anti-loop; dedupe provado
- [ ] Residual: instrumentar OOM/TALC `infer_worker` (2× no log 224429, span 6911MB — causa do null desconhecida)

## ✅ s431 — TALC claim budget completo (cura OOM teto 2030MB)

- [x] Causa-raiz: TALC span fixo 512MB estourava com bump no teto + RAM 70% livre (foto heap 2024/2030M 99%)
- [x] Claim do HEAP_BUDGET_MB real (6912MB) em VA própria 0x400000080000 (TALC_VA_MAX antes da arena Cortex), demand-paged
- [x] TALC_SPAN_END store ANTES do claim (size-tag do fim do span fora do range = storm no claim, boot 210748)
- [x] BUMP_BUDGET_CLAMPED separado do budget real (clamp da janela não sobrescreve mais o budget do TALC)
- [x] Headroom combinado bump+TALC em heap_headroom_bytes/heap_observe
- [x] Validação QEMU 8c: 13,6min, 0 OOM/heap-fail/budget-cap, bump cheio 2030MB e sistema vivo nos 8 workers
- [x] LIÇÃO canônica: cargo nk não regenera uefi.img — cargo build -p boot + build_image + prova de string no uefi.img

## ✅ s430 — Lab QEMU 8GB/8c: 5min na UI sem freeze (goal batido)

- [x] Watchdog slice 10s→30s (decode 8c ~11s legítimo — falso positivo rodada 1)
- [x] Deadline global no-progress (refresh A2_PROOF_DEADLINE_AT_US a cada poll_slice com did)
- [x] emotion::analyze alloc-free (buf stack 512B + contains_ci; cr2=0x28)
- [x] #PF storm park por IP (interrupts_ext.rs; sistema segue nos outros cores, provado 3×)
- [x] Gates headroom 48→128MB (InferQueue submit + claim re-check; BEI low_mem)
- [x] BEI guards: PromoteSkill re-check headroom_low (wasmi cr2=0x10); supervisor tick re-check bounded (BTreeMap cr2=0x702a00a8)
- [x] Logger no-op p/ crate `log` (LOGGER NULL+0x18; virtio-drivers/wasmi/cranelift)
- [x] Escalada I5:boot_log observe-only + mesh_frag_pressure no caller (fecha cadeia panic wasmi)
- [x] Rodada 10: 14,8min runtime, UI viva, 1 storm contido, OOM honesto final (logs 193855/195127/200959)

## ✅ s429 — VMD visão guest: tradução MMIO de offsets SHDW não-nativos

- [x] vmd.c provado antes de implementar: offset aplica-se a RECURSOS (bus = cpu − offset), NUNCA a DMA de RAM (upstream identidade no guest)
- [x] Tradução DOWNSTREAM MMIO: translate_bar puro (cpu = bus + offset dentro da janela), child_bar_cpu (guest = MEMBAR UC; nativo = phys UC)
- [x] init() sem abort em offset≠0: MEMBAR size validada (0 = abort honesto), janelas registradas, slog GUEST
- [x] NvmeDriver::probe_at_mmio_va (VA pré-resolvido pelo VMD guest); DMA de RAM do NVMe inalterado
- [x] Testes: tradução+bordas+None fora; nativo identidade; detecção guest off1\|\|off2 (k-nano 240); 0 erros
- [ ] **HW lab:** `nvme ok=true via=vmd guest` no BOOT.LOG do notebook (SHDW≠0 + MEMBARs do BIOS)

## ✅ s428 — a2_proof 120s + watchdog por slice

- [x] Deadline 300→120s (15× o pior caso mensurado 8s — pega wedge global)
- [x] Watchdog por slice: A2_SLICE_T0_US no latch; check no topo do poll_slice (fora do latch); >10s = wedge → terminal imediato
- [x] Stall real (não voltou) ≠ slice lento (voltou, A2_SLOW_SLICES) — caminhos distintos
- [x] Testes: constantes+ordem, wedge simulado terminal imediato, slice lento continua (cortex 110); 0 erros

## ✅ s427 — Loader-VRAM FAT→BAR sem heap (fase 1)

- [x] `read_root_file_dev_chunked` (k_nano): FAT32 por callback, zero Vec do blob
- [x] `loader_vram` (k_hal): probe header → layout → janelas → stream BAR → registro SEQ_MATS (lane ativo pré-load)
- [x] `on_model_loaded` no-op quando `loader_resident()` (sem upload duplo)
- [x] Gates: 0 erros; k-hal 74, k-nano 237
- [ ] **Residual:** stub dos packed no `load_llm_v6` quando `loader_resident()` — aí o heap NUNCA segura os pesos (liberação real)
- [ ] **HW lab:** `LOADER-VRAM streaming` no BOOT.LOG + lane ativo antes do load

## ✅ s426 — Card HUD Hints H3 (validação ao vivo)

- [x] `jarbas::cards::hints_card` (ID 8003): stage/fwd/n/vram resid direto dos statics, affordance do critério de aceite (fwd<100µs)
- [x] F11 toggle + `close_card_by_id` no compositor + refresh 2 Hz idempotente por id + help
- [x] Gates: 0 erros; jarbas 123/123 (damage_tests host skipado, pré-existente via stash), k-hal 71
- [ ] **HW lab:** F11 no notebook → `fwd` verde (<100µs) e `hints n` crescendo ~2/s = H3 fechado

## ✅ s425 — Linha `bootlog` no HUB HEALTH (falha de flush visível)

- [x] `boot_logger::hub_log_line()` — ok n<N> / fail backend+razão+streak / fail sem-backend / pre-fat / n/a (statics LAST_FAIL_KIND/LAST_TRY_BACKEND nos paths de falha)
- [x] Linha `bootlog` no HUB HEALTH (row 19, HUB_ROWS 20) + SysInfoAgent com a mesma string no slog
- [x] Gates: 0 erros; k-nano 237, jarbas hub 7/7, hermes 257 (t=1)
- [ ] **HW lab:** ver `bootlog fail sem-backend` (ou ok) no painel no notebook

## ✅ s424 — BOOT.LOG de fallback na ESP (evidência de boot)

- [x] Kernel: `overwrite_boot_log` não aborta em IoFail de UMA partição — continua p/ ESP (contrato `OverwriteResult` intacto)
- [x] Build: `mk_esp_fat.py` embute BOOT.LOG raiz pré-alocado 256KB na ESP (mesmo mecanismo, zero FS novo)
- [x] Fix latente: write do dirent calcula o setor real do `entry` (antes `sector_idx*bps` como byte offset)
- [x] Validação: ESP de teste 128MB — dirent BOOT.LOG 256KB na raiz, chain legível, BPB do parser OK; k-nano 237; check 0 erros
- [ ] **HW lab:** flush na ESP visível no D: montável no Windows quando o volume de dados falhar

## ✅ s423 — Power-on dGPU D3→D0 (H3 sobrevive ao D-state)

- [x] `k_hal/src/gpu/gpu_power.rs`: `wake_to_d0` (prova de vida antes do write PMCSR, budget TSC 10ms, re-scan persistido) + `wake_all` — 3 testes
- [x] Wire: `wake_all` no boot pós `detect_all()`; backstop idempotente em `init_vram_tier` + `nvidia::probe`
- [x] Gates: check release 0 erros; k-hal 71 (68+3); CHANGELOG/STATE/SESSION/AGENTS
- [ ] **HW lab:** boot no notebook → `GPUPWR woke` + HINT stage Resident (`hints on vram=3KB fwd<100µs`) + lane VRAM `vram_served=true`

## ✅ s422 — Intel VMD binder (NVMe notebooks Alder Lake+)

- [x] Driver VMD R0 `k_nano/src/vmd.rs` (CFGBAR=ECAM, busn_start VMCAP/VMCONFIG, scan flat, enable config MMIO, SHDW nativo ⇒ DMA direto, fail-closed honesto) — 5 testes
- [x] Refactor NVMe mínimo: `NvmeDriver::probe_at_mmio(bar0)`; probe nativo delega
- [x] Fallback no `probe_storage_drivers` → vmd::init + probe_vmd_nvme → global `NVME_DRIVER` (bin intocado)
- [x] Gates: check release 0 erros; k-nano 237 (232+5); TECNOLOGIAS 6.9 + CHANGELOG + STATE + SESSION_420 addendum
- [ ] **HW lab:** boot no notebook → `nvme ok=true via=vmd` + BOOT.LOG persistindo no NVMe + card sem `8086:a77f sem driver`

---

## ✅ MARCO S360 — UI + mesh + compute distribuído

- [x] Desktop Jarbas funcional (orb + Hub Health + compositor) no WHPX (lab 6-node S360/ADR-0081/SESSION_360)
- [x] Rede mesh 6 QEMU (3G/3c + 2G/2c + 4×1G/1c) + hub L2 (`tools/qemu_l2_hub.py`)
- [x] Computação distribuída FRAG matmul Master↔peers (Memory/Compute/Worker)
- [x] Fail/warn honesty (TLSPINS/Trust/CapGate/mouse soft-F4)
- [ ] Peer B estável em toda a topologia 6-node (residual S360)
- [ ] Re-boot mesh pós-fix s360 (AWAITING operador) — S455 (SESSION_455, 2 instâncias 6c/6GB) é re-teste separate, não o lab

---

## 🎯 OBJETIVOS

1. **Aceite metal pós-s328** — flash `usb_hw.img` e validar InferQueue/UI/BOOT.LOG no Alienware.
2. **Gate v2.0.0** — fechar ADR-0100 Ondas 0–3 + review formal + OK maintainer.
3. **Emagrecer `k_nano`** — primitivos R0 + hooks (ADR-0103; FASE A–H feita, S1 metal pendente).
4. **Sandbox Ring3 CPL=3** — um sandbox para blob nativo B/C; wasmi default até T-055 (ADR-0102).
5. **Kernel ternário-nativo** — Falcon3-3B 1.58-bit com ADD/SUB/SKIP packed no metal (ADR-0101).
6. **Observabilidade de boot** — 3 canais + `BOOT SCORE` + instrumentos FB (ADR-0092).
7. **Desktop Jarbas v2** — production-grade (ADR-0090); **s360 = desktop vivo + mesh no orb** ✅ parcial.
8. **SMP per-CPU runqueue** — feature ON; falta aceite metal (ADR-0089).
9. **Mesh cluster** — escalar 6-node lab (S360) → HW LAN real (ADR-0081).

---

## 🔥 ABERTOS NO METAL (s316–s328)

| Item | Evidência | Estado |
|------|-----------|--------|
| Freeze determinístico @ tick 1370 `network_agent` | SESSION_316 / STATE s327 | ABERTO — discriminador heartbeat (s326) aguarda boot no metal |
| xHCI MSC `CCS=0` pós-HCRST (HCRST deixa PP=0) | s327 (fix PP=1 RMW) | 🟡 wired; validar metal |
| UI/splash first-frame com MSC sem teto | s315/s296 | 🟡 wired; validar metal |
| BOOT.LOG `E:\BOOT.LOG` real | 0103 S1 | ▶️ AWAITING_OPERATOR |

## ⚠️ SAÚDE DE TESTES / CI

- [x] **s367** — `cargo test -p cortex --lib llm_response_gate` **7/7** + `infer_queue::tests` **5/5** (`--test-threads=1`); lab `tools/lab-llm-response.ps1` (QEMU decode AWAITING)
- [x] **s368** — workspace host **0 fail** (`--exclude neural-kernel,boot --no-fail-fast`); fixture GGUF; soft_stride/MHI/boot_observe locks; `apply_one_layer` poison on refuse
- [x] Fixture do teste `cortex`: `python tools/gen_test_gguf.py` → `target/test_tq2_0.gguf`
- [ ] CI job nightly = mesmo comando workspace (wire)

## 📌 PÓS-s328 (ordem do STATE)

`aceite metal → Prefill AirLLM layer-yield → métricas/budget TSC → Layer S (GPU/NPU WS-D/E)`

- [ ] Aceite metal: orb+mouse+mic vivos durante o generate; 1ª frase TTS antes de `LLM_RESPONSE`
- [ ] Prefill AirLLM: layer-yield (evitar hiccup no prompt)
- [ ] Métricas/budget por TSC
- [x] **ADR-0105 B0–B3** código (FAT/aliases, CpuOnly, CPU W2A8+gaps, telemetria). **Aberto:** B1.2/B2.4/B4 device golden AWAITING_HW.

---

## ITENS (priorizados: mais nova → mais antiga)

### 1. ADR-0103 — k_nano microkernel modular
**Goal:** `k_nano` = primitivos R0 + hooks; política em `k_hal`/`k_ai`/`cortex`/`hermes`/`jarbas`. Sem virar process-OS.

- [x] **FASE A** — 13 módulos mortos deletados (~1800 LOC)
- [x] **FASE B** — NIC drivers → `k_hal::net` (facades e1000/rtl8139/i225/virtio_net)
- [x] **FASE C** — FS primitivos → `k_hal::fat_assets` canônico (+ `legacy read_mbr` re-exports)
- [x] **FASE D** — sgdb canônico em `k_ai`; telemetry mantida R0 (8 callers hermes)
- [~] **FASE G+H** — analisadas, **não migradas** (dependências + risco)
- [ ] **S1** — `k_hal::usb` hub→MSC + early `probe_and_install` → `E:\BOOT.LOG` real (▶️ AWAITING_OPERATOR; ver "Abertos no metal")
- [ ] **S3** — Leitores FS não-boot órfãos (ntfs/btrfs/ext2 read-only) → crate ou delete
- [ ] **S4** — Storage cognitivo (tickv FE, rollback UI) → k_ai/hermes
- [ ] **S5** — Podar exports mortos + `check_duplication.py` limpo
- [ ] **S6** — (Opcional) esqueleto `arch/memory/scheduler/` sem mover lógica
- [ ] **Fase 2** — Schemes CPL=3 (🔴 gated: ADR-0102 aceite HW)

### 2. ADR-0102 — Ring3 sandbox CPL=3
**Goal:** um sandbox CPL=3 para blob B/C; `isolation_ring_available()==false` ⇒ só wasmi.

- [x] **H1** — Feature `ring3` propaga no bin (`neural-kernel` `default = ["ring3","cap-demos","smp-runqueue"]`)
- [x] **H2** — Demos P6 reais no tree (`demo_ring3*` em `k_nano::paging`; SESSION_302)
- [x] **H3** — `ring3_can_iretq()` + `can_register_native()` cindidos + self-test wired no boot
- [~] Demos P6 rodam no QEMU 4c (`ring3_can_iretq=true` + Jarbas greeting; SESSION_305); faltas contidas non-fatal
- [x] **T-051** — Separar `#GP` OVMF de `#GP` kernel (`gp_fault_class` / `gp_likely_firmware`) — código ✅; WHPX flaky fora do caminho crítico
- [ ] **T-052** — Metal: iretq+CPL3 + fault-containment ▶️ **AWAITING_HW** (depende Onda 2 SMP metal)
- [ ] **T-053** — Checklist 0077 §6 em HW ▶️ **AWAITING_HW** (HITL `/ring3 approve` marca gate; aceite = notebook)
- [x] **T-054** — `stash_native_runner` + HITL Escalate `ring3_register` → `promote_native_ring_if_ready` (SESSION_347)
- [ ] **T-055** — `isolation_ring_available()==true` só após T-053 metal (código gated; aceite HW pendente)
- [x] **T-056** — Fronteira xmm: `verify_blob_no_simd` antes do `iretq`
- [x] **T-057** — CapGate deny DMA/PIN/FB a CPL=3 (`sandbox_syscalls`)

**Operador:** `/ring3 status` | `/ring3 approve` → `/approve <id>`. QEMU nunca auto-registra (hv≠None).
### 3. ADR-0101 — Falcon3-3B Cognitive Lab
**Goal:** decode m=1 do 3B faz ADD/SUB/SKIP packed no SIMD do `x86_64-unknown-none`.

- [x] **Onda 0** — Inventário 3B-first (`falcon3_boot_names`, `fat_names_for(Active)` com `FALCON3.V6`)
- [x] **Onda 0** — SSE2 ADD/SUB/SKIP real + scalar ternário-nativo (`bitnet_sse.rs`, paridade vs scalar; SESSION_348)
- [x] **Onda 0** — GGUF inferência wired (TQ2_0 + BF16 + auto-config; SESSION_309)
- [~] **Onda 0** — AVX2 no target `none` (defer: metal usa SSE; host AVX2 ainda FMA dequant)
- [x] **Onda 1** — Shortlist logits + Medusa/n-gram wired + KV H2O telemetria/InferQueue (SESSION_348)
- [x] **Onda 2** — Difficulty gate no generate/InferQueue (policy budget; **não** KL early-exit)
- [x] **Onda 3** — `cognitive_runtime` composição + `/cog` status (SGDB continua prompt-side)

### 4. ADR-0100 — Backlog unificado K³CHJ (plano-mestre)
**Goal:** gate v2.0.0 = Onda 0 + Onda 1 (mín. T-011) + Onda 2 (um metal) + Onda 3 (A2 ou A3).

- [x] **Onda 0** — Honesty `BOOT_AI` + freeze HardwareInfo (T-001–T-006)
- [x] **Onda 1** — `measure_bandwidth` + `/hw/storage|gpu|net` (T-007–T-016)
- [ ] **Onda 2** — Metal K23 `online==madt-1` (T-017 img ✅; T-018–T-021 ▶️ metal)
- [ ] **Onda 3** — 0086 A2–A8 (T-022–T-032; A1/A9 HITL)
- [ ] **Onda 4** — `ap_pollable` + runqueue 0089 (T-033–T-044) 🟡 (feature 0089 já ON)
- [~] **Onda 6** — Ring3: código-complete (SESSION_347 HITL `/ring3`); metal T-052/053 ▶️ AWAITING_HW
- [ ] **Onda 7** — W2A8 gated + 0078 só Fase 1 (T-058–T-065)
- [ ] **Onda 8** — Golden GPU/SDMA/NPU (T-066–T-069) ▶️
- [ ] **Onda 9** — 0058 S5 um widget + A/V (T-070–T-072)
- [ ] **Onda 10** — AirLLM DMA/e2e (T-073–T-075) ▶️

### 5. ADR-0094 — Hermes Cleanup ✅
**Goal:** dead code excluído + testes host nos módulos ativos.

- [x] Comentar 35 módulos mortos (7.446 LOC, -23% build)
- [x] 22 testes `cognitive_bridge` + 11 testes `memory_store`
- [ ] Residual: deletar arquivos mortos do disco (manual)
- [ ] Residual: 2 testes `wasm_build` + 1 `cognitive_bridge` falhando (ver "Saúde de testes")

### 6. ADR-0093 — Jarbas Optimization ✅
**Goal:** lock-free render + caps + dirty-rect + PT-BR TTS.

- [x] 45 host tests (`jarvis.rs`) — 3 `soul_*` falhando (ver "Saúde de testes")
- [x] Mouse/theme lock-free (Atomic)
- [x] Memory caps (NotificationQueue 32, ChatWindow 100/64)
- [x] HUD string cache + dirty-rect gating
- [x] 8 fonemas PT-BR no formant
- [x] Orb JARVIS hex-grid + anti-flicker (SESSION_291); UI liveness/anti-black-screen (SESSION_315)

### 7. ADR-0092 — Boot observability
**Goal:** 3 canais (dmesg/produto/placar) + `BOOT SCORE` + instrumentos FB.

- [x] **O0** — Contrato `sev` ok|warn|fail|trace + filtro consola
- [x] **O1** — Banner `=== PHASE n= ===` + fase 8 PostRuntime
- [x] **O2** — Mudos BPB/INIT1/e1000/SIPI/scan/PnP
- [x] **O3** — `BOOT SCORE` + `tools/parse_boot_score.py`
- [x] **O4** — Sem K* no ecrã; HUD produto
- [x] **O5** — Profile qemu vs hw no placar
- [x] **Instrumentos FB** — `diag_mark`/`diag_stamp_agent`/`diag_stamp_exception`/`tick_stage`/heartbeat `T=` (s318–s327)
- [ ] **Metal** — Canal A em `E:\BOOT.LOG` (🟡 = ADR-0103 S1)

### 8. ADR-0091 — Migração neural-sgdb ✅
**Goal:** neural-sgdb externo como substrato de memória cognitiva.

- [x] Fase 0 — Dependência no_std
- [x] Fase 1 — TickvStorageAdapter
- [x] Fase 2 — NSGDB bridge + fallback
- [x] Fase 2.5 — Hits tipados, embedder seam, lexical default, lifecycle, scoping, cognitive ops
- [x] Fase 3 — Cortex/Hermes memory-aware
- [x] Self-Heal closed-loop via NSGDB (SESSION_316)
- [ ] Residual: migrar 75 callers gradualmente

### 9. ADR-0090 — Jarbas Desktop v2.0
**Goal:** desktop production-grade (4 Tiers / 15 features).

- [x] **Tier 1** — Glyph cache + grid pre-render (LUT seno ✅ e dock ✅ já no tree; `cargo check -p jarbas` 0 erros, 57 testes)
- [x] **Soul/Emotion** — unificação `hermes` (Emotion, Soul, SER→Affect, LoopPhase) delegando em `jarbas` (SESSION_319)
- [ ] **Tier 2** — Window animations, chat scrollback, hover states, voice waveform (~12d)
- [ ] **Tier 3** — Per-window back buffers, desktop real (~20d)
- [ ] **Tier 4** — Transformacional (~30d)

### 10. ADR-0089 — Per-CPU Run-Queues SMP
**Goal:** distribuição cooperativa de agents entre cores (`smp-runqueue`).

- [x] Run-queue slot-based + steal min-1 + telemetria (código)
- [x] Feature `smp-runqueue` no `default` do bin + `MAX_CORES=256` (s307)
- [ ] Aceite metal K23: `online==madt-1` + hybrid P/E

### 11. ADR-0045 — Pipeline de voz (auditoria s352)
**Goal:** o caminho wake→mic→STT→LLM→TTS→alto-falante com fidelidade, duplexidade explícita e latência medida.
Plano completo + evidência: `docs/implementation/2026-09-16-voice-pipeline-audit-s352.md` (IDEA_BANK #558–#566).

- [ ] **V1** Pacing do playback pelo clock (`PLAY_SAMPLES_DROPPED == 0` em `/tick 30/60/120`) + resample fora do laço de escrita — #558, #1.2
  - [x] **s368** código: `compute_mixer_want` free+TSC + host tests; aceite QEMU drops=0 residual
- [ ] **V2** Duplexidade explícita (fim do auto-barge-in) + `STT_UNCERTAIN` falado — #559, #560
- [ ] **V3** Teto de utterance + CTC incremental + ring SPSC para `AUDIO_FRAME` — #561, #562
- [ ] **V4** Wake-word: janela deslizante, log-mel + `tools/train_wakeword.py`, FPPH em holdout disjunto — #563
- [ ] **V5** `VOICE_LATENCY` + orçamento de contexto/SGDB episódico + identidade `voice_input`/HITL — #564, #565, #566
- [ ] Menores: doc drift de `audio/mod.rs` (`AudioPipelineAgent`/`MIC_CAPTURE_RING` não existem), `wake_window.min(120)`, saída dupla HDA+UAC no mixer

---

### 12. ADR-0106 — Decisões calibradas (D0–D4 + migração M0–M4)
**Goal:** juízo onde o significado importa deixa de ser cadeia de `contains()`; toda decisão carrega distribuição, confiança e abstenção explícita; θ vem da reliableza medida.
ADR + evidência: `docs/architecture/0106-decisoes-calibradas-confianca-abstencao-hitl.md` (IDEA #588). Triagem: `docs/architecture/0106-m0-triagem-sitios.md`.

- [x] **D0** Contrato `Decision<T>` (`Confidence` Q8, `margin`, `AbstainReason`, `DecisionSource`, `Q8Dist`) + `note_outcome` + acumulador de reliableza — `TODO-0106-1`
- [x] **D0** Persistência `ai/decisions/reliability` (best-effort Tickv) + contadores + `slog` ok/warn + `/decisions` — `TODO-0106-2` (HUD postura = residual leve)
- [x] **D1** `Intent::Unknown` + `decide()` canônico (score por braço) + `think()` wrapper; Hermes publica abstenção→LLM — `TODO-0106-3`
- [x] **D2** Política de θ por classe de sítio; `ApprovalGate::classify` → `site_policy` (Noul); `Deny` imune — `TODO-0106-4`
- [x] **D3** `Noul` destrutividade + `Score`/`decide_tier` ComputeTier + escape hatch Unknown — `TODO-0106-5`
- [x] **D4** Labels HITL + weak_auto → JSONL `example`; `train_router` merge (HITL×3); `/decisions correct` — `TODO-0106-6` (r3 replay contínuo = residual opcional)
- [x] **M0** Triagem canônica — `TODO-0106-7`
- [x] **M1** emotion / skill-creation — `TODO-0106-8`
- [x] **M2** plugin + marketplace — `TODO-0106-9`
- [x] **M3** Hub row `decide` + pill via `hub_posture_sev` — `TODO-0106-10`
- [x] **M4** Lista OUT formal — `docs/architecture/0106-m4-excluded-sites.md` — `TODO-0106-11`

---

## 🔗 DEPENDÊNCIAS (ordem obrigatória)

```
0100 Onda 2 (SMP metal) ──► 0102 T-052 (iretq metal) ──► 0103 Fase 2 (schemes)
        │
        └──► 0092/0089 aceite metal (0089 feature já ON)
```

## 🔴 BLOQUEIOS (não são trabalho de código)

| Item | ADR | O que falta |
|------|-----|-------------|
| S1 BOOT.LOG metal | 0103 | Operador: pendrive + `E:\BOOT.LOG` no Alienware |
| Flash `usb_hw.img` pós-s328 | — | Operador: Rufus (DD) + boot no Alienware |
| Onda 2 SMP metal | 0100 | Dois notebooks (i5 7ª + Core 7 240H), `online==madt-1` |
| T-052 iretq metal | 0102 | Depende de Onda 2 SMP metal |
| Fase 2 schemes | 0103 | Depende de 0102 aceite HW |

**Diagnóstico instrumentado (s316):** watchdog de tick lento (`[Sched] warn`), TSC visível (`ok`/`warn`), `cmd TIMEOUT ms=` — próximo boot no metal disambigua H1–H6. Checklist: `docs/memory/HW_DIAG_s316.md`.

## ✅ CONCLUÍDO PÓS-09-05 (rastreabilidade)

- s306 Dual QEMU 4c mesh Master/Worker + slog P2P visível
- s309 Falcon3 GGUF inferência wired (TQ2_0 + BF16 + auto-config)
- s315 Jarbas UI liveness + anti-black-screen (Alienware)
- s316 Self-Heal closed-loop + NSGDB + detectores de segurança; escada de instrumentos FB (s318→s327)
- s317 k_ai/cortex: ReAct + H2O + CodebookVQ
- s318 hermes/cortex FASE 1–4 (KvCache, MoE 0.05, dead code)
- s319 hermes/jarbas unificação (Emotion, Soul, SER→Affect, LoopPhase)
- s321 k_nano slimming FASE A–D + facades
- s327 freeze bisector + MSC Port Power
- s328 Full Infer D+B+C (InferQueue WS-H; ADR-0057)

## 🧾 DÍVIDA DE DOC/CONSISTÊNCIA

- [x] Licenca alinhada em s450: `TECNOLOGIAS.md` MIT->**AGPL-3.0** (header + tabela); `LICENSE`=AGPL-3.0 confirmado.
- [x] Métricas alinhadas ao **medido** em `AGENTS.md`, `SUMMARY.md`, `ROADMAP.md`, `codemap.md`, `HOWTO.md` (+ `TECNOLOGIAS.md`/`README.md`): ~169K LOC / ~672 `.rs` (12 crates do workspace) / 41 nativos / v1.9.99-s412 / 829 testes host

---

**Detalhes completos:** `docs/architecture/0100-k3chj-backlog-custo-anel.md` (T-001–T-075) · `AGENTS.md` · `docs/architecture/INDEX.md`

---

## 🔴 s446 — F1.5 fechado como PASS, persistencia bloqueada; lock do forum

- [x] `run-f15.ps1`: OVMF code+vars + `-cpu` + exit code **funcional** (o check
      anterior era quebrado: `.ExitCode` null sem handle cacheado)
- [x] `f15_parse.ps1` com escopo por nome da skill + suite `run_f15_fixtures.py` 19/19
- [x] Boot 1 **PASS** (exit 0) / boot 2 **FALSIFIED** (exit 1, `reason=not_found`) - SUPERSEDED s451: F1.5 PASS (boot1 act=gen hash=0x458653425da3b4a5 -> power cycle -> boot2 act=reuse MESMO hash, result=82).
- [x] Lock de escrita **compartilhado** do forum (py + PS), 0 id dup / 0 rasgada
- [x] `forum_repair_ids.py` (mode `"w"`) trancado no lock
- [x] Backend persistente no QEMU - RESOLVIDO s451: `[TICKV] backend=file dev=virtio` + F1.5 PASSOU (2 boots + ablacao).
      nenhum disco escrito: o eixo de persistência não fecha sem isto
- [ ] `gen_lsk1.py` com 512 bytes zerados (`BLOB_LEN=512` do `skill_lab` vs 256 do gerador)
- [ ] Warn único se 4 bytes != 0 e != `LSK1` (não setar `CONSUMED`)
- [ ] ⏳ Kani/E1: precisa de host Linux ou `wsl --install` (decisão do dono)
- [ ] Promover `target/gen_f15_fixtures.py` e `target/test_loop_locked.ps1` para
      `tools/` (hoje são gitignored e somem em `cargo clean`)

## Sprint 453 - lab QEMU 6G/8c em loop (FREEBU, lane harness/QEMU/runtime)

- [x] `tools/run-qemu-lab-loop.ps1`: 6 GB / 8c / WHPX, HW simulado maximo, restore pristine
      por ciclo, veredito por ciclo em `logs/lab_loop_cycles.csv` com carimbo de execucao (`run=`).
      **2 ciclos PASS medidos** (tick_max 26206 e 30300, 424 s, 0 #PF / 0 panic / 0 corrupcao).
- [x] Isolamento do lab (binario + imagens + portas proprios): outra thread mata todo
      `qemu-system-x86_64` e divide `target/uefi.img` + `disk_qemu.raw` + 4445/4446 + 5555.
- [x] `tools/test_run_qemu_lab_loop.ps1`: 10/10 (regressao dos 6 defeitos do instrumento).
- [x] Entrega dos artefatos que nunca chegavam ao guest (`target1/` + `models/`): o kernel
      ENCONTRA `HWEXPERT @0x13de00000` e REJEITA no parse (v3 esperado, v6 na arvore).
- [ ] **#651 `-VirtioBlk`: dry-run ok, runtime NAO medido** - falta isolar qual combinacao do
      set completo de HW colide (`drive with bus=0, unit=0 exists`). Caminho do degrau 2 do
      funil D3 (TICKV `backend=file`), o mesmo do `run-f15.ps1`.
- [ ] **#652 parser do expert aceita v6?** - decisao de kernel (dono: quem estiver no `main.rs`).
      `RUSTCDR2.BIN` (~300KB) segue inexistente na arvore; so ha `RUSTCDR3.BIN` (336MB), que
      nao cabe na janela de 2MB.
- [ ] F1aceite (8c/8GB por 1 h sem `#PF`) continua **UNKNOWN**: o lab roda 6 GB e ~7 min/ciclo.
