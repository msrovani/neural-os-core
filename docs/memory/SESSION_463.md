# SESSION_463 — Campanha de confiabilidade do Mesh: OOM root-cause + fixes estruturais + validação 2 e 4 nós

**Data:** 2026-10-10 · **Branch:** main · **Escopo:** confiabilidade do substrato (diretriz do mantenedor — sem novos eixos)

## Contexto

`LOG AGENTES (4).txt` / FREEBU-0150: mesh OOM em T+6018 (stamp `CellNetwork::sleep_cycle` → crescimento de Vec → heap esgotado → CR2=0x18 → PF storm → CPU estacionada). Fragmentação limitada a 64.000B vs payloads reais de 885KB–11,36MB. Worker insistia em retries após recusas. Ordem do mantenedor: (1) reproduzir OOM e identificar a alocação, (2) preflight antes de serializar, (3) capacidade de transporte + testes de fragmentação, (4) unificar estado de peers + backoff, (5) re-teste com logs completos.

## Diagnóstico (M3 — reprodução e root cause)

- **Reprodução:** 2 instâncias QEMU WHPX 2GB `-NoModels -NoDisk` — crash em **T+37384/37067** (6× depois do T+6018 original, já com M1/M2b no binário).
- **Cadeia real:** `HybridAllocator` **bump-first** + bump `dealloc` **no-op** (`allocator.rs:721-730, 117-119`) = TODO churn de alocação (vivo + transitivo) vira **permanente**. Crescimento medido **~31,9KB/tick**: heap 512→768 (T+15852) →1024 (T+24116) →1179MB (T+32413, só 39.530 páginas = **PMM exausto**).
- **#PF:** `String::clone` → `memcpy+0x17`, `err=0x2` (write, not-present), CR2 = offset ~1,155GB do heap (base 0xffffffff811B1748); demand-page tentou curar e falhou (`heap-fail`) 3× → storm → park do BSP. A página faultada estava **473KB ABAIXO do limite mapeado** = corrupção de page-table por **PMM duplo-uso** (residual SESSION_252 "ora-1").
- **Stamp ≠ culpado** (confirma SESSION_415-417): `CellNetwork::sleep_cycle` é limpo — `connect` deduplica fan_in/fan_out, `drain_cell` limitado por `inbox_cap`, prune é `retain`.
- **Tickv RAM ABSOLVIDO:** teto 1MB fixo (`flash.rs:603`), compact funciona em RAM (`gc_allowed` true), zero linhas de flush em 34 min (nunca cruzou HIGH_WATER 256KB).

## Fixes (todos com testes host verdes; `cargo check` 0 erros)

1. **M1 preflight** (`cortex/compute.rs`): estimativa zero-alloc **antes** de serializar; gate contra o limite real do wire (64.000B = `FRAG_MAX_PARTS×FRAG_MAX_CHUNK`) + budget RAM; resposta (k×m f32) também gated; antigo gate pós-serialize removido. 6 testes (`--features p2p`).
2. **M2 estados de peers** (`k_nano/net/mesh.rs`): `PeerState {Available, Degraded, Unavailable}` derivado de `PeerHealth` (fail-closed, pior vence); backoff exponencial com jitter (LCG Knuth, cap 3200 ticks, sem f32); `MAX_TASK_ATTEMPTS=3`; `peer_available()` = gate único. 10 testes.
3. **M2b wiring** (`cortex/compute.rs`): dispatch gated por `peer_available` (substitui `node_count()>=1` que ignorava o circuit breaker); retry loop limitado (attempt gate + backoff + `record_peer_failure` por tentativa); buffers por-iteração (nada retido entre retries); slog com estado explícito (`available/degraded/unavailable`, `attempt=N/3`, `backoff=`). 5 testes.
4. **M4a testes FRAG/FRACK** (`k_nano/net/udp_broadcast.rs`): 9 testes host (remontagem in/out-of-order nunca parcial, truncamento, duplicação idempotente, perda + reuso de slot + expiry, boundary 64.000/64.001, wire format FRACK 14B, chunking TX, stash M10) + seam de RX `cfg(test)` + `build_frag_ack` extraído puro.
5. **TX guard honesto:** `send_fragmented`/`send_fragmented_unicast` com payload >64.000B → `warn` + `return false` (antes retornava `true` com mensagem indeliverável — classe SESSION_354 "timeout que retorna sucesso é pior que hang").
6. **M3c demand-page honesty** (`k_nano/allocator.rs::try_fault_in_heap`): P|W (0x3) recusa cura (TLB flush nunca cura RO-write); fresh-map fora da imagem do kernel **removido** (fail-open do high-half); bound `kvirt_end` obrigatório.
7. **TALC-first pós-boot** (`allocator.rs`): `route_alloc()` gate puro (`boot_phase_done && TALC_READY && fits`) → TALC primeiro pós-boot (**dealloc real** devolve memória do churn); dealloc roteado por range de ponteiro (consistência alloc/dealloc, sem lock — SESSION_438); `ALLOC_BYTES` + `top_agent` via `tick_in_progress()` a cada 1000 calls; `set_boot_phase_done()` no Runtime phase (`main.rs:5416`). 38 testes + 1 `#[ignore = needs-QEMU]`.
8. **PMM reserve** (`k_nano/memory.rs`): kernel image + boot_ramlog (0x1000_0000, 256KB) reservados no `init_from_usable_ranges`; **detector de duplo-uso** em `allocate_frame` (warn one-shot — o tripwire que teria pego o bug em 1 run); hang latente do `reserve_range` corrigido (iterava ~4,5e15× em overflow → `checked_add`). 12 testes.
9. **EventBus clone accounting** (`event-bus/bus.rs`): `CLONED_BYTES`/`CLONED_COUNT` por publish (P2P_PACKET tem 6 assinantes = 6 clones/pacote); `clone_churn_snapshot()`. 3 testes.
10. **SecurityAgent::alerts cap** (`hermes/security.rs`): CAP=64 + drop-oldest (7 push sites → `push_alert`); correlate ≥3 verificado com o buffer capado. 3 testes.
11. **GGUF payload gate** (`cortex/gguf.rs`): `Σ nbytes_for_elements` (checked) vs `data[data_start..].len()` → **Err** em payload curto (antes `Ok` com payload curto — consumidores diretos de `file.data` viam lixo); `gguf_payload_truncation_is_err` + harnesses `gguf_proof` estendidos.

## Validação runtime

- **M5 (2 instâncias 2c/2G WHPX):** **T+79005/79846 (~72 min), ZERO anomalias** (OOM/PF/park/heap-fail), bump 12MB vs 1.155GB do M3, peers=1, unsigned=0 badsig=0. Logs preservados: `logs/boot_mesh_{a,b}_m5.txt`.
- **Mesh4 (4 nós × 4c/2G, hub L2 `tools/qemu_l2_hub.py` :19000, launcher novo `tools/run-mesh4-lab.ps1`):** convergiu **peers=3 ×4**, node 2 eleito Master nos 4; **marco M3 cruzado (T+37k) com zero anomalias**; bump 11-12MB; `badsig` D=3 (pré-TOFU benigno, estático); `replay` estático (clocks convergiram); fail=2 conhecidos; 3 blips de peers (1 amostra cada = janela TTL de 60s).
- **Host:** `cargo check` 0 erros; suítes k_nano (360+), cortex, hermes, event-bus verdes com `--test-threads=1`.

## Lições

1. **bump-first + dealloc no-op = amplificador de vazamento:** o churn (vivo + transitivo) vira permanente; rotear pós-boot para TALC (gate `boot_phase_done`) devolve free real ao runtime.
2. **PMM sem reservas entrega frames em uso** sob pressão → corrupção de page-table → #PF storm → park. O detector de duplo-uso teria pego em 1 run; o reserve fecha o residual SESSION_252 "ora-1".
3. **`task_result` vazio/corrompido ≠ trabalho ausente** — verificar a árvore (git diff + testes) antes de re-despachar (3 casos nesta sessão: GGUF, M3c, PMM-lane1; 2 deles tinham o trabalho completo).
4. **Task Manager não atribui CPU de WHPX à coluna padrão** — medir delta de CPU acumulado do processo (`(CPU₂-CPU₁)/Δt`).
5. **Hub L2 userspace (`tools/qemu_l2_hub.py`) é o mecanismo multi-nó no Windows** — mcast quebra (SESSION_233) e socket p2p aceita 1 conexão.
6. **Blips de peers=N−1 por 1 amostra = janela TTL de saúde**, não divergência telemetria-vs-compute (essa exigiria persistência).
7. **Polling repetido do mesmo marco é anti-padrão** — usar monitores hook-driven (tarefa que completa no marco/anomalia) em vez de checagens manuais em loop.
8. **Instrumentação de alocação (`ALLOC_BYTES` + `top_agent` via `tick_in_progress`) atribui crescimento em 1 run** — o par "offset/delta/top_agent" no slog periódico é o bisector de vazamento canônico.

## Pendentes

- **M4b:** transporte de payloads grandes (bitmap escalável vs transferência em janela) — requer teste direcionado com payload grande (mesh4/M5 não exercitam compute pesado; preflight atual rejeita honestamente).
- **Cap engine neural-sgdb** (`clock_index`/`sk_clocks`/`entity_index`/`sk_ids`/`ram_l0l1`/lexical — fora do workspace, `C:\DEV\neural-sgdb`): bomba-relógio em runs com chatter real (inerte nos testes atuais).
  **REFINADO (revisão pós-crash):** a premissa "capar os índices" estava ERRADA — o engine é **ADD-only por design** (doutrina: "new facts accumulate; conflict is retrieval-time; the HOST decides and runs curate"). Capar violaria a doutrina. O mecanismo real: `expire_old` = **invalidar-não-deletar** (soft, `MemoryState::Invalidated`); o GC físico é `forget_purge`/TTL `expires_at`. O host (`k_ai/sgdb/nsgdb_bridge.rs::lifecycle_tick`) roda decay + expire_old + consolidate, mas **nunca arquiva nem purga** (`archived: 0` hardcoded; nenhum `expires_at` setado) → docs soft-invalidados permanecem nos índices derivados → crescimento limitado só pelo volume de escrita. **Fix correto = política de retenção no HOST** (quais memórias purgar/arquivar e quando) — decisão do mantenedor, não um cap mecânico. Inerte nos testes atuais (Tickv RAM nunca cruzou HIGH_WATER).
- **Lanes 6-7** (baseline bench + contrato e2e) — adiadas pela diretriz de confiabilidade.
