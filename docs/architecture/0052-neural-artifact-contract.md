# ADR-0052 — Neural Artifact Contract

**Status:** Accepted (MVP)
**Lifecycle:** `fazendo`
**Data:** 2026-07-17
**Ideias:** PackageHub / Agency honesty / Hermes create / import sandbox
**Relacionadas:** ADR-0051 (PackageHub), ADR-0032 (WASM apps), ADR-0041 (CapGate), ADR-0056 (DeviceRecipe), NeuralFS §12

## Contexto

A SESSION_134 exportou ~255 `AGENT.md` quase idênticos a partir de tabelas Agency.
Isso **não** são artefatos Neural OS: faltam goal real, contexto, `acionaveis`, hash e assinatura Ed25519.
Stub não entra no fleet. Contagem honesta = nativos compilados + pacotes **validados**.

## Decisão

1. Todo artefato de ecossistema (`skill`, `agent`, `agent-wasm`, `workflow`, `plugin`, `mcp`, `model`, `firmware`, `device-recipe`) obedece ao **Neural Artifact Spec** (`.cursor/rules/neural-artifact-spec.mdc`). DeviceRecipe: seções UnlockDAG + path `ecosystem/devices/` (ADR-0056).
2. `PackageHub::validate` é **deny-by-default**: sem schema / campos obrigatórios / seções / `content_hash` / `signature` trusted → **Reject**.
3. Campo obrigatório **`acionaveis`**: `init | oneshot | continuous | event_driven | poll_every:N | on_demand`.
4. Hermes pode **criar** drafts (`provenance: hermes_created`); draft sem assinatura não ativa Auto nem seed de fleet.
5. Import externo (`provenance: imported`) exige `sandbox_status: passed` (sandbox CapGate/WASM SFI) **antes** do catálogo ativo.
6. Nativos Ring0/IRQ/HAL/Cortex/Hermes permanecem no bin (`provenance: native_compiled` = catálogo, não substitui código).
7. Seeds Agency stub e árvore `ecosystem/agents/*` gerada em massa → **apagados**. `AGENCY_SEEDS = &[]` até haver pacotes reais assinados.

## Schema mínimo (frontmatter)

```yaml
schema: 1
kind: skill|agent|agent-wasm|workflow|plugin|mcp|model|firmware|device-recipe
name: <id>
goal: <mensurável>
contexto: <papel no AIOS>
acionaveis: [...]
required_tokens: [...]
provenance: hermes_created|imported|native_compiled
sandbox_status: none|pending|passed|failed
content_hash: "<fnv1a64 hex16 do corpo canônico>"
signature: "<ed25519 hex128>"
```

Corpo canônico = manifesto sem linhas `content_hash:` / `signature:`.
Seções Markdown: Contexto, Goal, Acionaveis, Workflow, Pre-Flight, Success Criteria, Failure Policy.

## Consequências

- Fleet Agency pode ser **0** até pacotes reais — honesto.
- ADR-0051 permanece PackageHub/VFS; este ADR define o **contrato de conteúdo**.
- Skill generation (Sprint 108) deve evoluir para emitir o schema 1 (residual).
- Ferramenta `export_agent_packages.py` não regenera stubs em massa.

## Residuals

- [ ] Assinatura de pacotes de referência (chave maintainer) no CI/host tool
- [ ] Sandbox militar completo para import (hoje: gate de campo + CapGate WASM parcial)
- [ ] `verify_skill_md` unificado com `verify_artifact_md` (schema 1)
- [ ] Promoção Hermes draft → `agent-wasm` via SkillOpt/evolve

---

## Planos Cursor relacionados (implementados / corrigidos)

| Plano | Papel nesta ADR | Status |
|-------|-----------------|--------|
| `Ecosystem Package Hub` | Hub + namespace; gerou pressão por AGENT.md | ✅ MVP |
| `Migrar agentes NeuralFS` | Export em massa SESSION_134 | ✅ código → **corrigido** por esta ADR (stubs ≠ artefato) |
| `HANR Hermes Port` Wave 0 | Session Ed25519 + auto-sign drafts | ✅ ADR-0053 |

**Decisão canônica:** inventário copiado sem missão executável **não** é artefato; não registrar no fleet.
