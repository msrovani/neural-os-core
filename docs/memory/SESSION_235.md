# SESSION_235 — Mesh P2P aplicações reais: Marketplace + PROMOTE + Papéis (ADR-0081)

**Data:** 2026-07-31
**Objetivo:** Desbloquear os 3 stubs do mesh (1: marketplace real, 2: PROMOTE_SKILL, 3: propagação de papéis) + item 4 (compute distribuído).

## Resumo

| Item | Antes (stub) | Depois (real) | Validação QEMU |
|---|---|---|---|
| 1 Marketplace | `register_local_skill` nunca chamado → `local_skills` vazio → nada enviado | `activate_global` popula do SKILL_REGISTRY (14 skills, dedupe); throttle por TIMER_TICKS | A broadcast 14 offers (`broadcast skill ... sent=true`); B recebeu `type=4` ModelUpdate |
| 2 PROMOTE | Worker log-only ("PROMOTE pendente") | Worker envia `PROMOTE\0name\0desc` (Sync); Master detecta prefixo e registra DynamicSkill | caminho implementado (gate Master); cenário sem skill nova → "ja existe" |
| 3 Papéis | `assign_roles` computava mas "send role-assignment" era ponytail | `ROLE\0target\0role_u8` (Sync, throttle 110 ticks); receptor filtra por node_id() e aplica `set_role` | A=Master enviou `role-assign node=3 role=Memory`; B aplicou `role aplicado node=3 role=Memory` |
| Fix eleição | lazy-init usava MAC completo vs peers `[source_id,0,..]` → comparação lexicográfica sempre favorecia o peer → **todos Worker** | local usa `[node_id(),0,0,0,0,0]` (mesmo formato dos peers) | A=Master node=2, B=Worker node=3 |

## Problemas encontrados no teste QEMU dual

1. **Eleição Worker+Worker**: `MESH_ENGINE` lazy-init com MAC `[0x52,0x54,...]` vs peer `[3,0,0,0,0,0]` — `3 < 0x52` sempre → todo mundo via o peer "menor" → todos Worker. Fix: local usa `[node_id(), 0,0,0,0,0]`.
2. **Spam de log do marketplace**: `activate_global` logava "marketplace ativo" a cada bei_tick. Fix: log só quando o count muda.
3. **Throttle por CALLS (scheduler rate-limited)**: 200 chamadas demoravam minutos sob TCG (scheduler tick=160 após 2700 timer ticks) → broadcast nunca disparava. Fix: throttle por TIMER_TICKS (como o heartbeat).

## Verificação

- `cargo check --release -p neural-kernel`: 0 erros.
- QEMU dual (-NoDisk): A=Master node=2 (15 skills push + 14 offers broadcast), B=Worker node=3 (RX type=4 + role aplicado Memory), RX cruzado type=5 heartbeats.
- Commit `50bdf6b` (1+2+3), `e4917c1` (fix .data), `9239ac9` (node_id + tie-break).

## Item 4 — Compute distribuído (cortex::compute Worker→Master) ✅

- Feature `p2p` nova no cortex (bloco `#[cfg(feature="p2p")]` do `dispatch_ternary` existia mas nunca compilava).
- Worker: serializa w+x (`MW\0` + shapes u32 LE + packed_data 2-bit + x f32 LE, gate MTU 1200B), envia via `udp_broadcast` (TaskType::Inference), espera síncrona ~200 TIMER_TICKS a resposta `MR\0` (filtro `dest_id == node_id()`); timeout → fallback local.
- Master: `poll_mesh_requests()` drena EventBus P2P_PACKET, responde com `ternary_matmul_adaptive`. Gate "só Master" removido — responde mesmo Undecided (sob TCG o Master pode ainda não ter eleito quando o request chega).
- Self-test `mesh_matmul_self_test()` 16×16 (1107B ≤ MTU) + retry 5x no bei_tick (DIAG do boot roda antes da eleição — role Undecided — nunca pegava o P2P).
- **VALIDADO QEMU dual**: `[B] matmul request node=3 size=1107 sent=true` → `[A] matmul resposta node=3 sent=true` → `[B] matmul resposta node=3 ok shape=(16,16) primeiro=120.0 (mesh dispatch)`. Commit `b6ab13b`.

## Next

- Fechar loop: LLM emitir op-IR → registrar Skill/agent-wasm persistente (ADR-0059 F3→F5); fragmentação MTU p/ matmul grande (assíncrono); fl_trainer.rs + mesh_distrib.rs desbloqueio (mesmo padrão MW/MR).
