# SESSION_307 — SMP AIOS N-cores (roles dinâmicos + runqueue)

**Data:** 2026-09-03 | **Sprint:** v1.9.99-s307 TEST | **Status:** 🔄 wired (aceite metal residual)

---

## Objetivo

Remover hardcode de papéis/índices (anti-AIOS SESSION_279/ADR-0088) e ligar
distribuição real de agents (ADR-0089): MADT → pools P/E → roles ∝ N →
`ap_pollable` → runqueue + steal.

## Decisões

| Item | Valor |
|------|--------|
| Papéis | `init_roles_from_pools(N)` — sem `core3=Memory` |
| `MAX_CORES` RQ | **256** (bound array ≈ LAPIC; não teto de produto) |
| Feature | `smp-runqueue` default ON no bin; `ap-pollable` feature **não** força — barreira IDT seta runtime |
| Affinity | Input/Display/HwBridge/Security=0; Cortex=1; Hermes/Net=2 |
| Redox | Percpu+RQ+steal+IPI; **sem** EEVDF |

## Wire

- `k_nano::smp::runqueue`: roles, steal+affinity, `distribute_batch`, `try_run_one_agent`
- `init_smp` pós-wake: roles + `online==madt` slog + hybrid 0x1A honesty
- `agent-core`: offload hooks + tick-by-index + spinlock BSP↔AP
- `ap_idle_loop`: consome RQ quando `ap_pollable`
- IDEA `#492`/`#324` → fazendo

## Aceite

- [x] Roles N=2 sem Memory mágico; N=4 com 1 Memory (testes host)
- [x] `cargo check` 0 erros (target isolado)
- [ ] QEMU `-smp 2/4/8` NoDisk: grepar `roles n=` / `ap_pollable=true` / `runqueue:`
- [ ] Metal: `online==madt-1` + hybrid R1=P R2=E (K23)

## Fora de escopo

EEVDF; TCG `max_aps=4` env gate intacto; preempt IPI pleno (0065 Fase 4).
