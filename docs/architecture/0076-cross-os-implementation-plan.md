# Plano de Implementação — ADR-0076 Cross-OS Ecosystem

**Data:** 2026-07-26
**Status:** Proposed — aguardando autorização para execução
**Estimativa:** 7 fases · ~6.500 LOC · 4-8 semanas · 2 engenheiros simultâneos
**Base:** ADR-0076 + padrões FYY/Wetware/WeftOS/Oreulius/WAeasi

---

## Estrutura de Dependências

```
F1 (Skill Manifest) ─── independente
  │
F2 (Membrane) ───────── independente
  │
F3 (Discoverer) ─────── depende de F1 (formato das skills)
  │
F4 (JAIL) ───────────── depende de F2 (Membrane)
  │
F5 (WASI) ───────────── depende de F1 (formato)
  │
F6 (Interop FYY) ────── depende de F3 (Discoverer)
  │
F7 (Aprendizado) ────── depende de F3 + F4 + F5 (ciclo completo)
```

**Paralelizável:** F1 + F2 (independentes) · F3 + F5 (independentes após F1)

---

## FASE 1: Skill Manifest Schema — Compatibilidade FYY

**Inspiração:** FYY Skill Manifest spec + Anthropic Agent Skills
**Prioridade:** 🔴 Alta · **LOC:** ~800 · **Dias:** 3-5

### Dependências
- Nenhuma (pode começar imediatamente)

### Subitens

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 1.1 | Definir Schema JSON para skill manifest (skill.json) | `docs/specs/skill-manifest-schema.json` | Schema draft-07 validado contra exemplos |
| 1.2 | Implementar SkillManifest struct em Rust (serde/parse) | `crates/hermes/src/skill_manifest.rs` | `cargo check -p hermes` |
| 1.3 | Implementar `validate()` — verifica schema, permissões, risk_level | `crates/hermes/src/skill_manifest.rs` | validate() retorna Ok/Err |
| 1.4 | Implementar `from_skill_md()` — converte SKILL.md → SkillManifest | `crates/hermes/src/skill_manifest.rs` | Roundtrip idempotente |
| 1.5 | Integrar com PackageHub — novo campo `manifest` no PackageRecord | `crates/hermes/src/package_hub.rs` | Toda skill tem manifest |
| 1.6 | demo() — criar skill manifest, serializar, validar, carregar | `crates/hermes/src/skill_manifest.rs` | 3 skills exemplo OK |
| 1.7 | Adicionar `pub mod skill_manifest;` ao lib.rs | `crates/hermes/src/lib.rs` | `cargo check` |

### Skill Manifest Schema (v1)

```json
{
  "name": "string (obrigatório)",
  "version": "string (obrigatório, semver)",
  "type": "wasm | wasi | legacy | mcp",
  "description": "string",
  "when_to_use": ["array de strings — gatilhos de intenção"],
  "risk_level": "low | medium | high | critical",
  "permissions": {
    "filesystem": { "allow": ["glob"], "deny": ["glob"] },
    "network": "none | allow | proxy",
    "hardware": "none | display | audio"
  },
  "capabilities": ["array de strings — nomes das Capabilities"],
  "resource_limits": {
    "fuel_max": "u64 — instruções máximas",
    "heap_max": "u64 — bytes máximos",
    "timeout_ms": "u64 — timeout em ms"
  },
  "interop": {
    "mcp": "bool",
    "fyy": "bool",
    "agent_skills": "bool"
  }
}
```

---

## FASE 2: Membrane + CapGate — Zero Ambient Authority

**Inspiração:** Wetware (membrane grafts) + WeftOS (RBAC)
**Prioridade:** 🔴 Alta · **LOC:** ~1.000 · **Dias:** 4-7

### Dependências
- CapGate (ADR-0041 PoC) — já existe como conceito, precisa ser estendido

### Subitens

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 2.1 | Implementar `Membrane` struct com fs_allow, fs_deny, net_allow, capabilities, fuel_budget, heap_max, timeout_ms | `crates/hermes/src/membrane.rs` | Membrana criada com for_legacy() |
| 2.2 | Implementar `Membrane::check(&self, operation: &Operation) -> Result` | `crates/hermes/src/membrane.rs` | Allow/Deny funciona |
| 2.3 | Implementar `Operation` enum — FileRead(path), FileWrite(path), NetConnect(host,port), CapabilityUse(name) | `crates/hermes/src/membrane.rs` | 4 operações tipadas |
| 2.4 | Implementar `Capability` enum — lista de capacidades conhecidas (VfsRead, VfsWrite, NetTcp, DisplayFb, AudioPlay, etc.) | `crates/hermes/src/membrane.rs` | Enum com 10+ variantes |
| 2.5 | Integrar com CapGate (k_hal unlock_dag) — `Membrane.check()` delega ao CapGate para operações de HW | `crates/hermes/src/membrane.rs` → `k_hal/src/unlock_dag.rs` | CapToken verificado |
| 2.6 | demo() — criar membrana, testar allow/deny para cada Operation | `crates/hermes/src/membrane.rs` | Todas as combinações OK |
| 2.7 | Registrar no lib.rs | `crates/hermes/src/lib.rs` | `cargo check` |

### Membrane Design

```rust
pub struct Membrane {
    pub name: String,
    pub fs_allow: Vec<GlobPattern>,
    pub fs_deny: Vec<GlobPattern>,
    pub net_allow: Vec<String>,
    pub capabilities: Vec<Capability>,
    pub fuel_budget: u64,
    pub heap_max: usize,
    pub timeout_ms: u64,
}

pub enum Capability {
    VfsRead,
    VfsWrite,
    NetTcp,
    DisplayFb,
    AudioPlay,
    AudioCapture,
    UsbAccess,
    GpuCompute,
    RawMmio,     // NUNCA para apps legacy
    PortIo,      // NUNCA para apps legacy
}

pub enum Operation {
    FileRead(String),
    FileWrite(String),
    NetConnect(String, u16),
    CapabilityUse(Capability),
}
```

---

## FASE 3: Discoverer Multi-Fonte — Busca em Runtime

**Inspiração:** FYY (mesa de skills) + WeftOS (descoberta mesh)
**Prioridade:** 🟡 Média · **LOC:** ~800 · **Dias:** 4-6

### Dependências
- FASE 1 (Skill Manifest) — formato de retorno das skills

### Subitens

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 3.1 | Estender CrossOsDiscoverer com busca real no PackageHub via `hub.list()` | `crates/hermes/src/cross_os/discoverer.rs` | Skills locais encontradas |
| 3.2 | Implementar busca HTTP em marketplaces externos (GitHub API) | `crates/hermes/src/cross_os/discoverer.rs` | GET api.github.com/search/... |
| 3.3 | Implementar busca via MCP — consulta FYY/WeftOS usando MCP Server existente | `crates/hermes/src/cross_os/discoverer.rs` → `crates/hermes/src/mcp_server.rs` | MCP request/response |
| 3.4 | Implementar merge + dedup + rank por confiança | `crates/hermes/src/cross_os/discoverer.rs` | Sem duplicatas, ordenado |
| 3.5 | demo() — buscar "planilha excel", retorna 3+ candidatos | `crates/hermes/src/cross_os/discoverer.rs` | Busca funcional |
| 3.6 | Atualizar CrossOsAgent para usar o novo discoverer | `crates/hermes/src/cross_os/agent.rs` | Fluxo completo |

---

## FASE 4: JAIL — Sandbox com Membrane + Audit

**Inspiração:** Wetware (zero ambient authority) + WeftOS (ExoChain)
**Prioridade:** 🔴 Alta · **LOC:** ~1.500 · **Dias:** 6-10

### Dependências
- FASE 2 (Membrane) — base do isolamento

### Subitens

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 4.1 | Criar `Jail` struct com Membrane + state | `crates/hermes/src/jail.rs` | Jail criada |
| 4.2 | Implementar `Jail::exec(wasm_bytes)` — executa WASM em sandbox wasmi com fuel + mem limit impostos pela Membrane | `crates/hermes/src/jail.rs` → `crates/hermes/src/wasmi_rt.rs` | Fuel para no limite |
| 4.3 | Implementar `Jail::exec_legacy(pe_bytes)` — carrega PE em Ring3 UVM com Membrane | `crates/hermes/src/jail.rs` → `crates/hermes/src/elf_loader.rs` | App legacy roda isolado |
| 4.4 | Implementar interceptação de syscall — cada operação passa por `Membrane::check()` antes de executar | `crates/hermes/src/jail.rs` | FileWrite("/etc/shadow") → denied |
| 4.5 | Integrar MerkleAuditTrail — cada operação executada/negada vira entrada no audit trail | `crates/hermes/src/jail.rs` → `crates/k_ai/src/merkle_audit.rs` | verify_chain() PASS |
| 4.6 | demo() — executar WASM em JAIL, tentar operação negada, verificar audit | `crates/hermes/src/jail.rs` | Audit trail mostra block |
| 4.7 | Registrar módulo no lib.rs | `crates/hermes/src/lib.rs` | `cargo check` |

---

## FASE 5: WASI Preview 2 no wasmi_rt

**Inspiração:** WAeasi (WASI Preview 2 como syscall) + Oreulius (host ABI)
**Prioridade:** 🟡 Média · **LOC:** ~1.000 · **Dias:** 5-8

### Dependências
- wasmi_rt existente (ADR-0059)

### Subitens

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 5.1 | Mapear funções WASI Preview 2 para nosso runtime | `crates/hermes/src/wasi_mapping.md` | Documento de referência |
| 5.2 | Implementar `fd_read`, `fd_write`, `fd_close` — I/O básico via nossa VFS | `crates/hermes/src/wasmi_rt.rs` | echo WASM funciona |
| 5.3 | Implementar `path_open`, `path_read`, `path_write` — acesso a filesystem com verificação de Membrane | `crates/hermes/src/wasmi_rt.rs` | Arquivo lido/escrito |
| 5.4 | Implementar `clock_time_get`, `poll_oneoff` — tempo e espera | `crates/hermes/src/wasmi_rt.rs` | clock WASM OK |
| 5.5 | demo() — compilar hello.wasm com wasm32-wasi e executar no wasmi_rt | `crates/hermes/src/wasmi_rt.rs` | "Hello from WASI!" |
| 5.6 | demo() — skill WASM real (office2pdf) rodando via WASI | `crates/hermes/src/wasmi_rt.rs` | DOCX → PDF funcional |

---

## FASE 6: Interop FYY via MCP — Marketplace Externo

**Inspiração:** FYY (mesa P2P) + MCP Protocol
**Prioridade:** 🟡 Média · **LOC:** ~500 · **Dias:** 3-5

### Dependências
- FASE 3 (Discoverer com MCP)

### Subitens

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 6.1 | Estender MCP Server com métodos `skills/search`, `skills/install`, `skills/publish` | `crates/hermes/src/mcp_server.rs` | MCP client consulta |
| 6.2 | Implementar `FyyGateway` — conecta ao mesh FYY via MCP bridge | `crates/hermes/src/cross_os/fyy_gateway.rs` | Skill search retorna resultados |
| 6.3 | Implementar `import_skill_from_fyy(skill_id)` — baixa skill, scaneia (PluginHub), registra (PackageHub) | `crates/hermes/src/cross_os/fyy_gateway.rs` | Skill importada e funcional |
| 6.4 | demo() — buscar "weather" no FYY mesh, instalar, executar | `crates/hermes/src/cross_os/fyy_gateway.rs` | Ciclo completo OK |

---

## FASE 7: Ciclo de Aprendizado Completo — Auto-Evolução

**Inspiração:** WorkflowLearner existente + WeftOS cognitive tick
**Prioridade:** 🟡 Média · **LOC:** ~800 · **Dias:** 4-6

### Dependências
- FASE 3 + FASE 4 + FASE 5 (ciclo completo operacional)

### Subitens

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 7.1 | Estender CrossOsAgent com 3 estados: LEARN → PROPOSE → AUTO | `crates/hermes/src/cross_os/agent.rs` | Agente evolui |
| 7.2 | Implementar notificação proativa ao usuário na 2a execução | `crates/hermes/src/cross_os/agent.rs` | "Encontrei skill WASM..." |
| 7.3 | Implementar proposta de criação de skill na 3a execução | `crates/hermes/src/cross_os/agent.rs` | "Quer criar skill WASM?" |
| 7.4 | Integrar com WorkflowLearner (já implementado) para detectar padrões | `crates/hermes/src/cross_os/agent.rs` → `crates/k_ai/src/workflow_learner.rs` | Padrões detectados |
| 7.5 | Teste e2e: "preciso editar planilha" 3x → 1a JAIL → 2a WASM → 3a criar | `crates/hermes/src/cross_os/agent.rs` | Ciclo completo OK |

---

## Topologia de Execução

```
FASE 1-2 (Semanas 1-2) — paralelo
  Eng 1: F1 Skill Manifest (~800 LOC)
  Eng 2: F2 Membrane + CapGate (~1.000 LOC)

FASE 3-5 (Semanas 2-4) — paralelo
  Eng 1: F3 Discoverer multi-fonte (~800 LOC)
  Eng 2: F5 WASI Preview 2 (~1.000 LOC)
  (F4 JAIL pode começar após F2)

FASE 4   (Semanas 3-5)
  Eng 1-2: F4 JAIL ~1.500 LOC (depende F2)

FASE 6-7 (Semanas 4-6)
  Eng 1: F6 Interop FYY (~500 LOC)
  Eng 2: F7 Ciclo aprendizado (~800 LOC)

TOTAL: ~6.500 LOC · 4-6 semanas · 2 engenheiros
```

---

## Resumo de Esforço

| Fase | Item | LOC | Dias | Depende |
|------|------|-----|------|---------|
| F1 | Skill Manifest | ~800 | 3-5 | Nenhuma |
| F2 | Membrane + CapGate | ~1.000 | 4-7 | Nenhuma |
| F3 | Discoverer multi-fonte | ~800 | 4-6 | F1 |
| F4 | JAIL sandbox | ~1.500 | 6-10 | F2 |
| F5 | WASI Preview 2 | ~1.000 | 5-8 | F1 |
| F6 | Interop FYY via MCP | ~500 | 3-5 | F3 |
| F7 | Ciclo aprendizado | ~800 | 4-6 | F3+F4+F5 |
| **Total** | | **~6.500** | **~29-47** | |

---

## Gate Checklist (por Fase)

1. ✅ `cargo check --release` = 0 erros
2. ✅ `demo()` self-test PASS (assert-based)
3. ✅ Boot QEMU: CrossOsAgent registrado e funcional
4. ✅ Log de evidência no serial (`[CROSS-OS] action result`)
5. ✅ Nenhum `unsafe` novo sem safety comment
6. ✅ Nenhuma regressão em módulos existentes
7. ✅ STATE.md + CHANGELOG.md atualizados
8. ✅ Commit semântico + tag por fase

---

## Referências

- `docs/architecture/0076-cross-os-ecosystem.md` — ADR-0076
- `docs/archive/notes/cross-os-patterns.md` — Catálogo de padrões extraídos (absorvido na ADR-0076 §2, movido 2026-08-05)
- `crates/hermes/src/cross_os/` — Código já iniciado (intent, discoverer, agent)
- `crates/hermes/src/mcp_server.rs` — MCP Server existente
- `crates/hermes/src/package_hub.rs` — PackageHub existente
- `crates/hermes/src/wasmi_rt.rs` — wasmi runtime existente
- `crates/k_ai/src/merkle_audit.rs` — MerkleAuditTrail existente
- `crates/k_ai/src/workflow_learner.rs` — WorkflowLearner existente
