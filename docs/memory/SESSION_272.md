# SESSION_272 — Revisão profunda boot/K³CHJ com todas as premissas

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-17  
**Premissas aplicadas (todas):** ADR-0088 · emagrecer bin · Agent/Skill-first · Trust `(token,agent,skill)` · HITL Escalate · DeviceRecipe ADR-0056 · honesty (sem fake Ready / sem LLM no T+0) · memorizar (HANR)

## Por que a 271 não bastava

SESSION_271 fechou NIC rank + H1 cedo. A segunda revisão (este pedido) cruzou **todas** as premissas contra o boot real:

| Premissa | Achado |
|----------|--------|
| AIOS observe→plan→act→verify | Storage ainda ATA-first no bin (inverso do StorageBus NVMe>AHCI>USB>ATA). xHCI/HDA martelados sem olhar a árvore. SLIP como se fosse Net gate. |
| Emagrecer | Probe ATA/AHCI/NVMe/USB inline no `main.rs`. |
| Agent/Skill | DeviceTree existia e não virava card `HW_CAPABILITY` no T+0 (só HwDetect depois). Sem unsigned AGENT.md stubs. |
| Trust | Plano de bind sem `(1, boot_observe, plan)`. |
| HITL / DeviceRecipe | Recipe `Escalate` ainda podia entrar no Auto NIC. |
| Honesty Cortex | Comentário “LLM decide HW” com pesos ainda não carregados. |
| Memorize | Plano sumia depois do boot — SGDB não recebia HANR. |
| Não inventar recipe | Sem RECIPE.md stub; Escalate = EventBus, não Auto. |

## Decisão

| Anel | Mudança |
|------|---------|
| `k_nano::boot_bind` | `StorageKind` + rank NVMe>AHCI>USB>ATA; `should_probe_snd` / `should_probe_usb_host`; skip ATA persist se o plano não inclui PIO. |
| `k_nano::storage_probe` | `probe_storage_drivers()` na ordem do plano; `AhciDriver::probe_first()`. Bin só chama. |
| `k_ai::boot_observe` | `observe_and_plan(trust_ok)`: cards PnP (≤16), Escalate → `HEALTH_ISSUE:HITL:recipe_escalate` **sem** Auto NIC/storage; Trust Deny não apaga evidência; `hydrate_memory()` no `sgdb::boot_init`. |
| bin | Trust check; storage/HDA/USB gated no plano; SLIP = DEGRADED + `HEALTH_ISSUE:I5:net:degraded_slip_sandbox`; comentário Cortex honesto. |

## Premissas — como materializam

1. **AIOS:** H1 observa silício → k_ai planeja → R0 executa só o que existe → SelfHeal verifica a mesma árvore.
2. **Emagrecer:** lógica nova em `k_nano`/`k_ai`; bin = wire.
3. **Agent/Skill:** PnP via `build_card` + EventBus, não Agency stub.
4. **Trust:** `(1, boot_observe, plan)`; policy default Observe = transient allow (P05: não auto-grant TotalAccess).
5. **HITL:** Escalate ≠ Auto; maintainer via HEALTH_ISSUE.
6. **DeviceRecipe:** unsigned = None (tabela ainda pode classificar); Escalate explícito bloqueia Auto. Sem fake Ready.
7. **Honesty:** log `tabela+recipe; Cortex sem pesos`. SLIP não é Net gate.
8. **Memorize:** HANR `boot_bind` quando SGDB ready; senão EventBus só (honesto).

## O que NÃO foi feito (honesto)

- `measure_bandwidth` / BMIDE 0xC8 (#513 restante).
- LLM Cortex decidindo bind (pesos ainda no FAT, pós-DriverInit).
- CapGate syscall no T+0 (Trust nativo; CapGate continua nos demos ADR-0041).
- HwDetectAgent `detect_all()` depois ainda pode reemitir cards (duplicata OK).

## Testes

- `cargo test -p k-nano --target-dir target/check-s272-nano boot_bind`
- `cargo test -p k_ai --target-dir target/check-s272-kai boot_observe`
- `cargo check -p neural-kernel --features fat-boot-log --target-dir target/check-s272-nk`

## Lição

“IA desde o boot” não é um CortexAgent construído cedo. É Observe (DeviceTree) → Plan (Trust+recipe+tabela) → Act (probe na ordem) → Verify (SelfHeal na árvore) → Remember (HANR). Martelar ATA/xHCI “porque sempre foi assim” é bypass, mesmo com log.

## Pós-tarefa (2026-08-18)

Checklist GOVERNANCE (`docs/GOVERNANCE.md`) + ritual AGENTS (Aprenda → Memorize → Documente → Versione):

- [x] IDEA #513 🟡 (slice 272; residual measure_bandwidth) · #534 ✅
- [x] ADR-0088 operacionalização + Planos Cursor; INDEX lifecycle coerente (`fazendo`, residual #513)
- [x] TODO #18 🟡 residual explícito; pista ativa = ADR-0088 boot
- [x] STATE verdade operacional atual
- [x] SESSION evidência + limites (esta seção)
- [x] AGENTS.md Current Sprint + lições 271–274; CONTEXT glossário
- [x] TECNOLOGIAS 1.10 DeviceTree bind T+0
- [x] CHANGELOG Unreleased
- [x] `cargo test -p k-nano boot_bind` — 5 PASS
- [x] `cargo test -p k_ai boot_observe` — 3 PASS
- [x] `cargo test -p cortex trinity` — 2 PASS · `cargo test -p hermes runtime_observe` — 1 PASS · `cargo test -p k-nano mhi` — 10 PASS
- [x] `cargo check --release -p neural-kernel --features fat-boot-log --target x86_64-unknown-none` — 0 erros
