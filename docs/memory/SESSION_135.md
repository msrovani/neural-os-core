# SESSION_135 — ADR-0052 Neural Artifact Contract

**Data:** 2026-07-17  
**Foco:** Contrato de criação/validação de artefatos; remoção de stubs Agency; deny sem hash/assinatura.

## Veredito

Stubs `ecosystem/agents/*/AGENT.md` da SESSION_134 **não eram artefatos** (boilerplate idêntico, sem signature, SpecialistAgent só anuncia skills). Removidos. Contrato canônico em ADR-0052 + regra Cursor.

## Feito

1. **`.cursor/rules/neural-artifact-spec.mdc`** (`alwaysApply`) — schema, seções, `acionaveis`, provenance, sandbox, deny-by-default.
2. **`docs/architecture/0052-neural-artifact-contract.md`** — decisão formal.
3. **Apagados** `ecosystem/agents/**` stubs + `INVENTORY.json`.
4. **`AGENCY_SEEDS = &[]`** — `register_agency_agents` loga `[AGENCY] 0`.
5. **PackageHub** — `verify_artifact_md` + `validate` exige schema/goal/contexto/acionaveis/hash/signature; import exige `sandbox_status: passed`; Create/Update unsigned → `unsigned_denied` / Deny.
6. **`hermes_draft_md`** — Hermes pode gerar drafts (sem Auto).
7. **`export_agent_packages.py`** — default deny; `--legacy-count` / `--refresh-native-seed` apenas.

## Contagem honesta

| Grupo | Count | No fleet? |
|-------|------:|:---------:|
| Nativos compilados | 41 | Sim |
| Agency stub | 0 | — |
| AGENT.md agency assinado | 0 (hoje) | Só se signed |
| HW PCI | N | Sim |

## Residuals

- Assinar pacotes de referência (host tool + chave trusted)
- Sandbox militar completo para import (campo gate hoje)
- Unificar `verify_skill_md` Sprint 108 com schema 1
- Nativos no PackageHub ainda unsigned (catálogo `native_compiled` — bin = verdade)

## Check

`cargo clean -p neural-kernel && cargo check --release -p k_ai -p hermes -p neural-kernel` (target isolado).
