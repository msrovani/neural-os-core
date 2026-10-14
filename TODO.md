# 📋 TODO — neural-os-core

**Versão:** v1.9.99-s468 TEST
**Data:** 2026-10-13
**Fonte:** SESSION_468d / STATE.md + ADRs 0081/0088-0113
**Retomada:** docs/memory/RESUME.md (runbook único — comandos do F1.5, decisões D1–D5, mapa de donos da working tree, armadilhas medidas, UNKNOWN)
**Legenda:** ✅ feito | 🟡 em andamento | [~] parcial | 🔴 bloqueado | ⏳ agendado | ▶️ AWAITING_HW | [ ] pendente

---

## 🟢 s468 — BEI hardening + DynamicMoE GGUF + AP offload + traits + property tests + typed AFFECT channel

- [x] **P0.1/P0.2**: Watchpoint DR0 armed on ffect_regulator (s452); BeiState pinned to fixed HHDM frame via PMM — eliminates REMAP race in map_page_direct.
- [x] **P1**: ecord_result real metrics — CortexAgent stores LAST_LLM_SUBMIT_TICK on submit; HermesAgent computes latency (ticks) + heuristic confidence on LLM_RESPONSE, calls ei_state().record_result().
- [x] **P2**: connect_components documented honestly — lock order 
et→pc→mem→affect→supervisor→dmoe→lifecycle→budget; "PARTIAL/noop" warning removed.
- [x] **P3**: DynamicMoE loads GGUF experts — load_expert_from_gguf() parses GGUF, dequantizes tensor by name, converts to ternary via 32_to_ternary_packed, registers BitLinear in MoELayer + ExpertLifecycleManager (bits=2). AutoLearnAgent loads expert GGUF from FAT32 on learn_topic().
- [x] **P4**: BeiHeapPolicy centralizes thresholds (low=128MB, critical=48MB); ThrottledLog trait replaces 7+ ad-hoc statics (LAST_LOG, LAST_BUDGET, etc.).
- [x] **P5/Item 10**: ei_tick split — mesh/FL/CRDT offloaded to AP workers via k_nano::smp::ap_work::enqueue; new p_jobs.rs with mesh_fl_crdt_tick_job.
- [x] **Item 6**: op_ir_schema_hint() expanded with real ops + examples; model_skill_prompt uses it.
- [x] **Item 11**: ffect_regulator sync verified — Arc<Mutex> shared, BEI decay+snapshot under lock, Hermes/Display incorporate_affect, compositor reads snapshot.
- [x] **Item 12**: Traits for testability — k_ai::traits (ExpertLifecycle, BudgetManager), cortex::traits (PlasticityController).
- [x] **Item 13**: DynamicMoE property tests (8) — expert count ≤ max, merge/split/birth counts, metadata sync, entropy ≥ 0, confidence ∈ [0,1]; lush_births guards max_experts.
- [x] **Item 14**: AFFECT_SNAPSHOT typed EventBus channel — ffect_channel.rs with serialize/deserialize, typed publish/subscribe, backward-compat sync_affect_snapshot_both.
- [x] **Item 15**: ei_tick contract documented in ei.rs header — lock order, heap gates, fail-closed, mesh integration, metrics.
- [x] **Build**: cargo check --release 0 errors; cargo test -p cortex property_tests 8/8 passed.
- [x] **Image**: 	arget\usb_hw.img (8319 MB, kernel sha 2fe1c24c0ab1680) ready for metal flash.
- [ ] **Metal validation**: Flash 	arget\usb_hw.img → boot Dell (Secure Boot OFF) → capture console + HUD photos → report results.

---
# ðŸ“‹ TODO â€” neural-os-core

**VersÃ£o:** v1.9.99-s451 TEST
**Data:** 2026-10-04
**Fonte:** SESSION_451 / SESSION_450 / SESSION_449 / SESSION_448 / STATE.md + ADRs 0081/0088-0113
**Retomada:** `docs/memory/RESUME.md` (runbook Ãºnico â€” comandos do F1.5, decisÃµes D1â€“D5, mapa de donos da working tree, armadilhas medidas, UNKNOWN)
**Legenda:** âœ… feito | ðŸŸ¡ em andamento | `[~]` parcial | ðŸ”´ bloqueado | â³ agendado | â–¶ï¸ AWAITING_HW | `[ ]` pendente

---

## ðŸ“Œ s467 â€” PendÃªncias abertas (pÃ³s-tarefa s466b)

- [x] â–¶ï¸ Foto **PRE/POST do xHCI `00:14.0`** em HW real â€” **FECHADO (11/10, boot #2 metal, Core 7 240H)**: o dump completo saiu na TELA (FB console, fotos 4-5): `PRE HC=00:14.0 RS=0 port=N proto=SS/USB2 PORTSC=â€¦` Ã—16 + `xHCI[0] settle ccs=2 metal=1: P1..P16` â€” **PRE: P5/P6 USB2 CCS=1 PED=1 PLS=0 (U0), P1-P4 SS CCS=0 PLS=5, P7-P16 CCS=0; settle idÃªntico, PP=1 preservado** (a preocupaÃ§Ã£o HCRST-PP do SESSION_316/327 nÃ£o se materializou). HUD: `xhci c2 st10 00:14.0` + `usb 16p ccs4 no msc cc4 to0 dbc0 fo0 af2`. **DiagnÃ³stico metal:** Address Device falha 2Ã— (af2) com USB Transaction Error (cc4), sem timeout (to0) â†’ MSC None â†’ BOOT.LOG sem backend. Fronte s468. SESSION_467 Â§12+Â§17
- [x] `mkfat32.py`: BOOT.LOG empacotado **2Ã—** â€” **FIXADO** (entrada zeros do audit s402 removida; fica sÃ³ o append com header). Prova: `python tools/count_bootlog_dirents.py <img>` â†’ `PASS: exatamente 1 BOOT.LOG`
- [x] `regulatory.db`/`WIFI.CFG` "ausentes" do FAT32 â€” **NÃƒO empacotar** (evidÃªncia: zero consumidores em crates vivas; `WIFI.CFG` sÃ³ em `docs/archive/dead-modules/hermes/wifi_agent.rs` ORPHAN; `regulatory.db` = download s102 Ã³rfÃ£o). Arquivo morto na imagem = dÃ­vida; se o WifiAgent reviver, o pack ganha o consumer junto
- [ ] Mass Storage Reset (s465): ainda deferido â€” **agora com dado real de metal**: cc4 (USB Transaction Error) no Address Device (af2); investigar speed context/route string/TT de hub interno antes de MSG reset
- [x] **Soft off de tela sela o ramlog no clique** (`f56a5907`): os 3 botÃµes do power dialog chamam `seal_now()` (HARD seal, idempotente) + consumer `SYSTEM_HIBERNATE` (era clique morto). Imagem regenerada (kernel `7069d285`). Re-teste metal: regravar â†’ boot â†’ [Reiniciar] no dialog â†’ boot seguinte despeja BOOT.LOG (SESSION_467 Â§13)
- [x] Push dos commits `edc61ac4` (s466b) + docs s467 â€” **pushado** (pedido do maintainer 11/10; `dfaa17d6..30934e79` + tags s466/s467)

## ðŸ”¥ s449 â€” ConsolidaÃ§Ã£o do working tree: E1â€“E4 (ADR-0113) + F1 + storage AION + cleanup dead-weight

- [x] Commit de consolidaÃ§Ã£o dos 41 modificados + untracked desde a s441 (E1â€“E4 + F1 + AION-storage + dead-weight + s441â€“s445)
- [x] E1â€“E4 (ADR-0113): `mesh::p99_index`+Kani; `trust.rs` caps + `CAP_GENERATION`; `event-bus/stamp.rs` + `register_audit_hooks(sha256)`; `bench_stats.rs` + `BENCH` markers (IDEA #642)
- [x] Cleanup dead-weight: findings 1,2,3,5,6 (`matrix_learn.rs` deletado; `Oneshot`+receiver-gate; sem heartbeat falso; sem USB poll) (IDEA #643)
- [x] `cargo build --release -p boot` = 0 erros; testes isolados sÃ³ com 3 prÃ©-existentes (permission_gate, sgdb bench abort, tq2_0_gguf fixture)
- [ ] **Kani/E1/E2**: provas `#[cfg(kani)]` compilam mas nÃ£o rodam (`x86_64-unknown-none` fora do guide; WSL ausente) â€” verificaÃ§Ã£o formal = UNKNOWN (IDEA #639)
- [ ] **Validar em boot de QEMU** as frentes E1â€“E4/F1/storage/cleanup â€” o gate desta sessÃ£o foi host (`HOST PASS != RUNTIME PASS`)
- [ ] `tq2_0_gguf_load` (IDEA #644): isolar fixture vs dequant (prÃ©-existente)
- [ ] Sync n-sgdb de SESSION_449 + IDEA #642â€“#644

## ðŸ”´ s447 â€” Fechamento: fÃ³rum encerrado, F1.5 com carimbo Â§14, ablaÃ§Ã£o ausente

- [x] Â§14: sidecar `.imgid` no launcher + parser fail-closed sem ele (teste do carimbo EXIT=0, fixtures 22/22)
- [x] Â§14b: identidade comparada no parse (`bytes`+`epoch`+`sha`) â€” o carimbo parou de passar sobre imagem reconstruida depois do boot (IDEA #640); boot 1 real saiu de PASS para FALSIFIED
- [x] Gerador de fixtures versionado em `tools/gen_f15_fixtures.py` (vivia em `target/`, gitignored â€” um `git clean` apagava a suÃ­te) (IDEA #641)
- [x] ReclassificaÃ§Ã£o explÃ­cita do "boot1 PASS" â†’ `BOOT1_EXIT=1`/`BOOT2_EXIT=1` (medido: boot 1 reprova por `Â§14b`, boot 2 por `not_found` + sem sidecar)
- [x] Confirmar que `uefi.img` **nÃ£o** estÃ¡ stale (7/7 literais do AION na imagem)
- [x] Gate `tem_substancia` no watcher (4/4) + supressÃ£o contada
- [x] Encerramento: watcher parado (sem lock Ã³rfÃ£o), launcher + QEMU parados, log do boot cortado preservado
- [x] Runbook de retomada: `docs/memory/RESUME.md` + ponteiros em STATE/SESSION_INDEX/TODO/CHANGELOG
- [x] **Braco de ablaÃ§Ã£o (Â§7)** â€” `tools/f15_pristine.py` + bloco `[7]` no launcher (restore opt-in; no boot 2 Ã© a **condiÃ§Ã£o de controle**: run permitido, veredito reprovado; scan sempre) + `restore=`/`disk_lab_state_before=` no sidecar + 3 regras fail-closed; fixtures 26/26, ablaÃ§Ã£o 17/17, stamp exit 0 (IDEA #638, s448)
- [x] **QEMU com `-RestorePristine`** - ablacao EXECUTADA s451 (disco pristine -> `escalate=reuse reason=not_found`, comportamento reverte). D2 fechado.
- [x] `TICKV backend=file` no QEMU + os 2 boots com veredito (sidecar + identidade 14b) - DONE s451: `backend=file dev=virtio`, F1.5 PASSOU.
- [ ] Perguntar ao maintainer se o loop do OPMUSE (PID 9908) continua â€” Ã© a Ãºnica coisa que ainda escreve no fÃ³rum encerrado
- [ ] Sync n-sgdb de SESSION_447 + IDEA #638/#639/#640/#641 (servidor vivo, sem tool MCP nesta sessÃ£o)
- [x] `cargo build --release -p boot` desta arvore - 0 erros (s451).

## ðŸŸ¡ s445 â€” Forum CURAIX: recover no wipe + reload pos-Tickv (F1.5 aberto)

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

## âœ… s444 â€” A lane anti-fragmentaÃ§Ã£o usa o histograma (Dust x Benign)

- [x] `anti_frag::FragKind` + `frag_kind(hist)` (puro): `Dust` (>=60% de gaps <8KB), `Benign` (<=20%), `Mixed` (sem dominante **ou** sem histograma)
- [x] FragmentaÃ§Ã£o **benigna** vira observe-only **antes** de chamar a IA â€” nÃ£o gasta job LLM nem HITL humano em aÃ§Ã£o sem efeito (loop de feedback da s429-lab)
- [x] Slog passa a declarar a forma: `fragmentacao Dust detectada (...)`
- [x] 6 testes novos (28 no total) + **mutation 13/13** (M9..M13); `limiares_sao_exatos` fixa 60%/20%/30% exatos
- [x] ValidaÃ§Ã£o: anti_frag **28/28**, hermes **315/316** (prÃ©-existente), `cargo nk` 0 erros; restauraÃ§Ã£o pÃ³s-mutation conferida por SHA-256

## âœ… s443 â€” 2Âº consumer de longa vida no TALC (`ContextWindow`) + histograma de fragmentaÃ§Ã£o

- [x] `k_nano::allocator::TalcBuf`: buffer POSSUÃDO com `Drop` que devolve o chunk ao TALC (free real), fail-closed (`None`/`false`, conteÃºdo intacto), `Send` sem `Sync`
- [x] `grow` **sem `realloc`** (liÃ§Ã£o s434b: `HybridAllocator::realloc` em chunk TALC-resident chama `oom()` = derruba o kernel) â†’ alloc+copy+free, NULL honesto
- [x] `ContextWindow` migrado (`role`/`content`/`system_prompt`): fecha o **vazamento monotÃ´nico** do bump (churn de compactaÃ§Ã£o nunca devolvia memÃ³ria); `add()` â†’ `bool`; `talc_bytes()`
- [x] **Histograma de fragmentaÃ§Ã£o**: `talc_walk_bins` conta gaps por faixa (`TALC_HIST_BOUNDS`), sai no slog (`hist=a/b/c/d/e/f`), no snapshot JSON e em `HeapObserve`
- [x] `tools/talc_frag_report.py`: lÃª log real, classifica dust-dominant x benigno, exporta CSV (testado ponta a ponta, `EXIT=0`)
- [x] Testes: k-nano **276/276** (+11), k_ai context_window **5/5**, `cargo nk` 0 erros; `hist=`/`TalcBuf` no kernel.elf
- [x] `tools/test_talc_frag_report.py` **3/3**: teste de contrato que monta a linha a partir do template real do slog em `hub_triage.rs` (formato Ã© contrato, SESSION_411) â€” inclui log prÃ©-s443 (sem hist) e log sem telemetria (`!= 0`, nÃ£o finge relatÃ³rio)
- [ ] **QEMU:** comprovar em runtime que a janela estÃ¡ no TALC (`[CTX] COMPACT` + queda de `hist[0]`) e **quantificar** os bytes de bump que o roteamento devolve

## âœ… s442 â€” Anti-fragmentaÃ§Ã£o do TALC: IA-observa â†’ IA-age (vocabulÃ¡rio fechado + HITL + efeito medido)

- [x] `hermes::anti_frag` com `AntiFragCmd` (vocabulÃ¡rio FECHADO): `evict_kv` / `drop_kv` / `reset_moe_cache` / `no_action` â€” texto livre do LLM Ã© rÃ³tulo, nunca comando
- [x] Seams reais em `cortex`: `kv_h2o_evict_global` (H2O no KV global), `kv_global_pages`, `kv_global_len` (`None` = KV em voo â†’ no-op honesto, fail-closed)
- [x] HITL `Escalate` no `ApprovalGate` (`/approve <id>` / `/deny <id>`), TTL de 10 min, `try_lock` com limite (nunca `lock()` atravessando publicaÃ§Ã£o no bus)
- [x] `verify(before, after)`: sÃ³ Ã© `Improved` se o maior gap cresceu; `Worse` se uso subiu; `partial=1` nÃ£o afirma nada; **delay de 3 s antes de medir** (cache 2 Hz â€” medir cedo compara a amostra com ela mesma); cooldown de 5 min apÃ³s agir
- [x] Gates: mesma rÃ©gua de fragmentaÃ§Ã£o da s435 (uma sÃ³ verdade), sem claim/amostra parcial/sem modelo/sem headroom â†’ observe-only
- [x] 21 testes host + **mutation 8/8**
- [x] ValidaÃ§Ã£o: anti_frag 22/22, hermes 309/310 (prÃ©-existente de `permission_gate`), cortex 126/126, `cargo nk` 0 erros; strings no kernel.elf
- [ ] **QEMU:** boot com TALC fragmentado de verdade â†’ `fragmentacao detectada` â†’ `HITL #` â†’ `/approve` â†’ `efeito:`

## âœ… s441 â€” Fuzz host de `talc_walk_bins` (gap-lists corrompidas, guards de memÃ³ria)

- [x] MÃ³dulo `talc_walk_fuzz` (k_nano, 13 testes): scratch `[span | redzone]` com `size=32` no redzone (detecta leitura fora do span sem pÃ¡gina de guarda)
- [x] Classes cobertas: self-loop, ciclo de 2, 128 bins em loop (CAP global), node fora do span (abaixo/acme/`usize::MAX`), `size` gigante/saturante/zero, 4000 gaps sobrepostos (soma > span), desalinhado, bins nulo, span vazio
- [x] **G1 (achado):** `node` perto de `acme` lia `size` FORA do span (â†’ `#PF` num span demand-paged) â€” agora exige `node + 24B (MIN_CHUNK_SIZE) <= acme`
- [x] **G2 (achado):** `next` desalinhado = `read_volatile` desalinhado (UB) â€” agora exige `node % align_of::<usize>() == 0`
- [x] Fuzz dirigido 400 casos + cauda 2000 casos, xorshift determinÃ­stico, budget de 2s por walk (prova de "nÃ£o pende")
- [x] Contrapeso: `walk_over_real_talc_is_never_partial` â€” `Talc` real (`claim`/`malloc`/`free`) tem que dar `partial=0` (guards nÃ£o rejeitam o allocator de produÃ§Ã£o)
- [x] **Mutation testing 5/5:** sem footprint â†’ `STATUS_ACCESS_VIOLATION`; sem align â†’ UB check abort; CAP inflado â†’ self-loop FAILED; sem bounds â†’ ACCESS_VIOLATION; sem saturating â†’ passou (**inalcanÃ§Ã¡vel**, documentado no cÃ³digo)
- [x] ValidaÃ§Ã£o: k-nano **265/265** `-t1`, cortex 126/126 `-t1`, `cargo nk` 0 erros (hermes 287/288 â€” falha prÃ©-existente de `permission_gate`, provada com `git show HEAD`)

## âœ… s435 â€” Telemetria de uso REAL do TALC (HUB HEALTH + hub_triage, idea #630 residual)

- [x] `talc_walk_bins`: walk dos gap-nodes dos bins (layout confirmado no fonte talc 4.4.3; CAP 4096 + bounds-check â†’ partial=1 honesto; zero alloc)
- [x] Cache 2 Hz (`talc_refresh_usage`/`talc_usage`/`talc_usage_samples`) + seed pÃ³s-claim; walk nunca no caminho de alloc
- [x] Headroom honesto: `heap_headroom_bytes`/`heap_observe` somam o FREE medido (span inteiro aposentado); HeapObserve +5 campos talc_*
- [x] HUB HEALTH linha `talc` (21Âª): `u{}M/{}M lg{}M g{}`, Warn de fragmentaÃ§Ã£o (freeâ‰¥256MB e largestÃ—4<free), `partial g{}`, `n/a` sem amostra
- [x] hub_triage: snapshot JSON com talc_used/free/largest/gaps/partial + vereditos Observe (fragmentado / metadata parcial) + slog `ok talc u...f...lg...g...`
- [x] ValidaÃ§Ã£o: hermes 275/275, k-nano 244/244, jarbas 134/134 (-t1); QEMU 8GB/8c log 182016: `ok talc u0M f6911M lg6911M g1` telemetria viva, zero OOM
- [x] Lab longo (2 boots 8GB/8c, logs 183000/185325): telemetria estÃ¡vel 1/min, zero OOM/TALC em ~23,4k ticks â€” MAS **novo stall silencioso determinÃ­stico** (2/2 boots, T+23334/T+23437, sem OOM): log congela no cleanup pÃ³s-`a2_proof done` (1Âº decode ok, out_len=4) com QEMU vivo ~1 core em spin; correlaÃ§Ã£o temporal com o grow p/ teto 2030MB (T+23058/23148) e slow_slice n=22 nos 2 boots. Ãšltima linha comum: `[Log] [JARBAS] JARBAS:  and` â†’ suspeito: serial/log lock segurado no path do echo do Jarbas pÃ³s-job.
- [x] s436a: watchdog de silÃªncio `[SILENCE]` (`k_nano::silence_watchdog`) â€” BOOT.LOG parado + timer IRQ vivo â†’ dump de stamps de todos os cores (`irq:`/`prog: cN=`) via `puts` lock-free, observe-only, rate-limit 1/10s; choke points: dispatch_bytes (normal+NESTED) + buffer_log; progress por core no ap_idle_loop + heartbeat do bin. k-nano 249/249 (-t1, 5 novos); cargo nk 0 erros; strings provadas no uefi.img; QEMU 8GB/8c log 110255: vivo T+83156 (~20min), zero falso-positivo, zero OOM (stall do lab s435 NÃƒO reproduziu; disparo real pendente de stall ao vivo)
- [ ] ðŸ”´ s436b: bughunt do stall pÃ³s-a2_proof-done (id 2305, out_len=4) â€” carimbar enter/exit de JarbasAgent::tick + slog no path do eco; distinguir deadlock de serial/log vs spin infer vs hlt de AP; replicar sem carregar modelo (a2_proof-only); quando reproduzir, o `[SILENCE]` entrega o `prog: cN=` do core preso
- [ ] Residual: observaÃ§Ã£o de longo prazo (runtime de horas) da linha `talc` p/ calibrar o limiar largestÃ—4<free; consolidar `talc_capacity_mb`/`talc_capacity_bytes` se 3Âª cÃ³pia surgir

## âœ… s434 â€” Overflow TALC: causa-raiz + fail-closed de classe (idea #630)

- [x] InstrumentaÃ§Ã£o: TALC_PF_OUTSIDE_SPAN, bins_avail snapshot, PF_DIAG_PT_ALLOC_FAIL, pmm counters, OOM-DIAG no handler
- [x] Fail-closed de classe: oom() saiu do `loop { hlt() }` â†’ spin + heartbeat `[OOM-HALT]` 10s (quebrou o stall silencioso; revelou N cores parkados)
- [x] Gap 1 (s434b): Talck::realloc chama malloc interno â€” NULL de chunk TALC-residente sem counter â†’ snapshot + oom() no path
- [x] Gap 2 = causa-raiz (s434c): realloc bump-residente usava default realloc (alloc puro do bump, sem overflow) â†’ morte cega com TALC 6911MB livre â†’ realloc bump passa pelo hÃ­brido
- [x] ValidaÃ§Ã£o QEMU 8GB/8c (log 144435): **0Ã— OOM/TALC em ~16min (T+57771, recorde; morria em T+34k)**, bump no teto com scheduler/matmuls/HubTriage vivos, stall silencioso sumiu
- [x] Residual: fragmentaÃ§Ã£o de longo prazo do TALC â†’ **s435 implementou a telemetria** (linha `talc` no HUB + veredito Observe no triage)
- [ ] alloc_zeroed bump-residente verificar no prÃ³ximo bughunt

## âœ… s433 â€” HUB triage: proposta via LLM (heurÃ­stica = fallback)

- [x] Propose â†’ job LLM via InferQueue (reply HUB_TRIAGE_LLM) com prompt Ãºnico + snapshot JSON
- [x] Parser sem serde (scanner JSON minimalista, escapes, UTF-8 lossy, CAP 256B/campo)
- [x] DecisÃµes: Publish (HITL) / Decline `{}` (observe) / Fallback (marcadores InferQueue, gibberish, timeout 60s)
- [x] Gates headroom: should_try_llm no submit + publish_proposal_hitl recusa com heap_headroom_low
- [x] Anti-loop: fp heurÃ­stico reservado NO SUBMIT; fp da aÃ§Ã£o gerada na publicaÃ§Ã£o; cooldown 10min
- [x] Agente drena replies em qualquer tick (has_pending inclui receiver + timeout â€” lost-wakeup s411)
- [x] QEMU 8GB/8c (log 132152): submitted id=4 â†’ fallback-timeout honesto â†’ HITL â†’ Jarbas â†’ MoE â†’ LLM; dedupe cooldown provado
- [ ] Residual ðŸ”´: stall silencioso pÃ³s-OOM/TALC infer_worker (3Âª sessÃ£o; log congela T+33860, QEMU vivo) â†’ idea #630 instrumentar path de overflow

## âœ… s432 â€” WHPX 6-lane hardening (log 213145) + triagem IA do HUB

- [x] Lane 1: hlt gate WHPX â€” pause-spin sob MicrosoftHv (`interrupts.rs:83-105`; hlt sem wake = stall silencioso)
- [x] Lane 2: yields audio/HUB sob heap pressionado â€” capture caps 4ev/8fr + drains cap 8 (`hub_health.rs:144-161`)
- [x] Lane 3: refuse-in-slice â†’ TALC spill como serviÃ§o efetivo; huge-2MB gate OFF atÃ© validaÃ§Ã£o tok/s (#626)
- [x] Lane 4: HDA LPIB-frozen estimate (`dtÃ—48000` quando o LPIB congela)
- [x] Lane 5: submit-only proof + ps1 `-ModelKind 1b|3b`
- [x] Lane 6: posture MIN 4â†’8 + piso absoluto esc>=8 (fim do FAIL com 1 escalate, 11Ã— no log 213145)
- [x] **Triagem IA do HUB (premissa mÃ¡xima ADR-0088, #629):** `hub_triage.rs` â€” snapshot `HUB\0`+JSON 1/min, triagem worst-state pura, proposta HITL via toast + USER_INTENT, dedupe FNV + cooldown 10min
- [x] ValidaÃ§Ã£o QEMU 8GB/8c (log 224429, 13,9min): proposta HITL real T+36581 â†’ intent â†’ MoE â†’ LLM; 4 Observe anti-loop; dedupe provado
- [ ] Residual: instrumentar OOM/TALC `infer_worker` (2Ã— no log 224429, span 6911MB â€” causa do null desconhecida)

## âœ… s431 â€” TALC claim budget completo (cura OOM teto 2030MB)

- [x] Causa-raiz: TALC span fixo 512MB estourava com bump no teto + RAM 70% livre (foto heap 2024/2030M 99%)
- [x] Claim do HEAP_BUDGET_MB real (6912MB) em VA prÃ³pria 0x400000080000 (TALC_VA_MAX antes da arena Cortex), demand-paged
- [x] TALC_SPAN_END store ANTES do claim (size-tag do fim do span fora do range = storm no claim, boot 210748)
- [x] BUMP_BUDGET_CLAMPED separado do budget real (clamp da janela nÃ£o sobrescreve mais o budget do TALC)
- [x] Headroom combinado bump+TALC em heap_headroom_bytes/heap_observe
- [x] ValidaÃ§Ã£o QEMU 8c: 13,6min, 0 OOM/heap-fail/budget-cap, bump cheio 2030MB e sistema vivo nos 8 workers
- [x] LIÃ‡ÃƒO canÃ´nica: cargo nk nÃ£o regenera uefi.img â€” cargo build -p boot + build_image + prova de string no uefi.img

## âœ… s430 â€” Lab QEMU 8GB/8c: 5min na UI sem freeze (goal batido)

- [x] Watchdog slice 10sâ†’30s (decode 8c ~11s legÃ­timo â€” falso positivo rodada 1)
- [x] Deadline global no-progress (refresh A2_PROOF_DEADLINE_AT_US a cada poll_slice com did)
- [x] emotion::analyze alloc-free (buf stack 512B + contains_ci; cr2=0x28)
- [x] #PF storm park por IP (interrupts_ext.rs; sistema segue nos outros cores, provado 3Ã—)
- [x] Gates headroom 48â†’128MB (InferQueue submit + claim re-check; BEI low_mem)
- [x] BEI guards: PromoteSkill re-check headroom_low (wasmi cr2=0x10); supervisor tick re-check bounded (BTreeMap cr2=0x702a00a8)
- [x] Logger no-op p/ crate `log` (LOGGER NULL+0x18; virtio-drivers/wasmi/cranelift)
- [x] Escalada I5:boot_log observe-only + mesh_frag_pressure no caller (fecha cadeia panic wasmi)
- [x] Rodada 10: 14,8min runtime, UI viva, 1 storm contido, OOM honesto final (logs 193855/195127/200959)

## âœ… s429 â€” VMD visÃ£o guest: traduÃ§Ã£o MMIO de offsets SHDW nÃ£o-nativos

- [x] vmd.c provado antes de implementar: offset aplica-se a RECURSOS (bus = cpu âˆ’ offset), NUNCA a DMA de RAM (upstream identidade no guest)
- [x] TraduÃ§Ã£o DOWNSTREAM MMIO: translate_bar puro (cpu = bus + offset dentro da janela), child_bar_cpu (guest = MEMBAR UC; nativo = phys UC)
- [x] init() sem abort em offsetâ‰ 0: MEMBAR size validada (0 = abort honesto), janelas registradas, slog GUEST
- [x] NvmeDriver::probe_at_mmio_va (VA prÃ©-resolvido pelo VMD guest); DMA de RAM do NVMe inalterado
- [x] Testes: traduÃ§Ã£o+bordas+None fora; nativo identidade; detecÃ§Ã£o guest off1\|\|off2 (k-nano 240); 0 erros
- [ ] **HW lab:** `nvme ok=true via=vmd guest` no BOOT.LOG do notebook (SHDWâ‰ 0 + MEMBARs do BIOS)

## âœ… s428 â€” a2_proof 120s + watchdog por slice

- [x] Deadline 300â†’120s (15Ã— o pior caso mensurado 8s â€” pega wedge global)
- [x] Watchdog por slice: A2_SLICE_T0_US no latch; check no topo do poll_slice (fora do latch); >10s = wedge â†’ terminal imediato
- [x] Stall real (nÃ£o voltou) â‰  slice lento (voltou, A2_SLOW_SLICES) â€” caminhos distintos
- [x] Testes: constantes+ordem, wedge simulado terminal imediato, slice lento continua (cortex 110); 0 erros

## âœ… s427 â€” Loader-VRAM FATâ†’BAR sem heap (fase 1)

- [x] `read_root_file_dev_chunked` (k_nano): FAT32 por callback, zero Vec do blob
- [x] `loader_vram` (k_hal): probe header â†’ layout â†’ janelas â†’ stream BAR â†’ registro SEQ_MATS (lane ativo prÃ©-load)
- [x] `on_model_loaded` no-op quando `loader_resident()` (sem upload duplo)
- [x] Gates: 0 erros; k-hal 74, k-nano 237
- [ ] **Residual:** stub dos packed no `load_llm_v6` quando `loader_resident()` â€” aÃ­ o heap NUNCA segura os pesos (liberaÃ§Ã£o real)
- [ ] **HW lab:** `LOADER-VRAM streaming` no BOOT.LOG + lane ativo antes do load

## âœ… s426 â€” Card HUD Hints H3 (validaÃ§Ã£o ao vivo)

- [x] `jarbas::cards::hints_card` (ID 8003): stage/fwd/n/vram resid direto dos statics, affordance do critÃ©rio de aceite (fwd<100Âµs)
- [x] F11 toggle + `close_card_by_id` no compositor + refresh 2 Hz idempotente por id + help
- [x] Gates: 0 erros; jarbas 123/123 (damage_tests host skipado, prÃ©-existente via stash), k-hal 71
- [ ] **HW lab:** F11 no notebook â†’ `fwd` verde (<100Âµs) e `hints n` crescendo ~2/s = H3 fechado

## âœ… s425 â€” Linha `bootlog` no HUB HEALTH (falha de flush visÃ­vel)

- [x] `boot_logger::hub_log_line()` â€” ok n<N> / fail backend+razÃ£o+streak / fail sem-backend / pre-fat / n/a (statics LAST_FAIL_KIND/LAST_TRY_BACKEND nos paths de falha)
- [x] Linha `bootlog` no HUB HEALTH (row 19, HUB_ROWS 20) + SysInfoAgent com a mesma string no slog
- [x] Gates: 0 erros; k-nano 237, jarbas hub 7/7, hermes 257 (t=1)
- [ ] **HW lab:** ver `bootlog fail sem-backend` (ou ok) no painel no notebook

## âœ… s424 â€” BOOT.LOG de fallback na ESP (evidÃªncia de boot)

- [x] Kernel: `overwrite_boot_log` nÃ£o aborta em IoFail de UMA partiÃ§Ã£o â€” continua p/ ESP (contrato `OverwriteResult` intacto)
- [x] Build: `mk_esp_fat.py` embute BOOT.LOG raiz prÃ©-alocado 256KB na ESP (mesmo mecanismo, zero FS novo)
- [x] Fix latente: write do dirent calcula o setor real do `entry` (antes `sector_idx*bps` como byte offset)
- [x] ValidaÃ§Ã£o: ESP de teste 128MB â€” dirent BOOT.LOG 256KB na raiz, chain legÃ­vel, BPB do parser OK; k-nano 237; check 0 erros
- [ ] **HW lab:** flush na ESP visÃ­vel no D: montÃ¡vel no Windows quando o volume de dados falhar

## âœ… s423 â€” Power-on dGPU D3â†’D0 (H3 sobrevive ao D-state)

- [x] `k_hal/src/gpu/gpu_power.rs`: `wake_to_d0` (prova de vida antes do write PMCSR, budget TSC 10ms, re-scan persistido) + `wake_all` â€” 3 testes
- [x] Wire: `wake_all` no boot pÃ³s `detect_all()`; backstop idempotente em `init_vram_tier` + `nvidia::probe`
- [x] Gates: check release 0 erros; k-hal 71 (68+3); CHANGELOG/STATE/SESSION/AGENTS
- [ ] **HW lab:** boot no notebook â†’ `GPUPWR woke` + HINT stage Resident (`hints on vram=3KB fwd<100Âµs`) + lane VRAM `vram_served=true`

## âœ… s422 â€” Intel VMD binder (NVMe notebooks Alder Lake+)

- [x] Driver VMD R0 `k_nano/src/vmd.rs` (CFGBAR=ECAM, busn_start VMCAP/VMCONFIG, scan flat, enable config MMIO, SHDW nativo â‡’ DMA direto, fail-closed honesto) â€” 5 testes
- [x] Refactor NVMe mÃ­nimo: `NvmeDriver::probe_at_mmio(bar0)`; probe nativo delega
- [x] Fallback no `probe_storage_drivers` â†’ vmd::init + probe_vmd_nvme â†’ global `NVME_DRIVER` (bin intocado)
- [x] Gates: check release 0 erros; k-nano 237 (232+5); TECNOLOGIAS 6.9 + CHANGELOG + STATE + SESSION_420 addendum
- [ ] **HW lab:** boot no notebook â†’ `nvme ok=true via=vmd` + BOOT.LOG persistindo no NVMe + card sem `8086:a77f sem driver`

---

## âœ… MARCO S360 â€” UI + mesh + compute distribuÃ­do

- [x] Desktop Jarbas funcional (orb + Hub Health + compositor) no WHPX (lab 6-node S360/ADR-0081/SESSION_360)
- [x] Rede mesh 6 QEMU (3G/3c + 2G/2c + 4Ã—1G/1c) + hub L2 (`tools/qemu_l2_hub.py`)
- [x] ComputaÃ§Ã£o distribuÃ­da FRAG matmul Masterâ†”peers (Memory/Compute/Worker)
- [x] Fail/warn honesty (TLSPINS/Trust/CapGate/mouse soft-F4)
- [ ] Peer B estÃ¡vel em toda a topologia 6-node (residual S360)
- [ ] Re-boot mesh pÃ³s-fix s360 (AWAITING operador) â€” S455 (SESSION_455, 2 instÃ¢ncias 6c/6GB) Ã© re-teste separate, nÃ£o o lab

---

## ðŸŽ¯ OBJETIVOS

1. **Aceite metal pÃ³s-s328** â€” flash `usb_hw.img` e validar InferQueue/UI/BOOT.LOG no Alienware.
2. **Gate v2.0.0** â€” fechar ADR-0100 Ondas 0â€“3 + review formal + OK maintainer.
3. **Emagrecer `k_nano`** â€” primitivos R0 + hooks (ADR-0103; FASE Aâ€“H feita, S1 metal pendente).
4. **Sandbox Ring3 CPL=3** â€” um sandbox para blob nativo B/C; wasmi default atÃ© T-055 (ADR-0102).
5. **Kernel ternÃ¡rio-nativo** â€” Falcon3-3B 1.58-bit com ADD/SUB/SKIP packed no metal (ADR-0101).
6. **Observabilidade de boot** â€” 3 canais + `BOOT SCORE` + instrumentos FB (ADR-0092).
7. **Desktop Jarbas v2** â€” production-grade (ADR-0090); **s360 = desktop vivo + mesh no orb** âœ… parcial.
8. **SMP per-CPU runqueue** â€” feature ON; falta aceite metal (ADR-0089).
9. **Mesh cluster** â€” escalar 6-node lab (S360) â†’ HW LAN real (ADR-0081).

---

## ðŸ”¥ ABERTOS NO METAL (s316â€“s328)

| Item | EvidÃªncia | Estado |
|------|-----------|--------|
| Freeze determinÃ­stico @ tick 1370 `network_agent` | SESSION_316 / STATE s327 | ABERTO â€” discriminador heartbeat (s326) aguarda boot no metal |
| xHCI MSC `CCS=0` pÃ³s-HCRST (HCRST deixa PP=0) | s327 (fix PP=1 RMW) | ðŸŸ¡ wired; validar metal |
| UI/splash first-frame com MSC sem teto | s315/s296 | ðŸŸ¡ wired; validar metal |
| BOOT.LOG `E:\BOOT.LOG` real | 0103 S1 | â–¶ï¸ AWAITING_OPERATOR |

## âš ï¸ SAÃšDE DE TESTES / CI

- [x] **s367** â€” `cargo test -p cortex --lib llm_response_gate` **7/7** + `infer_queue::tests` **5/5** (`--test-threads=1`); lab `tools/lab-llm-response.ps1` (QEMU decode AWAITING)
- [x] **s368** â€” workspace host **0 fail** (`--exclude neural-kernel,boot --no-fail-fast`); fixture GGUF; soft_stride/MHI/boot_observe locks; `apply_one_layer` poison on refuse
- [x] Fixture do teste `cortex`: `python tools/gen_test_gguf.py` â†’ `target/test_tq2_0.gguf`
- [ ] CI job nightly = mesmo comando workspace (wire)

## ðŸ“Œ PÃ“S-s328 (ordem do STATE)

`aceite metal â†’ Prefill AirLLM layer-yield â†’ mÃ©tricas/budget TSC â†’ Layer S (GPU/NPU WS-D/E)`

- [ ] Aceite metal: orb+mouse+mic vivos durante o generate; 1Âª frase TTS antes de `LLM_RESPONSE`
- [ ] Prefill AirLLM: layer-yield (evitar hiccup no prompt)
- [ ] MÃ©tricas/budget por TSC
- [x] **ADR-0105 B0â€“B3** cÃ³digo (FAT/aliases, CpuOnly, CPU W2A8+gaps, telemetria). **Aberto:** B1.2/B2.4/B4 device golden AWAITING_HW.

---

## ITENS (priorizados: mais nova â†’ mais antiga)

### 1. ADR-0103 â€” k_nano microkernel modular
**Goal:** `k_nano` = primitivos R0 + hooks; polÃ­tica em `k_hal`/`k_ai`/`cortex`/`hermes`/`jarbas`. Sem virar process-OS.

- [x] **FASE A** â€” 13 mÃ³dulos mortos deletados (~1800 LOC)
- [x] **FASE B** â€” NIC drivers â†’ `k_hal::net` (facades e1000/rtl8139/i225/virtio_net)
- [x] **FASE C** â€” FS primitivos â†’ `k_hal::fat_assets` canÃ´nico (+ `legacy read_mbr` re-exports)
- [x] **FASE D** â€” sgdb canÃ´nico em `k_ai`; telemetry mantida R0 (8 callers hermes)
- [~] **FASE G+H** â€” analisadas, **nÃ£o migradas** (dependÃªncias + risco)
- [ ] **S1** â€” `k_hal::usb` hubâ†’MSC + early `probe_and_install` â†’ `E:\BOOT.LOG` real (â–¶ï¸ AWAITING_OPERATOR; ver "Abertos no metal")
- [ ] **S3** â€” Leitores FS nÃ£o-boot Ã³rfÃ£os (ntfs/btrfs/ext2 read-only) â†’ crate ou delete
- [ ] **S4** â€” Storage cognitivo (tickv FE, rollback UI) â†’ k_ai/hermes
- [ ] **S5** â€” Podar exports mortos + `check_duplication.py` limpo
- [ ] **S6** â€” (Opcional) esqueleto `arch/memory/scheduler/` sem mover lÃ³gica
- [ ] **Fase 2** â€” Schemes CPL=3 (ðŸ”´ gated: ADR-0102 aceite HW)

### 2. ADR-0102 â€” Ring3 sandbox CPL=3
**Goal:** um sandbox CPL=3 para blob B/C; `isolation_ring_available()==false` â‡’ sÃ³ wasmi.

- [x] **H1** â€” Feature `ring3` propaga no bin (`neural-kernel` `default = ["ring3","cap-demos","smp-runqueue"]`)
- [x] **H2** â€” Demos P6 reais no tree (`demo_ring3*` em `k_nano::paging`; SESSION_302)
- [x] **H3** â€” `ring3_can_iretq()` + `can_register_native()` cindidos + self-test wired no boot
- [~] Demos P6 rodam no QEMU 4c (`ring3_can_iretq=true` + Jarbas greeting; SESSION_305); faltas contidas non-fatal
- [x] **T-051** â€” Separar `#GP` OVMF de `#GP` kernel (`gp_fault_class` / `gp_likely_firmware`) â€” cÃ³digo âœ…; WHPX flaky fora do caminho crÃ­tico
- [ ] **T-052** â€” Metal: iretq+CPL3 + fault-containment â–¶ï¸ **AWAITING_HW** (depende Onda 2 SMP metal)
- [ ] **T-053** â€” Checklist 0077 Â§6 em HW â–¶ï¸ **AWAITING_HW** (HITL `/ring3 approve` marca gate; aceite = notebook)
- [x] **T-054** â€” `stash_native_runner` + HITL Escalate `ring3_register` â†’ `promote_native_ring_if_ready` (SESSION_347)
- [ ] **T-055** â€” `isolation_ring_available()==true` sÃ³ apÃ³s T-053 metal (cÃ³digo gated; aceite HW pendente)
- [x] **T-056** â€” Fronteira xmm: `verify_blob_no_simd` antes do `iretq`
- [x] **T-057** â€” CapGate deny DMA/PIN/FB a CPL=3 (`sandbox_syscalls`)

**Operador:** `/ring3 status` | `/ring3 approve` â†’ `/approve <id>`. QEMU nunca auto-registra (hvâ‰ None).
### 3. ADR-0101 â€” Falcon3-3B Cognitive Lab
**Goal:** decode m=1 do 3B faz ADD/SUB/SKIP packed no SIMD do `x86_64-unknown-none`.

- [x] **Onda 0** â€” InventÃ¡rio 3B-first (`falcon3_boot_names`, `fat_names_for(Active)` com `FALCON3.V6`)
- [x] **Onda 0** â€” SSE2 ADD/SUB/SKIP real + scalar ternÃ¡rio-nativo (`bitnet_sse.rs`, paridade vs scalar; SESSION_348)
- [x] **Onda 0** â€” GGUF inferÃªncia wired (TQ2_0 + BF16 + auto-config; SESSION_309)
- [~] **Onda 0** â€” AVX2 no target `none` (defer: metal usa SSE; host AVX2 ainda FMA dequant)
- [x] **Onda 1** â€” Shortlist logits + Medusa/n-gram wired + KV H2O telemetria/InferQueue (SESSION_348)
- [x] **Onda 2** â€” Difficulty gate no generate/InferQueue (policy budget; **nÃ£o** KL early-exit)
- [x] **Onda 3** â€” `cognitive_runtime` composiÃ§Ã£o + `/cog` status (SGDB continua prompt-side)

### 4. ADR-0100 â€” Backlog unificado KÂ³CHJ (plano-mestre)
**Goal:** gate v2.0.0 = Onda 0 + Onda 1 (mÃ­n. T-011) + Onda 2 (um metal) + Onda 3 (A2 ou A3).

- [x] **Onda 0** â€” Honesty `BOOT_AI` + freeze HardwareInfo (T-001â€“T-006)
- [x] **Onda 1** â€” `measure_bandwidth` + `/hw/storage|gpu|net` (T-007â€“T-016)
- [ ] **Onda 2** â€” Metal K23 `online==madt-1` (T-017 img âœ…; T-018â€“T-021 â–¶ï¸ metal)
- [ ] **Onda 3** â€” 0086 A2â€“A8 (T-022â€“T-032; A1/A9 HITL)
- [ ] **Onda 4** â€” `ap_pollable` + runqueue 0089 (T-033â€“T-044) ðŸŸ¡ (feature 0089 jÃ¡ ON)
- [~] **Onda 6** â€” Ring3: cÃ³digo-complete (SESSION_347 HITL `/ring3`); metal T-052/053 â–¶ï¸ AWAITING_HW
- [ ] **Onda 7** â€” W2A8 gated + 0078 sÃ³ Fase 1 (T-058â€“T-065)
- [ ] **Onda 8** â€” Golden GPU/SDMA/NPU (T-066â€“T-069) â–¶ï¸
- [ ] **Onda 9** â€” 0058 S5 um widget + A/V (T-070â€“T-072)
- [ ] **Onda 10** â€” AirLLM DMA/e2e (T-073â€“T-075) â–¶ï¸

### 5. ADR-0094 â€” Hermes Cleanup âœ…
**Goal:** dead code excluÃ­do + testes host nos mÃ³dulos ativos.

- [x] Comentar 35 mÃ³dulos mortos (7.446 LOC, -23% build)
- [x] 22 testes `cognitive_bridge` + 11 testes `memory_store`
- [ ] Residual: deletar arquivos mortos do disco (manual)
- [ ] Residual: 2 testes `wasm_build` + 1 `cognitive_bridge` falhando (ver "SaÃºde de testes")

### 6. ADR-0093 â€” Jarbas Optimization âœ…
**Goal:** lock-free render + caps + dirty-rect + PT-BR TTS.

- [x] 45 host tests (`jarvis.rs`) â€” 3 `soul_*` falhando (ver "SaÃºde de testes")
- [x] Mouse/theme lock-free (Atomic)
- [x] Memory caps (NotificationQueue 32, ChatWindow 100/64)
- [x] HUD string cache + dirty-rect gating
- [x] 8 fonemas PT-BR no formant
- [x] Orb JARVIS hex-grid + anti-flicker (SESSION_291); UI liveness/anti-black-screen (SESSION_315)

### 7. ADR-0092 â€” Boot observability
**Goal:** 3 canais (dmesg/produto/placar) + `BOOT SCORE` + instrumentos FB.

- [x] **O0** â€” Contrato `sev` ok|warn|fail|trace + filtro consola
- [x] **O1** â€” Banner `=== PHASE n= ===` + fase 8 PostRuntime
- [x] **O2** â€” Mudos BPB/INIT1/e1000/SIPI/scan/PnP
- [x] **O3** â€” `BOOT SCORE` + `tools/parse_boot_score.py`
- [x] **O4** â€” Sem K* no ecrÃ£; HUD produto
- [x] **O5** â€” Profile qemu vs hw no placar
- [x] **Instrumentos FB** â€” `diag_mark`/`diag_stamp_agent`/`diag_stamp_exception`/`tick_stage`/heartbeat `T=` (s318â€“s327)
- [ ] **Metal** â€” Canal A em `E:\BOOT.LOG` (ðŸŸ¡ = ADR-0103 S1)

### 8. ADR-0091 â€” MigraÃ§Ã£o neural-sgdb âœ…
**Goal:** neural-sgdb externo como substrato de memÃ³ria cognitiva.

- [x] Fase 0 â€” DependÃªncia no_std
- [x] Fase 1 â€” TickvStorageAdapter
- [x] Fase 2 â€” NSGDB bridge + fallback
- [x] Fase 2.5 â€” Hits tipados, embedder seam, lexical default, lifecycle, scoping, cognitive ops
- [x] Fase 3 â€” Cortex/Hermes memory-aware
- [x] Self-Heal closed-loop via NSGDB (SESSION_316)
- [ ] Residual: migrar 75 callers gradualmente

### 9. ADR-0090 â€” Jarbas Desktop v2.0
**Goal:** desktop production-grade (4 Tiers / 15 features).

- [x] **Tier 1** â€” Glyph cache + grid pre-render (LUT seno âœ… e dock âœ… jÃ¡ no tree; `cargo check -p jarbas` 0 erros, 57 testes)
- [x] **Soul/Emotion** â€” unificaÃ§Ã£o `hermes` (Emotion, Soul, SERâ†’Affect, LoopPhase) delegando em `jarbas` (SESSION_319)
- [ ] **Tier 2** â€” Window animations, chat scrollback, hover states, voice waveform (~12d)
- [ ] **Tier 3** â€” Per-window back buffers, desktop real (~20d)
- [ ] **Tier 4** â€” Transformacional (~30d)

### 10. ADR-0089 â€” Per-CPU Run-Queues SMP
**Goal:** distribuiÃ§Ã£o cooperativa de agents entre cores (`smp-runqueue`).

- [x] Run-queue slot-based + steal min-1 + telemetria (cÃ³digo)
- [x] Feature `smp-runqueue` no `default` do bin + `MAX_CORES=256` (s307)
- [ ] Aceite metal K23: `online==madt-1` + hybrid P/E

### 11. ADR-0045 â€” Pipeline de voz (auditoria s352)
**Goal:** o caminho wakeâ†’micâ†’STTâ†’LLMâ†’TTSâ†’alto-falante com fidelidade, duplexidade explÃ­cita e latÃªncia medida.
Plano completo + evidÃªncia: `docs/implementation/2026-09-16-voice-pipeline-audit-s352.md` (IDEA_BANK #558â€“#566).

- [ ] **V1** Pacing do playback pelo clock (`PLAY_SAMPLES_DROPPED == 0` em `/tick 30/60/120`) + resample fora do laÃ§o de escrita â€” #558, #1.2
  - [x] **s368** cÃ³digo: `compute_mixer_want` free+TSC + host tests; aceite QEMU drops=0 residual
- [ ] **V2** Duplexidade explÃ­cita (fim do auto-barge-in) + `STT_UNCERTAIN` falado â€” #559, #560
- [ ] **V3** Teto de utterance + CTC incremental + ring SPSC para `AUDIO_FRAME` â€” #561, #562
- [ ] **V4** Wake-word: janela deslizante, log-mel + `tools/train_wakeword.py`, FPPH em holdout disjunto â€” #563
- [ ] **V5** `VOICE_LATENCY` + orÃ§amento de contexto/SGDB episÃ³dico + identidade `voice_input`/HITL â€” #564, #565, #566
- [ ] Menores: doc drift de `audio/mod.rs` (`AudioPipelineAgent`/`MIC_CAPTURE_RING` nÃ£o existem), `wake_window.min(120)`, saÃ­da dupla HDA+UAC no mixer

---

### 12. ADR-0106 â€” DecisÃµes calibradas (D0â€“D4 + migraÃ§Ã£o M0â€“M4)
**Goal:** juÃ­zo onde o significado importa deixa de ser cadeia de `contains()`; toda decisÃ£o carrega distribuiÃ§Ã£o, confianÃ§a e abstenÃ§Ã£o explÃ­cita; Î¸ vem da reliableza medida.
ADR + evidÃªncia: `docs/architecture/0106-decisoes-calibradas-confianca-abstencao-hitl.md` (IDEA #588). Triagem: `docs/architecture/0106-m0-triagem-sitios.md`.

- [x] **D0** Contrato `Decision<T>` (`Confidence` Q8, `margin`, `AbstainReason`, `DecisionSource`, `Q8Dist`) + `note_outcome` + acumulador de reliableza â€” `TODO-0106-1`
- [x] **D0** PersistÃªncia `ai/decisions/reliability` (best-effort Tickv) + contadores + `slog` ok/warn + `/decisions` â€” `TODO-0106-2` (HUD postura = residual leve)
- [x] **D1** `Intent::Unknown` + `decide()` canÃ´nico (score por braÃ§o) + `think()` wrapper; Hermes publica abstenÃ§Ã£oâ†’LLM â€” `TODO-0106-3`
- [x] **D2** PolÃ­tica de Î¸ por classe de sÃ­tio; `ApprovalGate::classify` â†’ `site_policy` (Noul); `Deny` imune â€” `TODO-0106-4`
- [x] **D3** `Noul` destrutividade + `Score`/`decide_tier` ComputeTier + escape hatch Unknown â€” `TODO-0106-5`
- [x] **D4** Labels HITL + weak_auto â†’ JSONL `example`; `train_router` merge (HITLÃ—3); `/decisions correct` â€” `TODO-0106-6` (r3 replay contÃ­nuo = residual opcional)
- [x] **M0** Triagem canÃ´nica â€” `TODO-0106-7`
- [x] **M1** emotion / skill-creation â€” `TODO-0106-8`
- [x] **M2** plugin + marketplace â€” `TODO-0106-9`
- [x] **M3** Hub row `decide` + pill via `hub_posture_sev` â€” `TODO-0106-10`
- [x] **M4** Lista OUT formal â€” `docs/architecture/0106-m4-excluded-sites.md` â€” `TODO-0106-11`

---

## ðŸ”— DEPENDÃŠNCIAS (ordem obrigatÃ³ria)

```
0100 Onda 2 (SMP metal) â”€â”€â–º 0102 T-052 (iretq metal) â”€â”€â–º 0103 Fase 2 (schemes)
        â”‚
        â””â”€â”€â–º 0092/0089 aceite metal (0089 feature jÃ¡ ON)
```

## ðŸ”´ BLOQUEIOS (nÃ£o sÃ£o trabalho de cÃ³digo)

| Item | ADR | O que falta |
|------|-----|-------------|
| S1 BOOT.LOG metal | 0103 | Operador: pendrive + `E:\BOOT.LOG` no Alienware |
| Flash `usb_hw.img` pÃ³s-s328 | â€” | Operador: Rufus (DD) + boot no Alienware |
| Onda 2 SMP metal | 0100 | Dois notebooks (i5 7Âª + Core 7 240H), `online==madt-1` |
| T-052 iretq metal | 0102 | Depende de Onda 2 SMP metal |
| Fase 2 schemes | 0103 | Depende de 0102 aceite HW |

**DiagnÃ³stico instrumentado (s316):** watchdog de tick lento (`[Sched] warn`), TSC visÃ­vel (`ok`/`warn`), `cmd TIMEOUT ms=` â€” prÃ³ximo boot no metal disambigua H1â€“H6. Checklist: `docs/memory/HW_DIAG_s316.md`.

## âœ… CONCLUÃDO PÃ“S-09-05 (rastreabilidade)

- s306 Dual QEMU 4c mesh Master/Worker + slog P2P visÃ­vel
- s309 Falcon3 GGUF inferÃªncia wired (TQ2_0 + BF16 + auto-config)
- s315 Jarbas UI liveness + anti-black-screen (Alienware)
- s316 Self-Heal closed-loop + NSGDB + detectores de seguranÃ§a; escada de instrumentos FB (s318â†’s327)
- s317 k_ai/cortex: ReAct + H2O + CodebookVQ
- s318 hermes/cortex FASE 1â€“4 (KvCache, MoE 0.05, dead code)
- s319 hermes/jarbas unificaÃ§Ã£o (Emotion, Soul, SERâ†’Affect, LoopPhase)
- s321 k_nano slimming FASE Aâ€“D + facades
- s327 freeze bisector + MSC Port Power
- s328 Full Infer D+B+C (InferQueue WS-H; ADR-0057)

## ðŸ§¾ DÃVIDA DE DOC/CONSISTÃŠNCIA

- [x] Licenca alinhada em s450: `TECNOLOGIAS.md` MIT->**AGPL-3.0** (header + tabela); `LICENSE`=AGPL-3.0 confirmado.
- [x] MÃ©tricas alinhadas ao **medido** em `AGENTS.md`, `SUMMARY.md`, `ROADMAP.md`, `codemap.md`, `HOWTO.md` (+ `TECNOLOGIAS.md`/`README.md`): ~169K LOC / ~672 `.rs` (12 crates do workspace) / 41 nativos / v1.9.99-s412 / 829 testes host

---

**Detalhes completos:** `docs/architecture/0100-k3chj-backlog-custo-anel.md` (T-001â€“T-075) Â· `AGENTS.md` Â· `docs/architecture/INDEX.md`

---

## ðŸ”´ s446 â€” F1.5 fechado como PASS, persistencia bloqueada; lock do forum

- [x] `run-f15.ps1`: OVMF code+vars + `-cpu` + exit code **funcional** (o check
      anterior era quebrado: `.ExitCode` null sem handle cacheado)
- [x] `f15_parse.ps1` com escopo por nome da skill + suite `run_f15_fixtures.py` 19/19
- [x] Boot 1 **PASS** (exit 0) / boot 2 **FALSIFIED** (exit 1, `reason=not_found`) - SUPERSEDED s451: F1.5 PASS (boot1 act=gen hash=0x458653425da3b4a5 -> power cycle -> boot2 act=reuse MESMO hash, result=82).
- [x] Lock de escrita **compartilhado** do forum (py + PS), 0 id dup / 0 rasgada
- [x] `forum_repair_ids.py` (mode `"w"`) trancado no lock
- [x] Backend persistente no QEMU - RESOLVIDO s451: `[TICKV] backend=file dev=virtio` + F1.5 PASSOU (2 boots + ablacao).
      nenhum disco escrito: o eixo de persistÃªncia nÃ£o fecha sem isto
- [ ] `gen_lsk1.py` com 512 bytes zerados (`BLOB_LEN=512` do `skill_lab` vs 256 do gerador)
- [ ] Warn Ãºnico se 4 bytes != 0 e != `LSK1` (nÃ£o setar `CONSUMED`)
- [ ] â³ Kani/E1: precisa de host Linux ou `wsl --install` (decisÃ£o do dono)
- [ ] Promover `target/gen_f15_fixtures.py` e `target/test_loop_locked.ps1` para
      `tools/` (hoje sÃ£o gitignored e somem em `cargo clean`)

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

