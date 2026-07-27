# ADR-0076: Cross-OS Ecosystem — O Melhor de Cada Mundo

**Data:** 2026-07-26
**Status:** Proposed
**Lifecycle:** `por_fazer`
**Inspirado por:** FYY, Wetware, WeftOS, Oreulius, WAeasi, ClaudioOS
**Sprint:** v2.0.0+

---

## 1. Problema

O AIOS precisa executar software que não foi feito para ele. A abordagem não é "portar código" — é criar um **ecossistema inteligente** que:

1. **Descobre skills WASM em runtime** (não em design time)
2. **Aprende com cada uso** e evolui a experiência
3. **Isola apps legacy** com segurança militar
4. **Interopera com outros ecossistemas** (FYY, WeftOS, MCP, Anthropic Skills)

---

## 2. Melhor de Cada Mundo

### De FYY — Skill Manifest + Marketplace P2P

| Padrão | O que é | Como usamos |
|--------|---------|-------------|
| **Skill Manifest** | skill.json com nome, versão, permissões, when_to_use, risk_level | Schema canônico para descrever toda skill no PackageHub |
| **Grants-based access** | Permissões granulares por skill | Mapear direto para nosso CapGate + Membrane |
| **WireGuard mesh** | P2P criptografado entre agents | Conectar nosso MCP Server ao mesh FYY |
| **Interop layer** | Compatível com Anthropic Skills (740K+), MCP, A2A, ClawHub | Nossas skills.jsons devem ser compatíveis |

### De Wetware — Zero Ambient Authority + Membrane

| Padrão | O que é | Como usamos |
|--------|---------|-------------|
| **Zero ambient authority** | Célula WASM não tem acesso a NADA até receber capabilities | Nosso Jail começa sem nenhuma capability |
| **Membrane graft** | Cada chamada carrega um bundle de capabilities explícito | CapGate + Membrane struct por execução |
| **Cap'n Proto RPC** | IPC tipado entre células | Nossa NoProto + MessageBus |
| **Cells ~10ms spawn** | WASM cells leves vs VMs pesadas | wasmi_rt já faz isso |

### De WeftOS — Skill Ecosystem + ExoChain + RBAC

| Padrão | O que é | Como usamos |
|--------|---------|-------------|
| **SKILL.md declarativo** | Manifesto de skill em Markdown | Já temos — compatível! |
| **WASM sandbox + fuel** | Fuel metering + heap limit | wasmi_rt + AgentBudget |
| **ExoChain** | Hash chain de auditoria | MerkleAuditTrail já implementado |
| **3-branch governance** | allow/deny/escalate | Nosso SafetyInvariants + SecurityAgent |
| **Plugin SDK** | Extensões via WASM | PluginHub::scan() já faz scan |

### De Oreulius — Unikernel WASM-first

| Padrão | O que é | Como usamos |
|--------|---------|-------------|
| **143-entry host ABI** | Catálogo de funções host para WASM | Nosso wasmi Linker com imports controlados |
| **Dispatch table com nome** | Funções resolvidas por string (não índice) | Mesmo padrão do wasmi |
| **WAT test modules** | Testes WAT que validam ABI | Criar testes WAT para nossa skill API |

### De WAeasi — WASI Preview 2 como Syscall

| Padrão | O que é | Como usamos |
|--------|---------|-------------|
| **WASI Preview 2** | Interface padrão WASM-sistema | Adicionar suporte WASI no wasmi_rt |
| **SFI (Software Fault Isolation)** | Isolamento via bytecode WASM (mais leve que Ring3) | Usar wasmi com fuel + mem limit |
| **Async executor p/ WASM** | Components são Futures | Nosso async_rt já tem TimerFuture |

---

## 3. Arquitetura Consolidada

```
                    CrossOsAgent (agente Continuous)
                           |
           +---------------+---------------+
           |               |               |
     [Intent Analyzer]  [Discoverer]   [Learner]
           |               |               |
           v               v               v
     Categoria need    Busca skills    Aprende padrao
           |               |               |
           v               v               v
     +-----------------------------------------+
     |            Execution Engine              |
     |  +----------+  +---------+  +---------+  |
     |  |WASM Skill|  |   JAIL  |  |  MCP    |  |
     |  |(wasmi_rt)|  |(Membrane|  | Bridge  |  |
     |  | +fuel    |  | +CapGate|  |(FYY/    |  |
     |  | +WASI    |  | +Audit) |  | WeftOS) |  |
     |  +----------+  +---------+  +---------+  |
     +-----------------------------------------+
```

### 3.1 Skill Manifest (Formato Canônico)

Toda skill no PackageHub segue o schema FYY-compatible:

```json
{
  "name": "planilha-editor",
  "version": "1.0.0",
  "type": "wasm",
  "description": "Edita planilhas Excel (.xlsx)",
  "when_to_use": ["editar planilha", "modificar xlsx", "excel"],
  "risk_level": "low",
  "permissions": {
    "filesystem": { "allow": ["/tmp/*", "/home/*.xlsx"], "deny": ["/etc/*"] },
    "network": "none",
    "hardware": "none"
  },
  "capabilities": ["vfs_read", "vfs_write"],
  "interop": {
    "mcp": true,
    "fyy": true,
    "agent_skills": true
  },
  "resource_limits": {
    "fuel_max": 1000000,
    "heap_max": 67108864,
    "timeout_ms": 30000
  }
}
```

### 3.2 Membrane (Wetware-inspired)

Cada execução recebe uma **membrana** — um bundle de capabilities explícito:

```rust
pub struct Membrane {
    /// Paths de filesystem permitidos (glob)
    pub fs_allow: Vec<String>,
    /// Paths bloqueados (glob) — override em allow
    pub fs_deny: Vec<String>,
    /// Endpoints de rede permitidos
    pub net_allow: Vec<String>,
    /// Capacidades HW/MMIO — sempre vazio para apps legacy
    pub capabilities: Vec<Capability>,
    /// Fuel máximo (instruções WASM)
    pub fuel_budget: u64,
    /// Heap máximo (bytes)
    pub heap_max: usize,
    /// Timeout de execução (ms)
    pub timeout_ms: u64,
}

impl Membrane {
    /// Cria membrana para app legacy — zero capacidades, mínimo necessário
    pub fn for_legacy(app_name: &str) -> Self {
        Self {
            fs_allow: vec![format!("/jail/{}/*", app_name), "/tmp/*".into()],
            fs_deny: vec!["/etc/*".into(), "/boot/*".into(), "/dev/*".into()],
            net_allow: Vec::new(),  // sem rede
            capabilities: Vec::new(), // sem HW
            fuel_budget: 10_000_000,
            heap_max: 256 * 1024 * 1024, // 256MB
            timeout_ms: 60_000,
        }
    }

    /// Verifica se uma operacao é permitida
    pub fn check(&self, op: &Operation) -> Result {
        // CapGate::check() delegado
    }
}
```

### 3.3 Descoberta Multi-Ecossistema

O Discoverer consulta em paralelo:

```rust
pub async fn discover(intent: &IntentResult) -> Vec<SkillCandidate> {
    // 1. PackageHub local (já implementado)
    let local = search_package_hub(intent);
    
    // 2. FYY mesh via MCP
    let fyy = search_fyy_mcp(intent).await;
    
    // 3. WeftOS/ClawHub via MCP
    let weft = search_weftos_mcp(intent).await;
    
    // 4. Internet (GitHub, crates.io) via HTTP
    let net = search_internet(intent).await;
    
    // Merge + dedup + rank por confiança
    merge_and_rank(&[local, fyy, weft, net])
}
```

---

## 4. Segurança — Membrana (Wetware + WeftOS)

| Camada | Inspiração | O que bloqueia | Implementação |
|--------|-----------|---------------|---------------|
| L0 Scan | FYY PluginHub | Binário malicioso | PluginHub::scan() heurístico |
| L1 Fuel | WeftOS | CPU infinite loop | wasmi fuel + AgentBudget |
| L2 Memória | Wetware | OOM / overflow | Membrane.heap_max |
| L3 Membrana | Wetware | Acesso não autorizado | Membrane.check() → CapGate |
| L4 Filesystem | Wetware | Path traversal | fs_allow + fs_deny (glob) |
| L5 Rede | Wetware | Exfiltração | net_allow vazio (default deny) |
| L6 Audit | WeftOS (ExoChain) | Nada passa despercebido | MerkleAuditTrail |
| L7 Governance | WeftOS | Decisão: allow/escalate/deny | SafetyAgent + HITL |

---

## 5. Integrações Existentes

| Componente | Origem | Papel no Cross-OS |
|-----------|--------|-------------------|
| PackageHub | Já temos | Catálogo de skills WASM |
| marketplace.rs | Já temos | Busca remota HTTP |
| PluginHub::scan() | Já temos | Segurança pré-execução |
| wasmi_rt | Já temos (ADR-0059) | Runtime WASM com fuel |
| decode_harness | Já temos | Geração WAT→WASM |
| app_factory | Já temos | Seletor A/B/C + HITL |
| MCP Server | Já temos | Ponte FYY/WeftOS |
| WorkflowLearner | Já temos | Detecção de padrões |
| MerkleAuditTrail | Já temos | Audit chain |
| SafetyInvariants | Já temos | I1-I4 invariantes |
| CapGate | ADR-0041 (PoC) | Membrana de capabilities |

---

## 6. Plano

| Fase | O que | Inspiração | Entrega |
|------|-------|-----------|---------|
| F1 | Skill Manifest schema + compat FYY | FYY | Schema JSON validado |
| F2 | Membrane struct + CapGate | Wetware | Membrana operacional |
| F3 | Discoverer multi-fonte | FYY + WeftOS | Busca em runtime |
| F4 | JAIL com Membrane + audit | Wetware + WeftOS | Sandbox operacional |
| F5 | WASI suport no wasmi_rt | WAeasi | Skills WASM padrão |
| F6 | Interop FYY via MCP | FYY | Marketplace externo |
| F7 | Ciclo aprendizado completo | — | Auto-evolução |

---

## 7. Referências

- FYY: `github.com/feiyueyun/fyy` — Skill Manifest, WireGuard mesh, grants
- Wetware: `github.com/wetware/ww` — Zero ambient authority, membrane, Cap'n Proto
- WeftOS: `github.com/weave-logic-ai/weftos` — SKILL.md, ExoChain, RBAC
- Oreulius: `github.com/reeveskeefe/Oreulius-Kernel` — WASM-first, host ABI dispatch
- WAeasi: `github.com/Mesokiiii/WAeasi` — wasm32-wasi no_std, WASI Preview 2
- Anthropic Agent Skills: `github.com/anthropics/agent-skills` — 740K+ skills

