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

## Next

- Item 4: cortex::compute distribuído (plano em andamento — serializar w+x via NoProto tensor_len/param_len, round-trip Master, fallback local).
