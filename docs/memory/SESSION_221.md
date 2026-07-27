# SESSION_221 — 247+ Agents: Dead Code Removal + SKILL.md Pipeline + Hermes Enforcement

**Data:** 2026-07-26
**Propósito:** Revisar a premissa "247+ agentes", remover código morto (AGENCY_SEEDS), converter NativeAgentSeed para SKILL.md, e adicionar guarda Hermes para skill_writer.

## Contexto

O projeto sempre mencionou "247+ agentes" (20 nativos + 147 The Agency + ~80 importados + HW + FS). Na realidade, o runtime tem ~50-60 agentes. Os "147 The Agency" foram zerados (AGENCY_SEEDS = &[]) na migração v1.5 por segurança (ADR-0052: stubs negados sem AGENT.md assinado). Os "~80 importados" nunca existiram.

## O que foi feito

### 1. Auditoria
- `explorer` varreu a codebase: ~46 agent structs reais (não 247+)
- `AGENCY_SEEDS = &[]` — sempre vazio, deliberadamente
- `NATIVE_AGENT_SEEDS` — 41 entradas metadados, consumidas pelo PackageHub
- **247+ era aspiracional, nunca realidade**

### 2. AGENCY_SEEDS removido (dead code)
- `crates/k_ai/src/agency_seed.rs` deletado (13 linhas)
- `agency.rs`: `Agency::new()` agora retorna `Self { divisions: Vec::new() }`
- `package_hub.rs`: loop `AGENCY_SEEDS` removido
- `k_ai/src/lib.rs`: `pub mod agency_seed;` removido
- `export_agent_packages.py`: referências ao agency_seed.rs deletado limpas

### 3. 41 SKILL.md criados para agentes
- `skills/agents/<name>/SKILL.md` — cada agente nativo vira SKILL.md
- Formato frontmatter: name, division, mission, schedule, native_impl, kind, skills
- Fonte da verdade substituta do NativeAgentSeed compile-time

### 4. NativeAgentSeed → SKILL.md pipeline
- `native_agent_seed.rs` reescrito: usa `include_str!` para embutir os 41 SKILL.md
- Função `load_all()` parseia frontmatter em runtime → Vec<AgentSeed>
- `package_hub.rs` agora chama `load_all()` em vez de iterar sobre `NATIVE_AGENT_SEEDS`
- Build verificado: 0 erros

### 5. Docs cleanup (16 arquivos)
- AGENTS.md, README.md, HOWTO.md, TECNOLOGIAS.md, COMMERCIAL.md, SUMMARY.md, ROADMAP.md, CHANGELOG.md
- ADRs: 0036, 0044, 0047, 0062, 0026, sprint-plan-92-100
- "247+ agentes" → "~50 agentes nativos"
- "147 The Agency" e "~80 importados" removidos

### 6. Skill Writer corrigido
- Adicionada regra de auto-consulta obrigatória
- Documentado formato Agent Skill vs Skill de Usuário
- Pipeline de registro para ambos os tipos
- Auditoria reconhece agent skills

### 7. Hermes enforcement (skill_writer)
- `skill_loader.rs`: `SKILL_WRITER_CONTENT` como constante pública
- `cognitive_bridge.rs`: `is_skill_creation_request()` — detecta padrões de criação
- `agents.rs` (HermesAgent): guarda no Chat handler — log obrigatório antes de processar skill creation

## Arquivos modificados

### Criados
- `skills/agents/*/SKILL.md` (41 arquivos)

### Deletados
- `crates/k_ai/src/agency_seed.rs`
- `crates/hermes/src/package_hub.rs.bak`

### Modificados (código)
- `crates/k_ai/src/agency.rs` — Agency::new() vazio
- `crates/k_ai/src/lib.rs` — mod agency_seed removido
- `crates/k_ai/src/native_agent_seed.rs` — reescrito com SKILL.md includes
- `crates/hermes/src/package_hub.rs` — AGENCY_SEEDS loop removido, load_all()
- `crates/hermes/src/skill_loader.rs` — SKILL_WRITER_CONTENT constante
- `crates/hermes/src/cognitive_bridge.rs` — is_skill_creation_request()
- `crates/hermes/src/agents.rs` — skill_writer pre-flight guard
- `tools/export_agent_packages.py` — agency_seed refs limpas

### Modificados (docs)
- AGENTS.md, README.md, HOWTO.md, TECNOLOGIAS.md, COMMERCIAL.md, SUMMARY.md, ROADMAP.md, CHANGELOG.md
- 8 ADRs (0036, 0044, 0047, 0062, 0026, sprint-plan-92-100)

## Lições Aprendidas

1. **"247+" era puramente aspiracional.** Números grandes em README viram dívida técnica quando ninguém audita. Desde o início (~46 agentes), o número nunca passou de ~60.
2. **AGENCY_SEEDS vazio foi decisão consciente (ADR-0052).** O código morto ficou como "vazio proposital" por meses — melhor deletar e deixar o pipeline SKILL.md ser a fonte da verdade.
3. **SKILL.md como fonte da verdade unifica o formato.** Agora tanto skills de usuário quanto agent skills seguem SKILL.md. O `skill_writer` documenta ambos.
4. **Hermes deve enforce rules no sistema, não só documentar.** A guarda `is_skill_creation_request()` + log é enforcement mínimo que garante rastreabilidade.

## Build Verification
- `cargo check --release`: 0 errors (apenas warnings pré-existentes)
