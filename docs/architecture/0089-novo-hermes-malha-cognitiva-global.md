# ADR-0089: Novo Hermes — Malha Cognitiva Real (neural-os-core + Apps Host) e Economia de Energia

**Status:** Proposed
**Lifecycle:** pesquisa
**Criação:** 2026-08-09
**Revisão:** 2026-08-09 (v4 — **auditoria de viabilidade**: removidos ZK-STARKs, atestação TPM (não existe Quote), tensor sharding 70B (impraticável no transporte 64KB/LAN), eBPF (kernel não é Linux), PeerID libp2p (sistema usa u8), WASI-NN/llama.cpp (não existem). **Foco: o que o kernel JÁ faz + apps host que falam o mesmo protocolo NoProto/UDP 42069**)
**Tags:** hermes, mesh, p2p, wasm, wasi, noproto, tickv, sgdb, ephemeral-agents, c-watt, layer-dispatch, dream-state, k3chj, whitepaper
**Fontes:** Conversa evolutiva "Novo Hermes" (2026-08-09), `ADR-0081` (malha P2P — fases A/B/C/F reais), `ADR-0063` (SGDB TicKV+NoProto+Índices IA), `ADR-0042` (K³CHJ), `ADR-0088` (Premissa Máxima), `ADR-0059` (wasmi), `ADR-0077` (Ring3), `ADR-0045` (Sound), `ADR-0057` (Compute Dispatch), `ADR-0078` (slots), verificação factual (explorer 2026-08-09: tetos FRAG/CHK, transportes, TPM, wasmi gating, economy, TLS, sem listener TCP)

---

## 0. Auditoria de Viabilidade (v4) — alucinações e devaneios removidos

Revisão claim a claim contra o código real. **Veredito:** `real` = implementado, `parcial` = base existe/gap, `inviável` = impossível hoje, `removido` = era devaneio.

| # | Claim (v2/v3) | Veredito | Fato verificado | Correção v4 |
|---|---|---|---|---|
| A1 | Malha global WAN via libp2p/DHT/ZK | **removido** | Mesh = 16 nós, node_id u8, só UDP broadcast/unicast **L2** porta 42069 (`mesh.rs:1568`); broadcast 255.255.255.255 não atravessa roteador; sem TCP/relay/WAN | WAN via **apps host relay** (TCP/TLS no SO host) que falam NoProto com o kernel — §13 |
| A2 | Atestação TPM (Quote/AK/PCR) no handshake | **inviável hoje** | `tpm.rs` só tem `sha256` + `extend_pcr` (TPM2 0x182); **sem** TPM2_Quote/ReadPCR/AK/EK/GetRandom | Substituída por **assinatura de binário Ed25519 + hash** (viável, sem hardware) — §11.2 |
| A3 | ZK-STARKs / ZK-proof de execução WASM | **removido** | Nenhum ZK no tree; crates ZK não são no_std; custo proibitivo | Prova de execução por **fuel do wasmi + hash + assinatura** (barata, honesta) — §12 |
| A4 | Tensor sharding de LLM 70B por camadas | **inviável** | Payload max ~64KB (FRAG 64×1000, `udp_broadcast.rs:464`; CHK ≈64.900B, `mesh.rs:1350-54`); sincronização por-token entre nós = latência × camadas em UDP lossy | Manter o **dispatch real** (matmul MW\0, experts EDR\0, FL FD\0/FM\0) — §14 |
| A5 | eBPF no kernel | **removido** | neural-os-core é bare-metal x86_64, não Linux; eBPF exige kernel Linux | Watchdog/polling (padrão atual do scheduler) — §18 |
| A6 | Wi-Fi Direct / BLE / Thread no kernel | **removido** | NICs = e1000/rtl8139/virtio; sem rádio BLE/802.15.4; wifi_agent AWAITING | Movido para **apps host** (SO hospedeiro tem as stacks) — §16 |
| A7 | WASI-NN + llama.cpp | **removido** | WASI-NN não existe (só `aios::*` + WASI stubs); llama.cpp não existe | Host import `aios_ai::infer` chamando **cortex BitNet real** — §10 |
| A8 | Contratos com "PeerID" 42/52B (libp2p) | **corrigido** | Sistema usa **node_id u8** (último octeto do IP, `mesh.rs:726`) | Contratos §8 com `node_id: u8` + MAC 6B |
| A9 | Modo Sonho "loss ajusta SLM local" | **corrigido** | `economy.rs` = orçamento local; backprop real é dívida (ADR-0083); sem treino on-device | Replay adversarial **estático** (análise de padrões + regras), sem treino de pesos — §17 |
| A10 | "90% das intenções <80ms" (número inventado) | **removido** | Sem medição | Removido; substituído por hipótese marcada |
| A11 | "Sem listener TCP no OS" contradiz app server | **confirmado** | Kernel só faz GET/POST (cliente); `serve_update.py` roda no **host**; `log_agent.rs:4` explícito | §13: apps host fazem o server-side da malha |

**Veredito geral:** a malha **LAN real já funciona e é rica** (skills, CRDT, FL, experts, matmul, persona). O que falta NÃO é libp2p/ZK/TPM — é (1) expandir os contratos NoProto no transporte existente, (2) apps host que falem o mesmo protocolo e façam o papel de WAN/IoT, (3) lifecycle efêmero local.

---

## 1. Premissas do Dev AIOS — checklist de conformidade

Toda decisão filtrada pelas premissas irrevogáveis (AGENTS.md + ADR-0088):

| # | Premissa | Aplicação neste ADR |
|---|---|---|
| P1 | **AIOS-First (ADR-0088)** — IA desde o boot, HITL, self-*, nada bypassado | Vetor operacional do AIOS-First; replicação autônoma cercada por gates determinísticos (§8.2); `CAP_ADMIN_OVERRIDE` = HITL; Modo Sonho = auto-cognição (§17) |
| P2 | **Agent/Skill-First** — tudo é Agente com manifesto | Envelope de replicação com manifesto embutido (`agent_id`, `bytecode_hash`, `wasi_capabilities`) — §8.1 |
| P3 | **Bare-metal Rust** — no_std, zero POSIX | Tudo proposto compila no_std. **Nada de libp2p/ZK/TPM-quote no kernel** (auditoria A1-A3) — WAN vive nos apps host |
| P4 | **HW Real First** | Canário HW antes de gate de qualquer hardware novo (§21) |
| P5 | **0 erros `cargo check --release`**; `cargo clean -p neural-kernel` | Todo incremento mantém o gate; guarda `tools/check_duplication.py` (§20) |
| P6 | **Zero Hallucination** | **v4:** nomes verificados contra o código (auditoria §0). Nada inventado: NoProto/TickV = repos upstream reimplementados (ADR-0063); ARQ = ART+BQ+Recall+Doc; llama.cpp/libp2p/ZK/TPM-quote = **inexistentes no tree e marcados como tal** |
| P7 | **Emagrecer bin** — lógica nas crates; bin só wire | Alocação por crate (§5); nada novo no bin; apps host = projetos separados fora do kernel |
| P8 | **Crates K³CHJ** — fonte única de statics | Nova static na crate base; hermes/jarbas = facades `pub use` |
| P9 | **Segurança determinística** — wasmi sandbox, Ring3 gated, validação fora da IA | wasmi + CapGate `aios::*` (real); Ring3 `TRY_ENTER_RING3=false`; AST determinístico no lugar de LLM-guardrail |
| P10 | **Gate v2.0.0** — N1–N5 + review + zerar `por_fazer` + OK humano | ADR = `pesquisa` pós-v2.0 |
| P11 | **Governança** — IDEA→ADR→sprint; toda ideia tem destino | Fases → IDEA_BANK referenciando esta ADR (§22) |
| P12 | **TECNOLOGIAS.md** + `update_tecnologias.py` | Registrar **na adoção**; novas propostas (apps host, C-Watt) entram quando implementadas |
| P13 | **SGDB = TickV+NoProto+Índices; FAT = blobs; região crua no fim do disco (C1)** | Persistência cognitiva → SGDB; blobs → FAT; `BL/` no TickvLite; nada de LBA fixo novo |
| P14 | **Malha P2P (ADR-0081)** — R0, fail-closed, FRAG\0, 16 slots, ACK | Base real e única deste ADR; **limites (16/u8/LAN) são contrato, não bug** |
| P15 | **Custo cripto ≠ latência** — HMAC dados, Ed25519 controle | Já implementado (`sign_packet_tiered`); C-Watt conta instruções (fuel), não tempo |
| P16 | **Pós-tarefa** — documentar→versionar→commit | STATE + SESSION + CHANGELOG + tag por fase |

---

## 2. Linha Evolutiva

| Vetor | OpenClaw | Hermes atual (K³CHJ) | Novo Hermes (este ADR) |
|---|---|---|---|
| Arquitetura | Scripts & pipelines | Agentes orquestrados (Agency + scheduler) | Malha agêntica efêmera **na LAN real** + apps host |
| Contexto/Memória | Sessão / buffer | SGDB (ART/BQ/NMD1 + TickvLite) + SleepCycle | Grafo causal distribuído (CRDT\0 já sincroniza) + Tickv |
| Linguagem de ação | API / tool use | Skills WASM (wasmi, ADR-0059) | Bytecode WASM + replicação via `TaskType::Replicate` |
| Gargalo superado | Falta de autonomia | Isolamento de contexto | Overhead de infra e latência (64KB/LAN) |

O salto não é "software maior" — é **computação ambiental na rede que já existe**: o mesh real vira o Sistema Operacional invisível do ecossistema de dispositivos.

---

## 3. A Pilha do Sistema (o que é real hoje)

```
+-----------------------------------------------------------------------+
|  CAMADA 4: APPS HOST (Windows/Linux/macOS/Android/TV) — fora do kernel|
|            falam NoProto/UDP 42069 com o kernel; fazem WAN/TCP/TLS    |
+-----------------------------------------------------------------------+
|  CAMADA 3: SEGURANÇA (identity Ed25519 + crypto HMAC/AEAD + trust)    |
|            + firewall; proposta: Chave de Sangue + BL/ revogação      |
+-----------------------------------------------------------------------+
|  CAMADA 2: REDE LOCAL — ADR-0081 real: mesh.rs + udp_broadcast.rs     |
|            UDP 42069, FRAG\0/FRACK\0/CHK\0, 16 nós, fail-closed        |
+-----------------------------------------------------------------------+
|  CAMADA 1: KERNEL — Rust + wasmi_rt (CapGate aios::*) + EventBus/Mpmc |
+-----------------------------------------------------------------------+
|  CAMADA 0: MOTOR DE ESTADO — TickvLite + NMD1 + ART/BQ (ARQ) + CRDT   |
+-----------------------------------------------------------------------+
```

| Camada | Tecnologia real | Proposta v4 |
|---|---|---|
| 0 — Estado | `k_nano::storage::tickv` (TickvLite), `k_ai::sgdb` (AiosDatabaseEngine, NMD1, ART+BQ) | `BL/` revogação; quórum de consolidação sobre `crdt_sync` |
| 1 — Kernel | `wasmi_rt` (fuel 5M, CapGate), `app_factory` A, EventBus/Mpmc | Lifecycle efêmero (fila + wasmi pool); `aios_ai::infer` host import |
| 2 — LAN | mesh + udp_broadcast (42069, FRAG/FRACK/CHK, token bucket) | `TaskType::{Replicate, Revoke}` no transporte existente |
| 3 — Segurança | identity/crypto/trust/firewall | Chave de Sangue (Pair, Ed25519); `BL/` CRDT |
| 4 — Apps host | (fora do kernel) | Runner Host app: mesmo protocolo NoProto; relay WAN/TCP |

---

## 4. As 4 Leis do Novo Hermes

1. **Zero-copy por padrão.** `NoProtoParser` (overlay, `net/noproto.rs:216`) + `MemoryDocView` (NMD1, `memory_doc.rs:226`). Sem JSON no caminho quente.
2. **Efemeridade estrita.** `wasmi_rt::run_wasm` já instancia/executa/descarta; gap = lifecycle + transporte P2P de agentes (§7, §8).
3. **Soberania do edge.** Nuvem = nó externo não-confiável; hoje: net local-first (netstack + mesh LAN).
4. **Ciclo de sonho assíncrono.** Já existe: `SleepCycleAgent` REPLAY/DREAM/CONSOLIDATE/PRUNE/REFLECT (§17).

---

## 5. Mapa de Conexões com o Código Atual

Referências `caminho:linha:símbolo`. "**existe**" = hoje; "**gap**" = proposta v4.

### 5.1 Motor de estado (Camada 0)

| Item | Código real | Nota |
|---|---|---|
| KV Flash | `k_nano/src/storage/tickv.rs:64` `TickvLite` — append-log, `put` (:446 V-flag + GC), `get` (:475 CRC32), `compact` (:375), `write_ckpt` (:113) | Região: `flash.rs:33` fim do NVMe (C1); fallback RamFlash 1MB volátil logado |
| Facade | `k_ai/src/sgdb/store.rs:93` `put_kv`, `:102` `get_kv`; namespaces `md/ hanr/ pkg/ skill/ audit/ vdb/ sys/ hw/` | `BL/` entra aqui |
| Doc | `k_ai/src/sgdb/memory_doc.rs:102` `MemoryDoc` (NMD1) + `MemoryDocView` (:226) + `VectorClock` (:51) | Reimplementação inspirada no repo noproto (ADR-0063:186/248) |
| ARQ | `ARQ = ART + BQ + Recall + Doc`: `art.rs:87` `ArtIndex` + `bq.rs:38` `BqFlatIndex` + `layers.rs:92` `recall_semantic` + `memory_doc.rs` | Indexador de inferências (definição do maintainer) |
| CRDT sync | `k_ai/src/sgdb/crdt_sync.rs:43` `CrdtMemorySync`, `:236` `crdt_sync_global` via NoProto `TaskType::Sync`; wired `bei_init.rs:467` | **Já sincroniza memória entre nós** |
| Init | `main.rs:1930` `sgdb::boot_init()`; `main.rs:2717` `SgdbAgent` | Hook do `BL/` |

### 5.2 NoProto (transporte) — reimplementação in-tree do repo upstream

**NoProto é repo real** (`github.com/noproto/noproto`, ADR-0063:248) usado como base; tree = reimplementação `k_nano/src/net/noproto.rs`: `AiosTaskPacket` (36B: magic "AIOS", clock Lamport u64, source/dest id **u8**, `TaskType`, priority, tensor_len, param_len, flags) (:18); `TaskType {Unknown, Inference, Training, Sync, ModelUpdate, Heartbeat, Error, Shutdown}` (:44); `PacketFlags {persist, require_ack, compressed, encrypted}` (:66); `NoProtoParser` (:207). Storage = NMD1 (ADR-0063:185-201, "não portar crates upstream").

**Gap:** novos `TaskType::Replicate` e `TaskType::Revoke` (§8) — extensão simples do enum existente.

### 5.3 ARQ = ART + BQ + Recall + Doc (Indexador de Inferências)

| Componente | Código real | Função |
|---|---|---|
| **ART** (fatos L0-L3) | `sgdb/art.rs:87` `ArtIndex` (Node4/16/48/256 + SSE) | Índice de fatos O(k) |
| **BQ** (vetores L4-L5) | `sgdb/bq.rs:38` `BqFlatIndex` + `hamming_dispatch.rs:20` | Índice vetorial, top-k Hamming SIMD |
| **Recall** | `sgdb/layers.rs:92` `recall_semantic` + `:183` `remember_exchange_full` | Recuperação de inferências |
| **Doc** | `sgdb/memory_doc.rs:102` (NMD1) + VectorClock | Liga vetor → conteúdo |

**Substrato de execução:** `MpmcQueue` (`sync/mpmc.rs:20`), `SpscChannel` (`async_rt.rs:34`), `BoundedChannel` (`event-bus/channel.rs:9`), `EventBus` (`event-bus/bus.rs:26`; singleton `k_nano/globals.rs:12`), `LogicalClock/VectorClock` (`sync/clock.rs:18/79`).

### 5.4 Malha P2P (Camada 2) — o que é REAL

`crates/k_nano/src/net/mesh.rs` (1887 linhas): `NodeTier` L0..L4 (:80), `NodeCapabilities`+`capacity_score` (:110/181), `BrainMeshEngine` 16 nós (:319), `MESH_ENGINE` (:696), `mesh_tick` (:705), `node_id() → u8 = IP[3]` (:726), `PEER_KEYS[16]` (:783), `PEER_HEALTH[16]` (:786), `PEER_MAC_CACHE[16]` (:790), `TOKEN_BUCKET` (:794), `peer_public_key()` (:908), `CryptoTier {Full, Relativized}` (:1238), `set_segment_key` (:1266), `probe_node` (:1077), `TOPIC_P2P_PACKET` (:1531), `TOPIC_MESH_HEALTH` (:1535), `p2p_tick` (:1567 — heartbeat `PK\0`+pk+`CAP\0` :1619, fail-closed :1750, ROLE\0 :1808, CHK\0 :1841).

`crates/k_nano/src/net/udp_broadcast.rs` (855 linhas, porta **42069**): `sign_packet_authentic` (Ed25519) (:97), `sign_packet_tiered` (HMAC 32B) (:109), `seal_packet_tiered` (:179), `verify_or_open_tiered` (:193), `send_fragmented` (:472), `REASSEMBLY[16]` (:522), `send_fragmented_unicast`/FRACK\0 (:669/:739).

**O que JÁ trafega P2P (implementado, verificado):**

| Protocolo | Path | Payload |
|---|---|---|
| `SKILL\0`/`PROMOTE\0` | `hermes/src/skill_sync.rs:193,303,335` | Promoção de skills (worker→master) |
| SkillOffer | `hermes/src/skill_marketplace.rs:86` | Oferta de skill (NoProto ModelUpdate) |
| `CRDT\0` | `k_ai/src/sgdb/crdt_sync.rs:186` | Sync de memória SGDB |
| `FD\0`/`FM\0` | `k_ai/src/fl_trainer.rs:305,316,359` | Federated learning (deltas/pesos) |
| `FED\0`/`FEDW\0` | `cortex/src/federated.rs:156,194` | Router MoE federado |
| `EDR\0` | `cortex/src/mesh_distrib.rs:257,450` | Dispatch de experts (Master→Worker) |
| `MEM\0`/`SOUL\0`/`PERS\0` | `hermes/src/mesh_knowledge.rs:23-25,65,82,114` | Memória L3/L4, persona |
| `MW\0` | `cortex/src/compute.rs:359` | Matmul/weights worker |
| `PONG\0`/`ROLE\0`/`CHK\0` | `mesh.rs:1119,1808,1841` | Probe, papel, chunk |

**Limites (contrato, não bug):** 16 nós, node_id u8, teto ~64KB (FRAG `:464` 64×1000; CHK `mesh.rs:1350-54` ≈64.900B), anti-replay clock estrito sem janela WAN (`mesh.rs:1745` ponytail), só UDP L2.

### 5.5 Runtime WASM (Camada 1)

`hermes/src/wasmi_rt.rs`: `CAP_*` (:23-33), `MAX_WASM_ALLOC=1MB` (:36), `DEFAULT_FUEL=5_000_000` (:51), `check_cap` (:55), `install_host_abi` (:79 — `aios::log/debug/get_tick`, `aios_net::http_get`, WASI stubs), `run_i32_2` (:150), `run_wasm` (:231). `app_factory.rs`: **backend A (wasmi) executa SEMPRE** (:150); B/C gated (`register_native_ring` :29, `isolation_ring_available` :201). `wasm_build.rs`: op-IR→wasm. Kernel: `exec_arena.rs` W^X, `isolation_ring.rs` (`ring3_is_safe` só KVM :37, `TRY_ENTER_RING3=false` `user_mode.rs:45`).

**Gap:** lifecycle efêmero + transporte P2P de agentes. WASI-NN NÃO existe (só `aios::*`) — proposta §10 = host import `aios_ai::infer`.

### 5.6 Segurança (Camada 3)

| Item | Código | Gap v4 |
|---|---|---|
| Ed25519 | `identity.rs:31` verify, `:12` TRUSTED_PUBLIC_KEYS, `:65` IdentityToken | — |
| HMAC/AEAD | `crypto.rs:47` hmac_sha256, `:189` aead (ChaCha20-Poly1305, X25519 DH :151) | — |
| Trust | `k_ai/trust.rs:136` `trust_allow_agent(token,agent,skill)`, `:279` check_syscall | + trust_level (Chave de Sangue) |
| Fail-closed | `mesh.rs:1668-1782` (unsigned/replay → drop) | — |
| TPM | `tpm.rs:298` `extend_pcr` (0x182) — **sem Quote/AK** | **não usado para atestação** (inviável, A2) |
| Firewall | `neural-kernel/firewall.rs` | — |

### 5.7 Scheduler / Agency

`agent-core/lib.rs`: `AgentRegistry` (:190), `set_urgency` (:251), `check_budget` (:305), `run` (:387), watchdog (:470). `k_ai/agency.rs`: `for_task` (:58), `delegate` (:82). Registro: `agents.rs:1988` (147) + `:2027` (HW). **Gap:** agentes efêmeros não passam pelo registry.

### 5.8 Inferência (Camada 1)

| Capacidade | Código real | Nota |
|---|---|---|
| LLM | `cortex` BitNet ternário + Trinity MoE + router | **llama.cpp não existe** — WASI-NN v4 = `aios_ai::infer` → cortex |
| STT/TTS/VAD | `jarbas/src/audio/` (Piper, Stt CTC, VAD, WakeWord) + `TOPIC_AUDIO_IN`/`TOPIC_STT_TEXT` | Percepção local real (ADR-0045) |
| Embeddings | `k_ai/memory_systems.rs:17` `load_bge` → `remember_exchange_full` | RAG real |

### 5.9 Economia

`k_ai/src/economy.rs:52` `BudgetManager` — orçamento LOCAL (tokens/memória/ciclos). **Sem wallet/moeda** (grep wallet|coin|currency|credit|cwatt = 0). C-Watt = extensão (§12, sem ZK).

### 5.10 Modo Sonho (real)

`hermes/src/agents.rs:2341` `SleepCycleAgent` (PollEvery 1000): REPLAY (r3 `update_with_replay`), DREAM (`evolve_dream_tick` + BitNet QA), CONSOLIDATE (:2547 → `checkpoint_working` + `tiers::consolidate_tiers`), PRUNE (:2600 → `prune_working_ram`), REFLECT (`TOPIC_SELF_EVOLVE`). `k_ai/tiers.rs:77`.

---

## 6. As 4 Leis (invariantes de código)

- L1 → payloads novos em NoProto/NMD1, sem JSON no caminho quente.
- L2 → envelope `Replicate` com `energy_budget`/`ttl_hops`; instância destruída pós-commit.
- L3 → fallback: reflexo → SLM local → nó LAN → app host (WAN).
- L4 → consolidação fora do caminho crítico (SleepCycle garante).

---

## 7. Máquina de Estados do Agente Efêmero

```
UNINSTANTIATED → ALLOCATING → RUNNING → (SUSPENDED ⇄ RUNNING) → COMMITTING → TERMINATED
```

| Fase | Transição | Primitiva real | Gap |
|---|---|---|---|
| 0. UNINSTANTIATED | evento chega | `EventBus::publish` / `MpmcQueue::try_send` | — |
| 1. ALLOCATING | mapeia buffer + instancia | `wasmi_rt::run_wasm` (compile+instantiate) | instance pool |
| 2. RUNNING | executa | `wasmi_rt::run_i32_2` / `app_factory::execute` (A) com fuel | — |
| 3. SUSPENDED | espera I/O | **gap:** wasmi sem yield — salvar linear memory + `SpscChannel` wake | gap |
| 4. COMMITTING | grava delta | `sgdb::{put_doc,put_kv}` → `TickvLite::put` | — |
| 5. TERMINATED | destroi | drop; linear memory ao pool | pool |

**Segurança (P9):** injeção na Fase 2 → morre na Fase 5; CapGate `aios::*` + `permission_gate.rs` = fronteira determinística.

---

## 8. Contratos de Rede (extensão do NoProto interno — node_id u8, não PeerID)

Padrão: header `repr(C,packed)` + slices com len (como `AiosTaskPacket`). Novos `TaskType` em `k_nano/src/net/noproto.rs:44`:

```
TaskType::Replicate  // envelope de replicação autônoma
TaskType::Revoke     // revogação causal CRDT
```

### 8.1 Envelope de replicação (payload de `TaskType::Replicate`)

| Offset | Campo | Tamanho | Observação |
|---|---|---|---|
| 0 | agent_id | 36 | UUID ASCII |
| 36 | bytecode_hash | 32 | SHA-256 do .wasm |
| 68 | energy_budget | 8 LE | instruções WASM (fuel) |
| 76 | ttl_hops | 1 | saltos restantes (max 3) |
| 77 | **src_node_id** | 1 | **u8 (real do mesh), não PeerID** |
| 78 | src_mac | 6 | MAC real (PEER_MAC_CACHE) |
| 84 | min_host_fitness | 1 | 0-100 |
| 85 | max_memory_alloc_mb | 2 LE | teto RAM |
| 87 | wasi_capabilities | 4 LE | bitmask (§8.2) |
| 91 | state_delta_key | 16 | chave TickV origem |
| 107 | input_len | 4 LE | — |
| 111 | input_payload | ≤64.000 | **respeitar teto FRAG 64KB** |

### 8.2 Bitmask `wasi_capabilities`

| Bit | Flag | Exige HITL? |
|---|---|---|
| 0 | CAP_INFERENCE_LOCAL | não |
| 1 | CAP_READ_TICKV | não (read-only) |
| 2 | CAP_WRITE_TICKV | sim |
| 3 | CAP_NETWORK_OUTBOUND | sim |
| 4 | CAP_HARDWARE_IO | sim |
| 31 | CAP_ADMIN_OVERRIDE | sim (sempre) |

**Regra de ouro (P1/P9):** replicação autônoma = `wasi_capabilities & 0x14 == 0` (zero-caps de escrita). Gatekeeper rejeita lendo 2 bytes, antes de instanciar WASM.

### 8.3 Atestação de binário (substitui TPM — viável hoje)

| Offset | Campo | Tamanho | Observação |
|---|---|---|---|
| 0 | nonce | 32 | anti-replay |
| 32 | **runner_hash** | 32 | SHA-256 do binário (host app ou kernel) |
| 64 | signer_node_id | 1 | u8 |
| 65 | signature | 64 | Ed25519 (master key da casa) |

**Sem TPM, sem ZK** (inviáveis, auditoria A2/A3). A integridade vem de: binário publicado com hash assinado pela master key → nós verificam antes do Pair. Se o binário mudar, o hash quebra. Suficiente para o modelo de ameaças LAN atual (fail-closed + TOFU `PK\0` já existem).

### 8.4 Chave de Sangue (payload de `TaskType::Pair`)

| Offset | Campo | Tamanho |
|---|---|---|
| 0 | family_cluster_id | 32 |
| 32 | node_peer_id | **1 (u8 real)** |
| 33 | node_mac | 6 |
| 39 | trust_level | 1 (1=Sangue, 2=Guest) |
| 40 | issued_timestamp | 8 LE |
| 48 | expires_timestamp | 8 LE |
| 56 | authorized_wasi_mask | 4 LE |
| 60 | master_signature | 64 (Ed25519) |

### 8.5 Revogação causal (payload de `TaskType::Revoke`)

| Offset | Campo | Tamanho |
|---|---|---|
| 0 | revoked_node_id | 1 (u8) |
| 1 | revoked_public_key | 32 |
| 33 | causal_sequence | 8 LE |
| 41 | revocation_timestamp | 8 LE |
| 49 | issuer_node_id | 1 |
| 50 | master_signature | 64 |

---

## 9. Persistência (Camada 0)

| Dado | Destino | Via |
|---|---|---|
| Memória cognitiva (L0-L7, HANR, episódios, embeddings) | SGDB (`md/`, `hanr/`, `ts/`, `epi_`) | `sgdb::{put_doc, remember_*}` |
| SELF.STATE, checkpoint, audit, pins, net config | SGDB (`sys/*`, `audit/*`) | `put_kv/get_kv` |
| **Revogação** | **`BL/` + SHA256(node_id)** | novo — LWW por `causal_sequence` |
| Blobs grandes (modelos, firmware, skills >4KB) | FAT32/NeuralFS | `k_nano::fat32`, `k_nano::neural_fs` |

**CRDT de revogação:** chave `BL/` + SHA256(node_id); LWW por `causal_sequence`; validação Ed25519 (master key) antes de gravar; propagação via `crdt_sync::crdt_sync_global` (`crdt_sync.rs:236`) + heartbeat; corte de sessão no gatekeeper (O(1)).

**Grafo causal:** `VectorClock` (8 nós) + `CrdtMemorySync` já existem. Gap: arestas explícitas como docs `md/causal/{src}/{dst}` no ART (incremental, sem storage novo).

---

## 10. Inferência — host import `aios_ai::infer` (substitui WASI-NN/llama.cpp)

**Hoje (real):** `cortex` BitNet (LLM), `jarbas` Piper/STT (áudio), `k_ai` BGE (embeddings) — in-process por crate.

**Proposta (gap, viável):** novo host import `aios_ai::infer` em `wasmi_rt::install_host_abi` (padrão já usado por `aios_net::http_get` :117). O agente WASM chama `aios_ai::infer(model_id, input)` → host despacha para `cortex` (BitNet/embeddings) ou `jarbas` (STT/TTS) conforme `model_id`. **Sem WASI-NN spec, sem llama.cpp** — é um import host a mais no padrão existente. Backend auto-select já existe (`allow_avx2()` + SIMD dispatch em cortex).

Model pool lazy: hoje modelos residentes por slot; gap = evict por idle (ADR-0046 streaming já orienta).

---

## 11. Modelo de Ameaças e Defesa Imunológica (realista)

| Biológico | Computacional real hoje | Proposta v4 |
|---|---|---|
| Macrófagos/NK | Gatekeeper `verify_packet*` + bounds-check Rust | — |
| Células B de memória | `TrustCache` + `sys/tls_pins` + audit | grafo de antígenos (`BL/`) |
| Febre/inflamação | — | queda do fitness score do nó (`PeerHealth`) |
| MHC (auto-reconhecimento) | TOFU `PK\0` + fail-closed | **hash de binário assinado** (§8.3) |
| Apoptose | — | honeypot sintético (tarefa com resposta pré-calculada) + banimento `BL/` |

### 11.1 Vetores APT e defesas

| Vetor | Camada de falha | Defesa (real → proposta) |
|---|---|---|
| Side-channel no WASM | vazamento do host | sandbox wasmi + fuel (real). **Sem ZK** — prova por fuel+hash+assinatura |
| NoProto offset | ponteiros fora do buffer | `NoProtoParser::validate_packet` + bounds-check `saturating_add` |
| Dream poisoning | ruído vira regra no CONSOLIDATE | **quórum:** padrão remoto consolidado só com validação de 3+ nós (estende `crdt_sync`) |
| Rogue Node (binário alterado) | Runner Host adulterado | hash de binário assinado pela master key (§8.3); TOFU `PK\0` + fail-closed |
| Worm/fork bomb | replicação infinita | `energy_budget` + `ttl_hops` (divide, não multiplica) |
| Criptomineração | loop PoW | `DEFAULT_FUEL` aborta (real) + detector de padrão de instrução |

### 11.2 Atestação de binário (handshake)

```
[A]──nonce(32B)──►[B]   B: assina (nonce ‖ runner_hash) com a chave da casa
[A]◄─(nonce, runner_hash, signature Ed25519)─[B]
[A]: verifica sig vs master pubkey + compara runner_hash com o oficial → abre canal
     senão → drop
```

**Anula o Nó do Mal sem TPM:** alterar 1 bit do binário muda o runner_hash; sem a master key da casa o atacante não assina. Fallback para Guest (zero-caps) se desconhecido.

### 11.3 Self-Defender (sobre o real)

Real: `k_ai/self_heal.rs` — `HEALTH_ISSUE` → Hermes → LLM diagnostica → NetAgent HTTP GET → hot-load (:342/363). Proposta: agente efêmero morto por injeção deixa **antígeno** (rastro no grafo de erros); DREAM gera `.wasm` com verificação estática; propaga via mesh. **Sem treino de pesos** (backprop é dívida ADR-0083).

### 11.4 Anéis de confiança (proposta, sobre o real)

```
ANEL 2: Guest — Pair trust_level=2, zero-caps WASI, sem TickV da casa
ANEL 1: Sangue — Pair trust_level=1 assinado pela master key → TickV + arcos reflexos
```

Fronteira: `trust_allow_agent(token, agent, skill)` + `trust_level` do Pair.

---

## 12. Economia C-Watt (base: `k_ai::economy` — sem ZK)

**Hoje:** `economy.rs:52` `BudgetManager` — orçamento local. Sem moeda.

**Proposta (gap):**

```
Custo (C-Watt) = (instruções WASM × k1) + (tokens SLM × k2) + (bytes × k3)
```

- Determinismo: **fuel do wasmi conta instruções** (`DEFAULT_FUEL`) — sem disputa de tempo.
- **Voucher assinado (sem ZK):** cliente bloqueia saldo assinado (Ed25519); provedor executa; devolve `(fuel_usado, resultado, hash)`; cliente valida e assina voucher (`sequence_number` crescente anti-replay). Prova de execução honesta = fuel + hash + assinatura.
- **Reciprocidade causal:** infra doa compute ocioso → créditos; consome quando precisar.
- Implementação: estender `economy.rs` com ledger `sys/cwatt/*` (TickvLite) + validação no mesh (novo `TaskType` opcional, pós-v2.0).

---

## 13. A Rede Mesh Real: neural-os-core + Apps Host

**Fato (verificado):** kernel só faz cliente (GET/POST), sem listener TCP (`log_agent.rs:4`; `serve_update.py` roda no host). Mesh = UDP L2 LAN, 16 nós, u8.

**A malha global (WAN) NÃO é feita no kernel** — é feita por **apps host** (Windows/Linux/macOS/Android/TV) que:

1. Falam o **mesmo protocolo NoProto** (`AiosTaskPacket` + `TaskType` + assinaturas) sobre UDP 42069 com o kernel;
2. Servem de **relay WAN** (TCP/TLS no SO host) entre malhas locais — o kernel nunca sai da LAN;
3. Implementam Wi-Fi Direct/BLE/Thread (stacks do SO host, impossíveis no bare-metal — A6);
4. Rodam o **Runner Host** (binário Rust std) que hospeda SLMs locais e compila `.wasm` — o análogo cross-OS do kernel (ADR-0086 já orienta o fluxo de instalação).

**Contrato app↔kernel (proposta de protocolo):** o app host replica o comportamento do mesh kernel: heartbeat `PK\0`+pk+`CAP\0`, assinatura tiered (HMAC/Ed25519), FRAG\0/FRACK\0 — implementado como **crate compartilhada** (proposta: `crates/mesh_proto/` no workspace, reutilizável no_std kernel + std app), não como código duplicado (guarda P8).

**Gap honesto:** hoje não há app host algum — o `serve_update.py` só serve arquivos. A Fase de apps host é a maior lacuna de engenharia deste ADR (e a única via realista para WAN/IoT).

---

## 14. LLMs no Mesh — Dispatch Real (não sharding de 70B)

**Inviável (A4):** tensor sharding de LLM grande por camadas entre nós — payload 64KB + sincronização por token + UDP lossy. **Removido.**

**Real e já implementado:** dispatch de trabalho por pacotes:
- `MW\0` matmul (`cortex/src/compute.rs:359`) — matmul 64×64 ~17.5KB round-trip validado;
- `EDR\0` experts (`cortex/src/mesh_distrib.rs:257,450`) — Master→Worker por capacidade (`capacity_weighted_assign` :373);
- `FD\0`/`FM\0` federated (`k_ai/fl_trainer.rs`) e `FED\0` (`cortex/federated.rs`) — gradientes/pesos, não dados.

**Proposta:** o SLM de cada nó fica residente local (tiers §15); o mesh distribui **trabalho** (tokens de contexto, matmuls, experts), nunca **modelos inteiros**.

---

## 15. Especialização por Tiers (SLMs por dispositivo — via apps host)

| Tier | Dispositivo (host) | Modelo | Função |
|---|---|---|---|
| 0 | sensores/ESP32 (app host) | VAD (`jarbas::audio::vad`), regras | percepção, eventos |
| 1 | celular/smartwatch (app host) | SLM 0.5-1.5B | intenção, arco reflexo |
| 2 | Smart TV/mini PC (app host) | SLM 3-7B | contexto, RAG no SGDB |
| 3 | PC/workstation | 14B+ | raciocínio, WASM, provas |

Roteamento em cascata — base real: `app_factory::analyze_and_recommend` (A/B/C) + `Agency::for_task/delegate`:

1. "Ligue a luz" → arco reflexo (0ms LLM).
2. "Resuma meu dia" → Tier 1 → Wi-Fi Direct (app host) → Tier 2.
3. "Gere um módulo WASM" → Tier 1 → app host relay → Tier 3.

**Hipótese (não medida):** a maioria das intenções diárias resolve no Tier 1 sem acordar modelo maior — exige benchmark antes de qualquer claim.

---

## 16. Comunicação (realista)

| Canal | Papel | Onde |
|---|---|---|
| Ethernet/UDP LAN | malha ADR-0081 (42069) | **kernel — real** |
| Wi-Fi Direct / BLE / Thread | IoT, beacon, alta banda | **apps host** (stacks do SO) — impossível no bare-metal |
| TCP/TLS WAN | relay entre malhas | **apps host** (kernel só cliente) |

Smart TV como nó âncora: métrica análoga hoje = `NodeCapabilities.capacity_score` (`mesh.rs:181`) + `PeerHealth` (`mesh.rs:274`).

---

## 17. Modo Sonho (evolução realista do SleepCycle)

| Fase real | Hoje | Proposta v4 |
|---|---|---|
| REPLAY | `update_with_replay` + persist router | + replay adversarial (análise estática de padrões) |
| DREAM | `evolve_dream_tick` + BitNet QA | + detecção de padrões de injeção; **sem treino de pesos** (backprop = dívida ADR-0083) |
| CONSOLIDATE | `checkpoint_working` + `consolidate_tiers` | + **quórum** (3+ nós) p/ padrão vindo de nó remoto (anti dream-poisoning) |
| PRUNE | `prune_working_ram` + replay | + poda por assinatura de origem (trusted/untrusted) |
| REFLECT | `self_evolve` + `TOPIC_SELF_EVOLVE` | + vacina estática (regra de validação) propagada via mesh |

---

## 18. Extensões Futuras (realistas)

| Módulo | Base real | Veredito v4 |
|---|---|---|
| Cranelift AOT | feature `jit-cranelift` (ADR-0059, opt-in, gated ring+HITL) | manter gated |
| Crypto-shredding | `TickvLite` invalidation + flash erase | chave de queima + overwrite |
| MOC/CXL | — | **pesquisa distante**, sem plano |
| eBPF | — | **removido** (kernel não é Linux) — apps host podem usar equivalente do SO |
| WAN | — | via apps host relay (§13), nunca libp2p no kernel |

---

## 19. Papel do Hermes (H) no K³CHJ

1. **H = Transport & Agent Engine.** Empacota intenções em `.wasm` (`wasm_build`/`app_factory`), navega no mesh real (FRAG\0), morre deixando estado no TickV (SGDB).
2. **H = Mensageiro.** Revogações (`BL/` CRDT), antígenos (proposta), atestação de binário (proposta).
3. **H = Função de Entropia e Custo.** Governa C-Watt (sobre `economy.rs`), regula replicação (fuel + energy_budget).

```
                        [ K³CHJ CORE SYSTEM ]
                                  │
      ┌───────────────────────────┼───────────────────────────┐
      ▼                           ▼                           ▼
[ NEURAL-OS-CORE ]       [ HERMES ENGINES (H) ]      [ MEMÓRIA CAUSAL ]
- Kernel Rust            - Agentes Efêmeros WASM     - TickvLite + NMD1
- EventBus/Mpmc          - mesh LAN real (42069)     - ARQ (ART+BQ+Recall+Doc)
- apps host (futuro)     - C-Watt (futuro)           - CRDT\0 + BL/ + grafo causal
```

---

## 20. Relação com ADRs Existentes

| ADR | Relação |
|---|---|
| **0081** (Malha P2P) | Base real única de transporte/segurança (fases A/B/C/F, todos os protocolos §5.4). Este ADR estende `TaskType` e adiciona apps host. |
| **0063** (SGDB) | Motor de estado canônico; ARQ = "Índices IA" do título. |
| **0088** (Premissa Máxima) | Vetor operacional (P1). |
| **0083/0084/0085** (camada IA) | Inferência real do `aios_ai::infer`; backprop dívida (sem treino no Sonho). |
| **0059** (wasmi) | Runtime de agentes efêmeros (backend A). |
| **0077** (Ring3) | Permanece gated (`TRY_ENTER_RING3=false`). |
| **0086** (Instalação/OTA) | Runner Host app = análogo cross-OS; kernel só cliente. |
| **0045** (Sound) | Percepção Tier 0 real. |
| **0057** (Compute Dispatch) | Dispatch local; mesh usa o padrão (`MW\0`/`EDR\0`). |
| **0078** (slots) | Tiers de SLMs. |

**Guarda:** `tools/check_duplication.py` após qualquer implementação.

---

## 21. Riscos Honestos

| Risco | Severidade | Mitigação |
|---|---|---|
| Mesh 16 nós/u8/LAN é teto de design | média (aceito) | apps host agregam malhas (relay); 16 nós por malha local é suficiente para casa/escritório |
| Payload 64KB limita contexto distribuído | média | dispatch de trabalho pequeno (matmul/expert), não contexto inteiro; `FRAG_MAX_PARTS` ajustável com teste |
| App host (WAN/IoT) é o maior gap de engenharia | alta | crate `mesh_proto` compartilhada; primeiro app host = espelho do kernel em Rust std |
| wasmi sem yield | média | salvar linear memory + `SpscChannel`; medir custo de snapshot |
| Honeypot/antígeno ainda é proposta | média | começar por `BL/` (Fase A, baixo risco) |
| C-Watt sem moeda real | baixa | começa como orçamento estendido; monetização = decisão pós-v2.0 |

---

## 22. Estado e Próximos Passos

**Lifecycle:** `pesquisa` — pós-v2.0 (P10).

**Caminho (cada fase vira ADR própria + IDEA_BANK):**

1. **Fase A (R0, menor risco):** `BL/` revogação CRDT no TickvLite + `TaskType::Revoke` no NoProto + gatekeeper `is_peer_revoked`. Reusa 100% do existente.
2. **Fase B (R2/R3):** agente efêmero local (fila Mpmc/EventBus + wasmi pool) + envelope `Replicate` (energy/ttl/caps), local-first.
3. **Fase C (R0/R3):** transporte P2P de agentes (`TaskType::Replicate` no mesh existente) com gatekeeper determinístico (§8.2).
4. **Fase D (R3/k_ai):** Modo Sonho adversarial estático (quórum + antígenos, sem treino).
5. **Fase E (apps host, pós-v2.0):** crate `mesh_proto` compartilhada + primeiro Runner Host app (Rust std) falando NoProto/42069 + relay WAN.

**Antes de qualquer sprint:** IDEA_BANK (BL/, agente efêmero, Runner Host, C-Watt, mesh_proto) com destino = esta ADR; TECNOLOGIAS.md na adoção; STATE + SESSION por fase.

**Decisões do maintainer:**
1. Hermes (H) atual + Novo Hermes coexistem (recomendado) ou substituição?
2. C-Watt pós-v2.0 (recomendado) ou no gate?
3. Apps host: começar por espelho Rust std do kernel (recomendado) ou priorizar outra camada?

---

## 23. Conclusão

O Novo Hermes constrói sobre o que **já funciona**: o mesh LAN real (ADR-0081 — skills, CRDT, FL, experts, matmul, persona, fail-closed), o SGDB com ARQ (ART+BQ+Recall+Doc), o wasmi com CapGate, o SleepCycle. O que este ADR adiciona é honesto e incremental: revogação `BL/`, lifecycle efêmero, transporte de agentes no protocolo existente, host import `aios_ai::infer`, e — para o mundo além da LAN — **apps host** que falam o mesmo protocolo NoProto e fazem o papel de WAN/IoT que o bare-metal não pode.

> Removidos da v4, por impossibilidade verificada: ZK-STARKs, atestação TPM (sem Quote no chip), tensor sharding de 70B, eBPF no kernel, Wi-Fi Direct/BLE no bare-metal, PeerID libp2p, llama.cpp. O que resta é engenharia real sobre código real — e, como manda a Premissa Máxima, cada desvio desta visão vira análise, pesquisa e registro.
