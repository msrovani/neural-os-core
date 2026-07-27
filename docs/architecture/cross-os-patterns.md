# Padrões Extraídos — Ecossistema Cross-OS

**Data:** 2026-07-26
**Origem:** Pesquisa em repositórios de AIOS, skill marketplaces, unikernels WASM

---

## 1. FYY — Skill Manifest + Marketplace P2P

**Repo:** github.com/feiyueyun/fyy
**Licença:** FYY Software License (CE free)
**Core:** Marketplace descentralizado de skills, WireGuard mesh, Skill Manifest spec

### Padrões Consolidados

| Padrão | Status no neural-os-core |
|--------|--------------------------|
| Skill Manifest (skill.json) | ❌ Não implementado — PRECISAMOS |
| Grants-based access control | 🟡 CapGate existe mas não integrado com Membrane |
| WireGuard P2P mesh | ❌ Não implementado — MCP bridge path |
| MCP Gateway | ✅ MCP Server implementado |
| Interop com Anthropic Skills | ❌ Não implementado — pacote de 740K+ skills |
| CLI `fyy skill search/install/start` | 🟡 marketplace.rs tem search |

### O que Adotar

1. **Skill Manifest schema** — formato canônico para descrever toda skill
2. **Grants** — permissões granulares por skill, integrado ao CapGate
3. **MCP Gateway** — nossa ponte para o ecossistema FYY

---

## 2. Wetware — Zero Ambient Authority + Membrane

**Repo:** github.com/wetware/ww
**Core:** OS descentralizado para agentes, WASM cells, membrane capability grafts

### Padrões Consolidados

| Padrão | Status no neural-os-core |
|--------|--------------------------|
| Zero ambient authority | ❌ Não implementado — PRECISAMOS |
| Membrane (capability bundle) | ❌ Não implementado — PRECISAMOS |
| Cap'n Proto RPC | 🟡 NoProto implementado |
| Cells ~10ms spawn (WASM) | ✅ wasmi_rt com fuel |
| libp2p + IPFS | ❌ Não implementado |

### O que Adotar

1. **Membrane struct** — bundle de capabilities por execução
2. **Zero ambient authority** — Jail começa sem NADA, só o que a membrana concede
3. **Composable membranes** — A chama B chama C, cada hop carrega capabilities reduzidas

---

## 3. WeftOS — Skill Ecosystem + ExoChain + RBAC

**Repo:** github.com/weave-logic-ai/weftos
**Core:** AI OS em Rust, 22 crates, mesh networking, capability-based RBAC

### Padrões Consolidados

| Padrão | Status no neural-os-core |
|--------|--------------------------|
| SKILL.md declarativo | ✅ Já temos |
| WASM sandbox + fuel | ✅ wasmi_rt |
| ExoChain (hash chain) | ✅ MerkleAuditTrail |
| 3-branch governance | 🟡 SafetyInvariants |
| Plugin SDK | 🟡 PluginHub::scan() |
| HNSW vector store | 🟡 SGDB BQ index |
| Causal DAG cognitive tick | ❌ Não implementado |

### O que Adotar

1. **3-branch governance** — Allow/Deny/Escalate como padrão de decisão
2. **Plugin SDK** — Skills como plugins com scan obrigatório

---

## 4. Oreulius — Unikernel WASM-first

**Repo:** github.com/reeveskeefe/Oreulius-Kernel
**Core:** WASM-first unikernel, 143-entry host ABI dispatch table

### Padrões Consolidados

| Padrão | Status no neural-os-core |
|--------|--------------------------|
| Host ABI dispatch table | 🟡 wasmi Linker + imports |
| WAT test modules | ❌ Não implementado |
| SDK separado do kernel | ✅ wasmi_rt separado |
| Fuel metering | ✅ wasmi fuel |

### O que Adotar

1. **WAT test modules** — testes de integração para nossa skill ABI
2. **Dispatch table formal** — catálogo de funções host disponíveis para WASM

---

## 5. WAeasi — Microkernel WASI Preview 2

**Repo:** github.com/Mesokiiii/WAeasi
**Core:** Microkernel no_std que executa WASM Component Model

### Padrões Consolidados

| Padrão | Status no neural-os-core |
|--------|--------------------------|
| WASI Preview 2 como syscall | ❌ Não implementado |
| Software fault isolation | ✅ wasmi SFI |
| Single shared address space | 🟡 kernel já single-address-space |
| Async executor p/ WASM | 🟡 async_rt existe |

### O que Adotar

1. **WASI Preview 2** — suporte no wasmi_rt para skills padrão

---

## 6. Conclusão — Prioridade de Implementação

| # | O que | Inspiração | Esforço |
|---|-------|-----------|---------|
| 1 | **Skill Manifest** schema + compat FYY | FYY | 4h |
| 2 | **Membrane** struct + CapGate | Wetware | 4h |
| 3 | **WASI Preview 2** no wasmi_rt | WAeasi | 8h |
| 4 | **Discoverer multi-fonte** | FYY + WeftOS | 4h |
| 5 | **JAIL** com Membrane + audit | Wetware + WeftOS | 8h |
| 6 | **Interop FYY via MCP** | FYY | 4h |
| 7 | **WAT test modules** | Oreulius | 2h |
