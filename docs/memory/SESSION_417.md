# SESSION_417 — Federation de saúde + OOM fail-closed (MACHINE/FLEET_HEALTH)

**Motivo:** (a) fila de demandas: ligar Hermes ao AUDIO_HEALTH → SYS_HEALTH unificado → investigar freeze de UI (stamps intent-router/sys_health_agent + #PF na tela) → federation: consolidar os JSONs de saúde num veredito de máquina único e propagar via mesh para o Master agregar a frota. (b) ciclo pós-tarefa: aprender/memorizar/documentar/versionar.

## Diagnóstico do freeze/#PF (3 boots, mesmo fenômeno)

- Sintoma na tela: `#PF ip=ffffffff8023bda7 err=0` + painel HUB HEALTH vivo a 60 Hz (`heap 1773/1792MB 98%` vermelho) + stamp FB no agente em execução (`network_agent`, `intent-router`, `sys_health_agent` variando entre boots).
- **Causa-raiz:** `LazyBumpAllocator` (global allocator) tem `dealloc = no-op` → toda alloc de runtime é leak permanente → auto-grow em passos de 256MB (512→…→**2030MB = teto da janela wrap 2^64**) → satura em ~30min com decode LLM → `alloc()` devolve **NULL** → caller deref null+offset → **#PF cr2=0x50/0x8** → `try_fault_in_heap` falha (cr2 fora de todas as faixas) → handler faz `hlt` eterno **no AP** que faultou (BSP vivo — painel continua).
- **Carimbo ≠ culpado:** o stamp FB mostra o agente em execução no AP no instante do fault (`AGENT_TICK_BUSY`), não o causador do OOM. Os 3 "freezes diferentes" = 1 OOM com 3 carimbos.
- Símbolos resolvidos via `nm target/limine-esp-tree/kernel.elf` (awk com `substr(a,9)` para 32 bits baixos — `strtonum` perde precisão acima de 2^53): boot4 = `hermes::memory::MemoryStore::tick_advance` (bei.rs:254 → `entry.data.clone()`); boot5 pós-fix = `cortex::moe::DynamicMoE::flush_merges` (cr2=0x8) chamado de `k_ai::expert_lifecycle::candidates_for_merge` no `bei_tick` (lifecycle MoE a cada 100 ticks). **Crash site migra conforme o fix — OOM sem gate tem N vítimas.**

## Fixes aplicados (fail-closed por classe, não por site)

1. **agent-core:** `try_with_agent_tick_lock_ms(budget_ms=2)` (via `TICK_CLOCK_HOOK`, zero-dep) no display always-first + mid-cycle boost — display pula frame em vez de girar eterno no lock global do tick. SCHED gap 10761 → ~3351.
2. **infer_queue:** `try_claim_into_active` recusa claim se `heap_headroom_critical()` (novo accessor em `k_nano::allocator`, piso 64MB); job EM CURSO → `Phase::Finishing` (termina honesto com payload parcial). Testes 10/10.
3. **MoE lifecycle (bei.rs + moe.rs):** gate `heap_headroom_critical()` no bloco do lifecycle do `bei_tick` (com re-check bounded padrão lost-wakeup SESSION_411) e dentro de `flush_merges`/`flush_splits`/`flush_births` (`cfg(target_os="none")` — self-test host tem BUMP_MAX_OFFSET=0). Pendências ficam para o próximo tick com headroom.
4. **Hybrid allocator (bump-first + TALC overflow):** TALC-first stallou o boot no SMP bring-up (claim demanda páginas antes do demand-page pronto) → revertido para bump primeiro, TALC como segunda chance quando bump recusa. Diagnóstico novo: contador `TALC_OVERFLOW_NULL` + stamp FB `ALLOC null bump+TALC size=… agente=…` (3 primeiras) — na próxima saturação a tela mostra se o fallback TALC é consultado/recusado (dívida estrutural: bump sem free satura por design; TALC overflow ainda não demonstrado sob saturação).
5. **Health agents pinados no BSP** (`set_affinity_ring(..., 0)`) + honestidade: `playback_demand` idle = UNKNOWN (nunca NO_GO); `playback_stall_diag` re-lê LPIB (~20µs) para `lpib_moving` vs `lpib_frozen` real.

## Federation de saúde (demandas 1-4 consolidadas)

**Modelo puro (k_nano::sys_health) — 10/10 testes:**
- `DomainVerdicts` (fields + reasons) com `worst()` = **NO_GO > UNKNOWN > GO** (UNKNOWN contamina — sem evidência não se afirma saúde; mesmo princípio do `n/a ≠ 0`).
- `MachineHealth` + `machine_verdict()` + `machine_verdict_json()` — `{"node":15,"overall":"NO_GO","sys":{"net":"NO_GO",…},"audio":{"playback":"GO",…},"reasons":[…]}` (padrão MESH_HEALTH).
- `parse_machine_json()` — parse no_std com contrato fixo (campos estáticos; `node` é número — o parse por needle `"node":"` falhava silenciosamente).
- `fleet_worst()` — agregação worst-of entre nós campo a campo, razões dedupe.

**Consolidação na máquina (hermes::sys_health):**
- `SysHealthAgent` assina AUDIO_HEALTH (lazy subscribe, `try_receive`) e consolida sys+audio num `MachineHealth` 1×/s; publica `MACHINE_HEALTH` + `LAST_MACHINE_JSON` (HUD sem re-parse).
- `machine_prompt()` = **um prompt só** ao LLM com o snapshot consolidado (substitui os prompts separados sys/audio).
- TX mesh: prefixo `MCH\0` + JSON via `mesh_send_large` (chunking automático), **cooldown 10 s** (1000 ticks) — heartbeat já flui 1×/s.

**Agregação de frota no Master (hermes::fleet_health) — 4/4 testes:**
- RX `MCH\0` via `P2P_PACKET` (repasse canônico `skill_sync::poll_p2p()` → `fleet_health::poll_p2p(now)`), valida o JSON **antes** de armazenar (não guarda lixo; `FLEET_MCH_BADJSON`).
- Snapshot por nó em **array fixo `[Option<NodeHealth>; 16]`** (teto do PEER_KEYS; evicção do mais velho por `last_seen`) — sem heapless (zero nova dep), sem Vec ilimitado (runtime hygiene SESSION_410).
- `aggregate()` → `fleet_worst()` → publica `FLEET_HEALTH` no bus + escala ao LLM do Master **1×/incidente** via `EscalationState` (política única) com cooldown maior (600 ticks — LLM do Master é o recurso mais caro do sistema).
- Testes com `TEST_LOCK` + `reset_state()` (statics compartilhados entre testes paralelos = lição SESSION_346). Teste dos handlers de RX usa o **wire format completo** (`MCH\0` + JSON) — helper `mch(json)`.

## Verificação

- QEMU 8GB/6c (boot `093629`): **MCH TX fluindo** (25 publicações mesh, bytes=97, cooldown respeitado), `FleetHealth subscribed P2P_PACKET` (RX wired), **zero PF_DBG**, a2_proof completo (prefill 79,8s), SYS/AUDIO_HEALTH 1Hz, boot passou do ponto do stall anterior (T+27.6s → T+41.7s+).
- Testes: k-nano sys_health 10/10, hermes fleet_health 4/4, hermes sys_health 6/6, cortex moe/infer_queue OK, workspace 0 erros (check canônico `-p boot`; `--workspace` sem `--target` dá erros fantasma de lang item no host toolchain).

## Residual

- **Stall silencioso pós-teto** (log para ~T+27-42s com QEMU queimando CPU, SEM #PF e sem ALLOC null) — pré-existente, classe lost-wakeup SESSION_411 ou lock no path pós-a2_proof (SGDB remember/bus). Investigação separada.
- **Dívida estrutural do heap:** bump sem free satura por design; TALC overflow implementado mas não demonstrado sob saturação (ver `ALLOC null bump+TALC` na próxima ocorrência). Upgrade real: TALC primário pós-boot com demand-page garantida no span.
- Federation e2e com 2 nós (RX de frota com `MCH` alheio real) e consumo de `FLEET_HEALTH` no HUD do Jarbas.
- n-sgdb: lote ADR/SESSION/IDEA pendente (MCP neural-sgdb não acessado nesta sessão).
