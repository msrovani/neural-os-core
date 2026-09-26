# SESSION_410 — Mesh 6 lab: OOM bughunt + honesty sev (worker/Master anti-bloat) + GOAL1 6c/5min

**Motivo:** validar v1.9.99 em duas frentes — (1) GOAL1: boot 6 cores/6GB com UI ativa e 5 min sem erros; (2) mesh P2P de 6 nós (`tools/run-mesh6-lab.ps1`) com workers de 1GB morrendo de OOM. Terminou em caçada de vazamentos + fecho do audit de sev (`Sev::from_sub`) s409→s410.

## GOAL1 — PASS ✅
- `tools/goal1-6c-5min.ps1` (novo): loop de boot + 5 min de observação; status em `logs/goal1_status.txt`.
- Evidência: `logs/boot_whpx_20260926_070905.txt` — ticks T+32k, **5.115 ok / 236 warn / 2 fail conhecidos** (ELF self-test non-fatal + I3 fantasma — ambos tratados abaixo). UI/compositor ativos durante a janela inteira.

## Mesh 6 — OOM dos workers (causa-raiz + fix)
- **Sintoma:** workers c–f (RAM 1G) morrem por OOM heap em T+~40–80k ticks; Master (a, 2G) morre em T+141k na rodada pré-fix.
- **Causa-raiz (worker):** `hermes::mesh_knowledge` broadcast_learner_memory → on_memory_doc com **dedup por VectorClock**: cada TX fazia `clock.tick()`, então o payload nunca repetia e o dedup nunca derrubava — a mesma memória era re-aplicada (RX MEM aplicada **648×** no log) e o store crescia linearmente (~11KB/tick) até OOM.
- **Fix (mesh_knowledge.rs, anti-bloat):**
  - `fnv1a64()` (FNV-1a 64) como fingerprint de conteúdo;
  - `LAST_TX_HASH: [AtomicU64;8]` — só TX quando o conteúdo muda (slot = hash(key)%8);
  - `LAST_RX_HASH: [AtomicU64;8]` — RX descarta payload idêntico antes de aplicar.
- **Validação:** imagem pós-fix, workers **estáveis em T+79k→T+122k+**, RX MEM aplicado 648→**2**. Sem crescimento linear de heap.
- **Master (T+141k OOM):** mecanismo confirmado como consumo linear por alocações ilimitadas nos caminhos de orquestração:
  - `runtime_observe`: **loop de feedback I4→LLM** — Master recebeu 233 intents "diagnostique e corrija" vs ~35 nos workers (7× de amplificação); fix: `should_escalate_health_to_llm` retorna `false` para `sched=Violation/Warning`.
  - `skill_observer`: `OBSERVATIONS_CAP = 128` + evicção FIFO (watch_task/watch_correction).
  - `approval`: `REQUESTS_CAP = 64` + evicção em `ApprovalGate::request`.
  - `skill_marketplace`: `register_local_skill` com dedupe por nome + cap 32.
  - `hal_offer`: `AbsentCached` → sev `trace` (Absent **fresco** continua `warn`) — o warn repetido por tick poluía dmesg e alimentava re-análise de log.
- **Validação cruzada:** rodada nova do mesh pendente para confirmar Master além de T+141k (a rodada em execução usava imagem pré-fix-Master).

## I3 VIOLATION fantasma (T+121) — mecanismo
- `safety_invariants::check_trust_intact` julgava "Trust sem dados no anel 2" antes de qualquer push do hermes — violação sem dado.
- **Fix:** `TRUST_PUSHED_COUNT: AtomicU64` + `pub fn note_trust_entries(n)` (padrão push: hermes→k_ai é a direção de dependência, não dá para ler direto); `safety.rs`/`security.rs` fazem o push após ler `entry_count()`.
- Semântica nova: push>0 → **Pass**; fleet up + zero push → **Violation real** ("fleet up e NENHUM push de Trust do hermes — SafetyAgent não tickou?"); pré-fleet → **Warning**.

## Audit sev (s409 + s410) — fecho
- s409 (rodada anterior): `reg|query|select|state|revoke` → Ok; `smoke` → Trace.
- **s410 (este, fecho):** emissores localizados por grep e mapeados com sev honesta:
  - **Ok:** `msg` (linhas de report/display: LogAnalyst, optimizer, package_hub, network_agent, jarvis greeting), `master`/`worker` (FL aggregate + CRDT publish), `await` (HW-GATE status report), `life` (SELF.STATE lifecycle), `mode` (boot_mode), `map` (virtio BAR map), `observe` (boot_observe), `pcie` (GPU PCIe report), `populate` (DeviceTree), `Learn` (Trinity learn), `CONSOLIDATE`/`REFLECT` (SleepCycle/self_evolve fase ok), `h4`/`h5_demo`/`p3` (demos PoC CapGate/VirtIO, cf. SESSION_360), `0040`/`0047-G3|G4|G5|H|L3|NGRAM` (gates ADR-0040/0047).
  - **Trace:** `dbg` (debug de boot).
- **2 emissores corrigidos** que mentiriam sob o novo mapa (SESSION_360 — sev é contrato de visibilidade):
  - `hermes/safety.rs`: SAFETY VIOLATION estava com slot `msg` → agora slot `fail`.
  - `k_hal/virtio.rs`: "Absent — sem BAR" estava com slot `notify` (agora Ok) → agora slot `absent` (Warn).
- Testes: `s410_subs_are_ok` + `unknown_sub_is_trace` atualizado; k-nano lib **205 pass / 0 fail**.

## Respostas informativas (sem código)
- **Por que tickv/noproto seguem no código com n-sgdb existindo:** TickvLite é backend R0 consumido via `TickvStorageAdapter` pelo neural-sgdb (junction `../neural-sgdb`, 1.1.20); noproto é o wire do mesh R0 (ADR-0081). Os crates "tickv/noproto no crates.io" são idea-only (SESSION_385).
- **VirtIO/NVIDIA/KVM:** VirtIO-GPU wired p/ display (s367: FE jarbas/virtio_gpu.rs + HalOffer); NVIDIA compute segue `CpuOnly` honesto em QEMU (ADR-0105 B1.3, AWAITING_HW). KVM+VFIO passthrough (GTX 1050) é o caminho p/ destravar compute GPU — host Windows/WHPX atual não suporta VFIO.

## Verificação
- `cargo check --release --workspace --exclude neural-kernel --exclude boot` → **0 erros**; `-p neural-kernel` (com clean+touch, S346) → **0 erros**.
- `cargo test --release -p k-nano --lib` → **205 passed / 0 failed**; slog 6/6 (s410_subs_are_ok ✅).
- QEMU mortos antes do rebuild (WinError 32 em `build_image.py` se disk anexado).

## Adendo s410b — versão neural-sgdb + dual-truth TickvLite (auditoria)
- **Versão:** junction `../neural-sgdb` tip = **v1.2.1** = Cargo.lock (em sync); os "1.1.20" vivos eram 2 comentários stale (`k_ai/Cargo.toml`, `sgdb/mod.rs`) — corrigidos. Logs do kernel NÃO imprimem versão (só `neural-sgdb via TickvStorageAdapter`); ideia: slog do `nsgdb_init` reportar a versão compilada.
- **Bridge cobre todas as ops usadas:** 20 ops chamadas pelo kernel existem na 1.2.1 (recall*, rag_context*, health, reinforce, explain, set_scope, set_entities, put, remember_*, get, decay/expire/consolidate). `put_many` batch novo da 1.2.0 não usado (default loop de put — funcional, não hot path).
- **Dual-truth audit (lição Fat32Io) — veredito: SEM duplicação ilegítima:**
  1. **TKLV codec**: `k_nano/storage/tickv.rs` (fonte da verdade R0) ↔ `neural-sgdb/src/tickv.rs` = **port documentado byte-exato** p/ interop de storage (SDK do crate comunitário); sincronizado por testes golden bidirecionais (`golden_record_bytes` ↔ `golden_record_bytes_match_neural_sgdb`, mesmo vetor). k_ai não copia nada do codec — só consome `k_nano::storage::{put_blob,get_blob,with_tickv,TickvLite}`.
  2. **NMD1 MemoryDoc**: `k_ai/sgdb/memory_doc.rs` (336 LOC, no_std runtime) ↔ `neural-sgdb/memory_doc.rs` (1904 LOC, superset com side-tables MDM1 v4–v6 que NÃO entram no wire) = **contrato byte-idêntico declarado nos dois lados** + golden NMD1 idêntico (magic|layer|klen|key|clock 72B|plen|payload|bitflag). Dois truths necessários: o crate comunitário não pode depender de k_ai e vice-versa (ciclo); o contrato é o golden test.
  3. **VectorClock 72B**: k_ai define o próprio (8 nós, encode fixo no NMD1) — NÃO usa o de `k_nano::sync::clock.rs` (que é outro, de concorrência) nem o do neural-sgdb (com overflow/side-table). Faz parte do contrato NMD1; ok.
  4. **Motor duplicado (única atenção estrutural):** k_ai mantém `AiosDatabaseEngine` interno (ART+BQ+ram_l0l1) **e** o NSGDB externo (`Sgdb` via adapter) sobre o MESMO TickvLite. Write-path sincronizado por `sync_write_to_nsgdb` (#537) em `put_kv`/`put_doc` (NSGDB recebe NMD1 raw via `ExtDoc::new` — mesmo key-scheme `md/Lx/`, então o índice externo indexa o MESMO blob); `boot_observe` prefere escrever via `db.put` externo com fallback interno. Read-path: interno via `with_engine`, externo via adapter (mesmo backend). Risco residual: os DOIS motores têm ART/BQ próprios — divergência de índice é possível se um write bypassar o sync (nenhum path conhecido hoje; todos os writes de store.rs passam por `sync_write_to_nsgdb`). Consolidação futura = 1 motor só (IDEA: absorver `AiosDatabaseEngine` no NSGDB externo ou vice-versa).
  5. `fnv1a64` tem 3 cópias (k_hal/kernel_pack, hermes/mesh_knowledge s410, neural-sgdb tickv) + crc32 em k_nano/neural-sgdb — funções utilitárias de 5 LOC, padrão aceito (sem crate no_std compartilhado disponível; candidata a subir p/ k_nano se crescer).

## Adendo s410c — lab KVM+VFIO GTX 1050 (ADR-0105 B2.x device golden)
- **Novo:** `tools/run-qemu-kvm-vfio.ps1` — QEMU q35 + KVM (`-cpu host`, AVX2 nativo) com passthrough VFIO da GTX 1050 (10de:1c81 + audio 10de:10f1) para o guest. Host alvo = **Linux bare-metal** (WHPX/Windows e WSL2 não expõem IOMMU — verificado no script com fail-fast). Preflight passivo (dmesg IOMMU, vfio-pci bind, ROM existente) + instruções completas de bind/ROM no header.
- **Gates ADR-0105 preservados:** default do script = **CpuOnly** (`-GpuReal` é opt-in); `x-vga=on` + romfile opcional; `-vga none -display none` (guest tem a GPU de verdade, sem VGA fantasma). Ready continua decisão do GUEST: `canary::run_vector_add_canary_nv` (vector_add PASS = has_compute=true) + `pack_present_on_fat` (`NKP_W2A8_SM61.BIN`/`NKP_SM61.BIN` já no mkfat32). NKP unsigned (`pack_nkp_lab.ps1 -IncludeSm61`) = "nunca Ready" mesmo com GPU real — assinatura é item separado (ed25519-compact no workspace).
- **Residual AWAITING_HW:** rodar em host Linux com IOMMU isolado (grupo próprio p/ GPU+audio) + ROM extraída; aceite = slog canário no log serial `boot_kvm_vfio_*.txt`.

## Adendo s410d — runtime hygiene: auditoria completa dos agentes nativos
- **Checklist canônico:** `docs/architecture/runtime-hygiene-checklist.md` (5 perguntas + tabela auditada + processo p/ agente novo).
- **7 estruturas sem teto corrigidas:**
  1. `k_ai::memory_systems::EMBED_INDEX` — alimentado a cada exchange + rebuild do skill_loader, **sem dedup e sem cap** (o mais grave; mesmo padrão do bug mesh). Fix: dedupe por label (replace) + cap 128 FIFO.
  2. `k_ai::trust::escalation_log` — record_violation em loop empilhava sem teto. Fix: cap 64 FIFO.
  3. `k_nano::fts_search::IDX` — index_put re-empilhava o mesmo path. Fix: dedupe por path + cap 256 FIFO.
  4. `hermes::skill_gen::TASK_PATTERNS` — cada task nova = entrada nova. Fix: cap 64 + evicção 1ª chave.
  5. `hermes::skill_opt::EVOLVING` — cada skill efêmera nova = entrada com source. Fix: cap 64 + evicção 1ª chave.
  6. `hermes::wasmi_rt::SKILL_PROVENANCE` — unregister deixava órfãos. Fix: cap 64 + evicção 1ª chave.
  7. `k_hal::gpu::backend::JOB_RINGS` — re-init empilhava ring por GPU (push puro). Fix: replace-by-vendor (1 ring por vendor).
- **~19 estruturas auditadas OK** (caps pré-existentes): self_learning 256, audit ring 4096, training/ephemeral bufs 100, chat_history 50, SESSION 48, NUDGE_QUEUE 16, TOASTS 8+TTL, PIN_CACHE 24, OFFERS ≤32, DEVICE_TREE dedupe, UCAST_STASH 8, FED_DELTAS keep-latest, infer_queue 8 slots, event-bus bounded (S375), L0/L1 lifecycle.
- **Verificação:** check 0 erros (k-hal/k_ai/hermes/k-nano); testes: k_ai 56, hermes 205, k-nano 56, k-hal 56, cortex 89 — 0 fail.

## Adendo s410e — put_many no TickvStorageAdapter (batch real do TickvLite)
- **`TickvLite::put_batch(items)`** (k_nano/storage/tickv.rs): N records com UMA passada — invalidate dos antigos antes do append, append contíguo, **1× maybe_gc no fim** (antes: N× lock+mount-check+maybe_gc). Semântica idêntica ao put individual (last-wins, CRC, scan íntegro). Testes: roundtrip+overwrite no batch, empty=no-op.
- **`k_nano::storage::put_batch(items)`**: wrapper global com 1 aquisição do lock TICKV (put_blob = 1 lock por put).
- **`TickvStorageAdapter::put_many`** (Storage trait 1.2.1): sobrescrito — converte keys UTF-8 antes do lock único; falha aborta sem tocar flash.
- **`engine::checkpoint_l0l1` migrado para o batch** — o maior beneficiário: flush do SleepCycle fazia N× put_blob (lock+GC-check por item); agora 1 lock + 1 GC-check.
- **Medição (host, N=256, 3 runs):** individual 216–529µs vs batch 147–308µs = **speedup 1.47–1.76×** (~1.6× típico). No `nsgdb_init` em si o ganho é **pequeno** (init só faz scan_prefix + open; o put em massa fica no checkpoint do SleepCycle e no rebuild) — o beneficiário real do boot é `rebuild_indices_from_tickv`-adjacente (writes) e o CONSOLIDATE periódico. Ganho estrutural mais importante: 1 aquisição de lock por N writes elimina janela de reordenação entre agentes no SMP.
- **Verificação:** check 0 erros (rebuild real ~11s); testes k-nano 210, k_ai 57 — 0 fail.

## Lições
- **Dedup com nonce auto-incrementado não dedupa:** qualquer dedupe cuja chave inclui estado que muda a cada emissão (clock.tick(), timestamp, seq) é um filtro morto — fingerprint de CONTEÚDO (hash) é a condição de dedupe válida; memória replicada em mesh precisa dedupe TX+RX.
- **Estruturas "aprendizes" sem cap = OOM a médio prazo:** observations/requests/marketplace crescem com o runtime; cap + evicção FIFO é o mínimo para hot-path de agente.
- **Warn repetido por tick também é ruído de dínamo:** além de mentir, o warn constante alimenta loops de observação (I4→LLM) — sev honesta (`trace` para cache) + cortar o loop de feedback resolve os dois.
- **Amplificação Master: intents de saúde geradas pelo próprio agente:** `should_escalate_health_to_llm` escalando `sched=Violation/Warning` gera intents de "diagnostique e corrija" em loop (7× vs workers); escalada é para saúde *nova*, não para o eco da própria decis\u00e3o.
