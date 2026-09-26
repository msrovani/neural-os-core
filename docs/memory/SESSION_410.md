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

## Lições
- **Dedup com nonce auto-incrementado não dedupa:** qualquer dedupe cuja chave inclui estado que muda a cada emissão (clock.tick(), timestamp, seq) é um filtro morto — fingerprint de CONTEÚDO (hash) é a condição de dedupe válida; memória replicada em mesh precisa dedupe TX+RX.
- **Estruturas "aprendizes" sem cap = OOM a médio prazo:** observations/requests/marketplace crescem com o runtime; cap + evicção FIFO é o mínimo para hot-path de agente.
- **Warn repetido por tick também é ruído de dínamo:** além de mentir, o warn constante alimenta loops de observação (I4→LLM) — sev honesta (`trace` para cache) + cortar o loop de feedback resolve os dois.
- **Amplificação Master: intents de saúde geradas pelo próprio agente:** `should_escalate_health_to_llm` escalando `sched=Violation/Warning` gera intents de "diagnostique e corrija" em loop (7× vs workers); escalada é para saúde *nova*, não para o eco da própria decis\u00e3o.
