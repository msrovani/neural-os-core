# SESSION_419 — Fleet no HUD + heartbeat do scheduler + stall pós-teto + federation e2e + H3-revisit

**Motivo:** fila do maintainer — (1) consumir FLEET_HEALTH no painel HUB HEALTH; (2) fechar o stall silencioso pós-teto de heap (log morre ~T+27-42s, CPU queimando, sem #PF); (3) federation e2e dual-node; (4) estudar a ADR-0047-HMI frente à ADR-0112 (VRAM) e implementar o H3-revisit + melhorias no HMI; (5) aprender/memorizar/documentar/versionar/commit/push.

## 1. FLEET_HEALTH no HUB HEALTH (linha "fleet")

- `jarbas/display/agent.rs`: static `FLEET_SNAPSHOT` + `fleet_health_snapshot()`; lazy subscribe `TOPIC_FLEET_HEALTH` (mesmo padrão do `mesh_health_receiver`) com pull imediato de `last_fleet_json()`; parse no_std reusando `k_nano::sys_health::parse_machine_json` — **payload inválido nunca substitui o snapshot** (validar antes de armazenar).
- `gauges.rs`: `HUB_ROWS` 16→17 (depois 19); linha `fleet` = overall + nós + 1ª razão (`GO 2n`, `NO_GO 2n NET_LINK_DOWN`), cor por veredito (GO verde, NO_GO vermelho, ambos no pill; UNKNOWN warn **fora** do pill — sem evidência não puxa header). Sem agregado → `n/a` honesto (single-node nunca publica FLEET_HEALTH).
- Testes host: 3/3 `fleet_snapshot` com **TEST_LOCK** (statics compartilhados em paralelo sobrescreviam uns aos outros — lição s346/417 mordiu de novo).

## 2. Stall pós-teto: instrumentado, capturado, causa-raiz

- **Instrumentação nova (canal que sobrevive):** `agent-core::set_heartbeat_hook(min_ms)` — heartbeat **pós-retorno de cada tick** `(agente, ms)`; bin registra → `HB post-tick agent=X ms=N` via `log_quiet` (BOOT.LOG). O watchdog antigo só reporta se o tick RETORNA; num stall "CPU 100% sem log" nada era emitido. Marcadores `infer-claim`/`post-done` no infer_queue. **Validado em disco** (BOOT.LOG extraído do raw).
- **Captura 1 (pré-fix):** serial morreu com `[SMP] barrier wait pending=5 done=0 us=5000001` + dump de registradores → barrier SMP wedged.
- **Captura 2 (com instrumentação):** BOOT.LOG congela em **T+27121 `auto-grow 1792 MB → 2030 MB`** — heap no teto durante prefill id=2; serial morre pós `done id=2`. CPU queimando = BSP no spin do barrier.
- **Causa-raiz (classe):** `parallel_matmul`/`parallel_ternary_matmul` usam statics globais (CTX, ROWS/COLS_CLAIMED, barrier PENDING/DONE) **sem exclusão mútua** e são chamados de dois cores (BSP processa o reply pós-`done` enquanto o AP roda slice de prefill); o 2º dispatch faz `clear_queue`+`reset_barrier` no meio do 1º → DONE zerado → `pending=5 done=0` eterno → spin silencioso 60s → timeout → fallback corrompido.
- **Fix por classe** (`parallel_matmul.rs`): `SmpMmGuard` `try_lock` (2º dispatch → single-core honesto, nunca bloqueia), timeout 60s→**5s** (shapes reais ≤150ms), evidência de timeout persistida no BOOT.LOG.
- **Residual honesto (ABERTO):** prefill a2_proof + KV + reply ainda cruzam o teto de 2030MB (classe OOM-sob-teto s415-417). Próximo: gate de headroom fail-closed no prefill slice (TASK-01).

## 3. Federation e2e dual-node (demandas #621 fechadas)

- **Lacuna achada e corrigida:** `fleet_health::aggregate()` existia e era testado (4/4) mas **não tinha chamador em runtime** — MCH armazenado, FLEET_HEALTH nunca publicado. Fix: `fleet_tick(now)` 1Hz no `skill_sync::poll_p2p()` (aggregate + publish + overall no slog). Classe: "tópico novo sem produtor wired — wire e2e antes de marcar done".
- **e2e em 2 QEMU** (WHPX 8G/6c, hub UDP L2): sem flags netmode ambos caíram em node_id=15 (lição s411 reproduzida; replay=124, peers=0). Com flags: **node_id=2 e 3**, TOFU settled, `peers=1`, eleição Master/Memory. **`FleetHealth aggregate nodes=2 overall=UNKNOWN` em AMBOS os nós** (>100 agregações cada) — MCH cruzou a mesh, worst-of consolidado. UNKNOWN honesto (ambos sem modelo). Escala ao LLM não disparou — correta (EscalationState só com NO_GO persistente; UNKNOWN não conta).

## 4. H3-revisit (estudo ADR-0047-HMI × ADR-0112 → implementação)

- O descarte do H3 ("diffusion 263M+ inviável soft-float") apoiava-se em "compute neural exige executar na GPU" — premissa derrubada pela ADR-0112 (stage Mapped = host lê VRAM via BAR).
- **`k_hal/gpu/hint_render.rs`** (novo): MLP de render hints 64→128→16 W2A8 (~12KB packed); `upload_hint_weights` sobe pelo MESMO mecanismo do LLM (`upload_layer_weights` + RingPlan); `render_hints()` forward lendo pesos direto da aperture (`read_volatile` + `prefetcht0`), ReLU, saída (energia, matiz) por região; zero alloc, TSC medido; estágios honestos Off→Ready→Resident(→Device). Sem canário BAR → sem lane → compositor clássico (aumentativo, §6.4 da ADR).
- **HMI vivo:** HUB_ROWS 17→19 — linhas `vram` (stage BAR + MB residentes + GB/s canário) e `hints` (stage + fwd µs + n). `OrbSignals.infer_intensity` = **tok/s REAL** do decode → anéis do orb respondem à inferência medida (H4 literal; anti-spam temporal). Alimentador 2Hz `set_ui_state` (infer, tok/s, lane BAR, peers, mesh busy, activity) + tópico `HINTS` publicado (consumidor opcional).
- **QEMU single-node:** VirtIO-GPU vram=0MB igpu=true → Off → painel `vram n/a`/`hints n/a` — honesto. HW real (GTX 1050) mostrará Mapped + GB/s medidos.
- **Governança:** IDEA #623 (IDEA_BANK); addendum s419 na ADR-0047-HMI e ADR-0058 (premissa superada; diffusion puro segue descartado até ComputeDevice).

## Verificação

- `cargo check --release -p boot` (touch main.rs): **0 erros** (todos os ciclos).
- Testes: k-hal **66/66** (5 novos hint_render), jarbas fleet_snapshot 3/3 + hub 7/7, cortex **105/105** (threads=1), k-nano **232/232**, hermes **257/257** (fleet_health 4/4), agent-core 5/5.
- QEMU: single-node (MCH TX, painel honesto) + dual-node (nodes=2/3, peers=1, aggregate nodes=2 ambos os lados).

## Lições (→ AGENTS.md)

1. **Wire e2e antes de "done":** módulo testado 4/4 sem chamador em runtime = feature inexistente (aggregate nunca rodava). Teste de unidade prova a lógica; só o boot prova o wire.
2. **Watchdog que só reporta pós-retorno não detecta stall:** heartbeat pós-tick no canal persistente (BOOT.LOG) — a linha seguinte ausente localiza o agente.
3. **Statics de dispatch (barrier/CTX) são estado global:** dois cores no mesmo caminho (BSP pós-done + AP em slice) cruzam reset — exclusão mútua `try_lock` com fallback honesto, nunca bloquear o scheduler.
4. **Prefill cruza o teto:** fail-closed por classe cobre os consumidores pesados, mas o prefill a2_proof + KV + reply precisam de gate de headroom próprio (residual aberto).
5. **Premissa de descarte de ADR tem validade:** "263M+ inviável" morria no heap, não na física — ADR-0112 (VRAM) reabriu o H3 aumentativo. Revisar descartes quando a fundação muda.
