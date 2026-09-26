# Checklist — Runtime Hygiene (Cap + Evicção)

**Origem:** s410 — mesh 6 OOM (dedup VectorClock morto → RX MEM 648× → heap +11KB/tick → OOM fatal dos workers). Auditoria completa s410d nos agentes nativos (adendo SESSION_410).

**Regra de ouro:** toda estrutura de estado runtime de um agente (observações, aprendizados, índices, caches, logs, históricos) deve responder **SIM** às 5 perguntas abaixo. Estrutura estática de boot (constante por construção: DeviceTree, Limine memmap) está isenta — o que cresce com o *runtime* não está.

## As 5 perguntas

1. **Cap:** existe `const *_CAP` limitando o tamanho da estrutura? (números mágicos soltos não valem)
2. **Evicção:** quando o cap é atingido, o que sai? FIFO (`remove(0)` / `pop_front`), LRU, replace-by-key, take do buffer — qualquer política **determinística e documentada**.
3. **Dedup:** operação repetida com o MESMO conteúdo re-insere (duplicando) ou substitui/atualiza? Dedupe por conteúdo/label/path, nunca por nonce que muda a cada emissão (clock.tick(), timestamp, seq).
4. **Ownership do ciclo:** a estrutura é drenada/consumida por alguém? (ring drenado por tick não precisa de cap agressivo; stash à espera de consumidor precisa)
5. **Bounded por unidade HW/agentes** (quando aplicável): 1 entrada por device/GPU/peer — re-init substitui em vez de empilhar.

## Tabela de referência (auditada s410/s410d)

| Estrutura | Cap | Evicção | Status |
|---|---|---|---|
| `hermes::mesh_knowledge` LAST_TX/RX_HASH | 8 slots × hash | replace por slot | ✅ s410 (fix OOM) |
| `hermes::skill_observer` OBSERVATIONS | 128 | FIFO | ✅ s410 |
| `hermes::approval` REQUESTS | 64 | FIFO | ✅ s410 |
| `hermes::skill_marketplace` register_local | 32 | dedupe por nome | ✅ s410 |
| `k_ai::memory_systems` EMBED_INDEX | **128** | **dedupe por label + FIFO** | ✅ **s410d (fix sem teto)** |
| `k_ai::trust` escalation_log | **64** | **FIFO** | ✅ **s410d (fix sem teto)** |
| `k_nano::fts_search` IDX | **256** | **dedupe por path + FIFO** | ✅ **s410d (fix sem teto)** |
| `hermes::skill_gen` TASK_PATTERNS | **64** | **evicção 1ª chave** | ✅ **s410d (fix sem teto)** |
| `hermes::skill_opt` EVOLVING | **64** | **evicção 1ª chave** | ✅ **s410d (fix sem teto)** |
| `hermes::wasmi_rt` SKILL_PROVENANCE | **64** | **evicção 1ª chave** | ✅ **s410d (fix sem teto)** |
| `k_hal::gpu::backend` JOB_RINGS | 1/vendor | **replace-by-vendor** | ✅ **s410d (fix sem teto)** |
| `k_ai::self_learning` learned | 256 | FIFO | ✅ pré-existente |
| `k_ai::audit` AuditTrail ring | 4096 | ring wrap (head) | ✅ pré-existente |
| `k_ai::fs::inference_fs` TRAINING_BUF | 100 | FIFO | ✅ pré-existente |
| `hermes::fs::inference_fs` EPHEMERAL_BUF | 100 | FIFO | ✅ pré-existente |
| `hermes::fs::hermes_fs` CHAT_HISTORY | 50 | pop_front | ✅ pré-existente |
| `hermes::cognitive_bridge` SESSION | 48 | drain | ✅ pré-existente |
| `hermes::cognitive_bridge` NUDGE_QUEUE | 16 | dedupe + FIFO | ✅ pré-existente |
| `jarbas::clipboard_notify` TOASTS | 8 + TTL | FIFO + expiry | ✅ pré-existente |
| `k_hal::blit` PIN_CACHE | 24 | replace/lookup | ✅ pré-existente |
| `k_hal::offer` OFFERS | ≤32 (MAX_BINDS) | rebuild via refresh_from_tree | ✅ pré-existente |
| `k_hal::discovery` DEVICE_TREE | MAX_DEVICES | dedupe por PCI + clear_tree | ✅ pré-existente |
| `k_nano::udp_broadcast` UCAST_STASH | 8 | cap no push | ✅ pré-existente |
| `cortex::federated` FED_DELTAS | 1/nó | keep-latest + take no merge | ✅ pré-existente |
| `cortex::infer_queue` QUEUE_CAP | 8 (slots fixos) | backpressure Full | ✅ pré-existente |
| event-bus (bus/latent/mailboxes) | bounded | drop_oldest | ✅ pré-existente (S375) |
| `k_ai::sgdb` L0/L1 RAM arena | quotas do lifecycle | prune/consolidate SleepCycle | ✅ pré-existente |

## Processo para agente novo

1. Todo `static Mutex<Vec/BTreeMap/VecDeque>` que **cresce com eventos do runtime** entra neste checklist na PR.
2. Code review pergunta: "o que acontece depois de 10 milhões de ticks?" — a resposta deve citar o cap.
3. Métrica de smoke no lab: heap estável em janela longa (GOAL1 30min / mesh 6) sem crescimento linear.

## Anti-padrões que causam OOM (lições)

- **Dedupe com nonce auto-incrementado** (s410): `clock.tick()` no payload = dedup nunca derruba.
- **Push sem consumidor garantido**: stash/publicação à espera de dreno — cap local, não confie no consumer.
- **Re-init que empilha**: registro por unidade HW/agente deve substituir (by-vendor/by-key), não `push` de novo.
- **Índice derivado sem teardown**: provenance/metadata que sobrevivem ao unregister do objeto principal = órfãos acumulando.
