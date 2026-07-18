# SESSION_134 — Agency / nativos → NeuralFS PackageHub

**Data:** 2026-07-17  
**Pista:** ADR-0051 + fleet data-driven (manifestos externos; código nativo permanece)

## Objetivo

Externalizar os **255** agentes fixos (214 Agency + 41 nativos) para `ecosystem/agents/*/AGENT.md`, mantendo implementações boot/IRQ/HAL no binário.

## Decisões (exploradores)

| Fonte | Decisão |
|-------|---------|
| [Mapear imagem e documentação](d9c8d850-a012-42f1-b9e4-b45fc9debed5) | **Não** seedar nested no `mkexfat` (flat root + reader só lista root). Seed = embutido + mkdir NeuralFS. |
| [Mapear VFS e PackageHub](0a6858b3-c6d1-4779-ad7d-ca308e6b4c04) | Bridge VFS: Hermes callbacks → `neural-kernel::fs` após `init_fs_agents`. |
| [Mapear Agency externalizada](82731c63-91f6-434d-a6a7-3445d59086d4) | 214 specs; `content-creator` dup; kind `Agent` ≠ `AgentWasm`. |

## Implementado

- `tools/export_agent_packages.py` + `tools/native_agents.toml`
- `k_ai::{agency_seed,native_agent_seed}` (gerados) + `Agency::from_specs`
- Ponte `neural-kernel::{agency,agency_importer}` → `k_ai`
- `PackageKind::{Agent,Workflow}` + `agency_specs()` + seed agents no hub
- `hermes::globals::install_vfs_bridge` + wire no boot
- `NeuralFsAgent::ensure_ecosystem_tree` + `storage_path` (≤22 chars)
- Docs: ADR-0051, NeuralFS §12

## Contagens

| Grupo | N | No scheduler? |
|-------|--:|:-------------:|
| Agency | 214 | sim |
| Nativos | 41 | sim |
| Fixos | 255 | — |
| HW PCI | N | sim |
| FS VFS | 8 | não |

## Verificação

- [x] `py -3 tools/export_agent_packages.py` → 255 AGENT.md (agency_slots=214, unique=213, native=41)
- [x] `cargo check --release -p k_ai -p hermes -p neural-kernel` = 0 erros (`target/check-agency`)
- [ ] Boot: `[NEURALFS] ecosystem/ tree ready` + `[PKG] seed agents agency=214 native=41` + `[AGENCY] 214`

## Residual

- Nested `ecosystem/` no disco exFAT = bloqueado até list subdir + writer de dirs
- AgentWasm → AgentScheduler pleno
- Drift docs “147 Agency / 20 nativos” → corrigir STATE/TECNOLOGIAS/AGENTS.md
