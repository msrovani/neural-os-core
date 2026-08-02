# SESSION_239 — Fase C ADR-0081: experts distribuídos + DSD + NodeTier + FL federado + CRDT (2026-08-01)

**Objetivo:** Implementar a Fase C da ADR-0081 (computação distribuída/CRDT/federated/SKYNET) sobre o transporte P2P já validado (Fase A segurança + FRAG\0).

## Resumo

| Item | Componente | Protocolo | Status |
|---|---|---|---|
| C2 Experts distribuídos | `cortex/mesh_distrib.rs` | `ED\0` (Worker→Master lista experts) / `EDR\0` (Master→Worker assign ponderado) | ✅ |
| DSD SpeculativeDecoder | `cortex/speculative.rs` (novo, ~150 LOC) | draft_verify local (self-test accepted=8), stats, mesh_tick | ✅ |
| NodeTier SKYNET #315.27 | `k_nano/net/mesh.rs` | `NodeTier L0-L4` + `score_bonus` (1.0-3.0) no capacity_score; `NodeCapabilities::new_tiered` (new delega L1) | ✅ |
| C5 FL federado #312f | `k_ai/fl_trainer.rs` | `FD\0` gradiente (packing 2-bit LSB-first) / `FM\0` modelo global; Master agrega FedYogi + broadcast | ✅ |
| C4 CRDT sync #315.26 | `k_ai/sgdb/crdt_sync.rs` | `CRDT\0` version sync; LWW merge (maior version vence); peer_versions | ✅ |

Wiring em `bei_init.rs` bei_tick: `poll_expert_requests` + `dsd_tick` + `broadcast_local_experts` (1x Worker) + `crdt_sync_global` + `mesh_tick_global` (FL).

## Padrão P2P (copiado de compute.rs, validado)

- Emissor: serializa payload binário (prefixo ASCII), `sign_packet`, `send_fragmented(&signed, 42069)`.
- Receptor (Master): subscribe lazy no EventBus `P2P_PACKET` (static `Mutex<Option<Receiver>>`), drena `try_receive`, parse, responde com dest_id = source.
- Worker wait loop: `recv_fragmented(42069)` + parse + verify + filtro dest_id == node_id().
- TaskType::Inference (1) para FD\0/FM\0/CRDT\0/ED\0/EDR\0 — evita colisão com skill_sync (Sync=3) e marketplace (ModelUpdate=4).
- Fase A preservada: k_nano p2p_tick verifica assinatura fail-closed no ingress ANTES de publicar no EventBus — consumidores recebem payload já verificado.

## Validação QEMU dual

- A=Master: `CRDT sync iniciado (role=Master)` + `publish v=0 peers=0 sent=true`; `FL stats fl round=0 global=0 grads=0`; `mesh role=Master`.
- B=Worker: `CRDT sync iniciado (role=Worker)` + `publish v=0 sent=true`; matmul 64×64 fragmentado round-trip `shape=(64,64) primeiro=2016.0 (mesh dispatch)` (FRAG\0 18/17 partes).
- `cargo check --release`: 0 erros.

## Lições

- **TaskType importa**: usar Inference para tráfego novo evita que skill_sync (Sync=3) e marketplace (ModelUpdate=4) façam parse indevido de payloads FD\0/CRDT\0.
- **Dois fixers concorrentes em bei_init.rs**: a sobreposição gerou `}` fora de lugar — detectado no cargo check e corrigido. Validar a árvore integrada ANTES do commit quando lanes paralelas tocam o mesmo arquivo de wiring.
- **FL com weights zerados**: `local_weights` = 0 → nenhum gradiente enviado (gate honesto). O canal está ativo (stats rodando); gradientes reais chegam quando houver treino.

## Pendente ADR-0081

- Fase B cripto (X25519+ChaCha20) — tráfego sensível em rede não-isolada
- SemanticRouter (HNSW) — recomendação médio prazo
- Merkle piece verification — distribuição de modelos
- Merge de conteúdo CRDT (ART/BQ) — hoje sync de versão (ponytail)

## Verificação

- `cargo check --release -p neural-kernel`: 0 erros (árvore integrada após lanes paralelas).
- QEMU dual: CRDT publish bilateral + FL stats + matmul 64×64 fragmentado.
- Commit `866e0e6` (7 arquivos, +1099 LOC).
