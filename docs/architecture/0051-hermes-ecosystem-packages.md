# ADR-0051: Hermes Ecosystem Packages — skills / plugins / mcp / agents

**Data:** 2026-07-17  
**Status:** Accepted (MVP)  
**Lifecycle (INDEX):** `fazendo`  
**Relacionadas:** NeuralFS.md §12 (namespace), ADR-0032 (WASM apps), ADR-0031 (assinatura), Sprint 108 (self_evolve)

## Contexto

O AIOS é agentico: Agents decidem, Skills são pastas portáteis, Workflows orquestram.  
Faltava um **hub único** onde Hermes/Cortex saibam *onde*, *o quê*, *como achar*, *qual segurança* e *para que* serve cada pacote — com CRUD e human-in-the-loop.

## Decisão

1. **Namespace FS** — canônico em NeuralFS §12 sob `/mnt/neural/ecosystem/`.
2. **PackageHub** (`hermes::package_hub`) — catálogo + CRUD + verify + HITL.
3. **Assinatura embutida** — frontmatter `signature:` / `content_hash:` (ed25519 via `identity::verify_trusted`). Sem chave → `unsigned` + Escalate.
4. **Hermes** — comandos `/pkg …` + `/approve` `/deny` existentes aplicam pending package ops.
5. **Cortex** — `catalog_for_cortex()` prepend no system prompt (≤2KB).

## PackageKind

| Kind | Path relativo | Uso |
|------|---------------|-----|
| Skill | `skills/<name>/SKILL.md` | Procedimento → SkillLoader |
| Agent | `agents/<name>/AGENT.md` | Manifesto nativo ou SpecialistAgent (Agency) |
| AgentWasm | `agents/<name>/MANIFEST` (+ wasm) | WASM tickável (sandbox) |
| Workflow | `workflows/<id>/WORKFLOW.md` | Fluxo declarativo |
| Plugin | `plugins/<name>/` | Bundle + risk |
| Mcp | `mcp/<name>/` | Tools → USER_INTENT |
| Model | `models/` | Pesos / .bitnet |
| Firmware | `firmware/` | Blobs HW |
| DeviceRecipe | `devices/<name>/RECIPE.md` | LEGO HW bind+UnlockDAG (ADR-0056) |

Alias legado ADR-0032: `/agents/<stem>.wasm` → `ecosystem/agents/<stem>/MANIFEST`.

## Fleet data-driven (SESSION_134 → corrigido SESSION_135 / ADR-0052)

- **SESSION_134** gerou ~255 `AGENT.md` stubs idênticos — **não são artefatos**.
- **SESSION_135 / ADR-0052:** stubs apagados; `AGENCY_SEEDS = &[]`; fleet Agency = **0** até pacotes assinados.
- Contagem honesta: **41 nativos compilados** + N PCI + pacotes PackageHub **validados** (schema+hash+signature+acionaveis).
- Contrato de conteúdo: **ADR-0052** + `.cursor/rules/neural-artifact-spec.mdc`.
- Create/Update sem assinatura → **Deny** (`unsigned_denied`).
- Import (`provenance: imported`) exige `sandbox_status: passed`.
- Hermes pode criar **drafts** (`hermes_draft_md`) — Escalate/HITL; não Auto.
- Implementações nativas (Hermes/Cortex/drivers) **permanecem no bin**; só o manifesto é externalizado.
- `content-creator` duplicado (creative + marketing-imported) → `package_id` com `--division`.

## Residuals

- Assinatura com chave privada de sessão → **fechado** ADR-0053 (`init_session_identity` + `sign_artifact_md`)
- MCP JSON-RPC mínimo (tools/list|call) → ADR-0053; TCP listener pleno ainda aberto
- **Não** seedar árvore nested no `mkexfat` (disco de dados é flat root; PackageHub fala só com NeuralFS)
- Persistência NeuralFS GPT dedicada (residual NeuralFS)
- AgentWasm → AgentScheduler pleno (CapGate) — fora desta leva

## Critérios MVP

- [x] ADR + NeuralFS §12
- [x] `package_hub` + seed skills pasta
- [x] `/pkg` + approve path
- [x] catalog no Cortex prompt
- [x] `cargo check --release` 0 erros (gate de sessão)
- [x] kinds Agent + Workflow
- [x] VFS bridge Hermes → neural-kernel FS_AGENTS
- [x] Agency/nativos no catálogo — nativos seed OK; Agency seed vazio até assinados (ADR-0052)
- [x] Persistência honesta: árvore `ecosystem/` no mount + VFS bridge; nested mkexfat = residual
- [x] Contrato ADR-0052 + deny unsigned (SESSION_135)

---

## Planos Cursor implementados

### `Ecosystem Package Hub` (`ecosystem_package_hub`)

| Todo | Status | Evidência |
|------|--------|-----------|
| NeuralFS §12 + ADR-0051 + INDEX | ✅ | esta ADR + NeuralFS.md |
| `hermes::package_hub` CRUD+sig+HITL+catalog | ✅ | `package_hub.rs` |
| Migrar `skills/` → pastas `SKILL.md` | ✅ | `skills/*/SKILL.md` |
| `/pkg` + Approve path + hw_pnp promote | ✅ | hermes shell |
| `catalog_for_cortex` no system prompt | ✅ | cortex wire |
| cargo check + SESSION/CHANGELOG | ✅ | SESSION_134–135 |

### `Migrar agentes NeuralFS` (`migrar_agentes_neuralfs`)

| Todo | Status | Nota |
|------|--------|------|
| VFS unify PackageHub↔boot | ✅ parcial | bridge Hermes → `neural-kernel` FS |
| `export_agent_packages.py` + seeds | ✅ → **corrigido** | SESSION_134 gerou stubs; SESSION_135/ADR-0052 **apagou** stubs idênticos |
| Agency data-driven | ✅ | `AGENCY_SEEDS = &[]` até assinados; nativos no bin |
| kinds Agent + Workflow | ✅ | PackageHub |
| 41 nativos catalog `native:true` | ✅ | `native_agent_seed` |
| nested mkexfat seed | ❌ residual | disco flat; não mentir `persisted` |

Contrato de conteúdo: **ADR-0052**. Trust/marketplace: **ADR-0053**.
