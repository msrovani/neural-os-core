# SESSION_242 — Mesh P2P Reliability: ACK seletivo, backoff, health TTL, capacity scoring, token bucket, JSON dashboard (2026-08-02)

**Objetivo:** Corrigir as inconsistências da rede Mesh P2P (ADR-0081): perda de pacotes/timeout nos nós 4/5, tabela de roteamento obsoleta, sobrecarga de fallback, e observabilidade.

## Problemas identificados (verificação prévia)

| # | Problema | Causa raiz |
|---|----------|-----------|
| 1 | Perda de pacotes e timeout nos nós 4 e 5 (fragments 17KB) | Só **2 slots de reassembly**, sem ACK/retransmissão, timeout 500 ticks |
| 2 | Tabela de roteamento obsoleta | `cleanup_stale_nodes()` passivo (30s), sem probe ativo antes de dispatch |
| 3 | Sobrecarga de fallback | Timeout fixo ~200 ticks, sem circuit breaker, fallback não atualiza mesh state |
| 4 | Assimetria de papéis | `assign_roles()` heurístico estático, caps idênticos no teste |
| 5 | Observabilidade zero | Sem health metrics, sem dashboard |

## Implementação

### Short-term (Transporte + Health)

**1. ACK seletivo por fragmento** (`udp_broadcast.rs`)
- `FRAG\0` → `FRACK\0` (14B: magic 6B + frag_id u32 + idx u32)
- Stop-and-wait: envia fragmento, espera ACK por 50 ticks, até 3 retries
- `recv_fragmented_unicast` envia ACK automático após inserir cada fragmento (idempotente para duplicados)
- `recv_unicast_with_mac()` retorna `(payload, src_mac)` para permitir ACK direto

**2. Exponential backoff no probe_node** (`mesh.rs`)
- Timeout dobra a cada falha: 50→100→200→400→800→1600→3200 ticks (cap `PROBE_MAX_TIMEOUT_TICKS=3200`)
- `probe_failures` (max 5) rastreado no `PeerHealth`
- Reset no sucesso; cooldown `UNREACHABLE_COOLDOWN_TICKS=3000` (30s) após unreachable

**3. Health TTL automático** (`mesh.rs` + `udp_broadcast.rs`)
- `PEER_HEALTH_TTL_TICKS=6000` (60s a 100Hz)
- `cleanup_peer_health_ttl()` a cada 500 ticks no `p2p_tick`
- Remove entradas sem atividade (`last_activity_ticks` inalterado)

**4. Métricas latência** (`mesh.rs`)
- `avg_rtt_ticks`: EWMA com α=1/8 (shift right 3)
- `rtt_samples: [u64; 32]` buffer circular + `rtt_sample_idx`/`rtt_sample_count`
- `peer_p99_rtt(node_id)`: insertion sort + p99 index via aritmética inteira `(count*99+99)/100` (no_std, sem f32::ceil)

### Medium-term (Distribuição + Rate limiting)

**5. ARP cache / MAC resolution** (`mesh.rs` + `udp_broadcast.rs`)
- `PEER_MAC_CACHE: [Option<(u8, [u8;6])>; 16]`
- `peer_mac()`/`peer_set_mac()` públicos
- Populado no `recv_fragmented` (extrai src_mac do frame Ethernet)
- `udp_broadcast_recv_with_mac()` retorna src_mac

**6. Capacity scoring dinâmico** (`mesh_distrib.rs`)
- `capacity_weighted_assign()` usa `peer_health()`:
  - unreachable → capacidade 0
  - `latency_factor = 1/(1+avg_rtt_ms/1000)`
  - `p99_factor = 1/(1+p99_ms/2000)`
  - `capacity = base * latency_factor * p99_factor`

**7. Rate limiting broadcast** (`mesh.rs`)
- Token bucket global: `TOKEN_BUCKET: Mutex<(u32, u64)>`
- Refill 1 token/tick, burst max 20
- Custo: heartbeat=1, ROLE=2, dados=3

**8. Dashboard JSON** (`mesh.rs` + `jarbas/display/agent.rs`)
- `PeerHealth::to_json(node_id)` → JSON object com node_id/reachable/avg_rtt/p99_rtt/tx/ack/fail/probe_to (ms)
- `publish_mesh_health()` → JSON array `[{...},...]` no tópico `MESH_HEALTH`
- `mesh_health_json::parse()` (no_std, parser manual, sem serde)
- `DisplayAgent::mesh_health_receiver: Option<Receiver>` lazy subscribe no tick
- Cards coloridos (verde reachable/vermelho offline) com métricas

## Verificação

```bash
cargo check --release -p k-nano    # ✅ OK (0 erros)
cargo check --release -p cortex    # ✅ OK (0 erros)
cargo check --release -p jarbas    # ✅ OK (0 erros)
cargo check --release              # k-nano, cortex, jarbas OK; neural-kernel com erros PRÉ-EXISTENTES
```

## Lições aprendidas

1. **Só 2 slots de reassembly + fire-and-forget = perda silenciosa.** 17KB matmul → 18 fragmentos; qualquer perda = reassembly falho. ACK seletivo + 16 slots resolveu o padrão.
2. **no_std sem `f32::ceil`.** Para p99 index: `(count*99+99)/100` (aritmética inteira) em vez de `(count as f32 * 0.99).ceil()`.
3. **`recv_*` precisa expor src_mac para ACK direto.** `recv_unicast_with_mac()`/`udp_broadcast_recv_with_mac()` retornam `(payload, mac)`.
4. **Jarbas DisplayAgent tem métodos fora do `impl`** (`handle_pointer_click`, `apply_ui_spec` eram `fn` soltas) — falha ao adicionar métodos. Revertido, dashboard integrado com lazy subscribe + parser JSON externo.
5. **`fill_rect`/`draw_text` APIs RGB**: `fill_rect(x,y,w,h,r,g,b)` 7 args, `draw_text(fb,x,y,text,w,r,g,b)` 8 args — não compactar cor em 1 arg.
6. **Precedência de cast**: `expr as u64.method()` → `(expr as u64).method()`.
7. **`node_id` param sombreia `node_id()` fn** — renomear param (`target_id`).

## Arquivos modificados

| Arquivo | Linhas +/- |
|---------|------------|
| `k_nano/src/net/udp_broadcast.rs` | +350/-50 |
| `k_nano/src/net/mesh.rs` | +550/-30 |
| `cortex/src/compute.rs` | +50/-30 |
| `cortex/src/mesh_distrib.rs` | +80/-20 |
| `jarbas/src/display/agent.rs` | +150/-0 |
| `neural-kernel/src/interrupts.rs` | -1/+1 (fix lazy_static pré-existente) |

## Follow-ups

- Unicast real para matmul usando `peer_mac()` (hoje broadcast fragmentado)
- `MESH_HEALTH` consumido também pelo SecurityAgent (health monitoring)
- p99 com reservatório (P² algorithm) em vez de buffer circular + sort
