# LEGACY — snapshots preservados

Código histórico retirado do workspace ativo sem perda de informação.
Nada nesta pasta participa do build padrão.

| Caminho | Origem | Motivo |
|---------|--------|--------|
| `k_ia/` | `crates/k_ia/` | Nome e implementação anteriores ao crate ativo `k_ai` |
| `jarvis/` | `crates/jarvis/` | Nome e implementação anteriores ao crate ativo `jarbas` |
| `v1.5-neural-kernel-src/` | `crates/neural-kernel/src/` | Snapshot do monólito v1.5 |
 | `v1.5-dead-k2chj/` | crates K²CHJ | Módulos mortos removidos na Ponytail Audit |
| `v1.9.9-test/k_nano/p2p/` | `crates/k_nano/src/p2p/` | P2P networking (NoProto, clock, MPMC) — especulativo, sem runtime real |
| `v1.9.9-test/k_nano/net/brain_mesh.rs` | `crates/k_nano/src/net/brain_mesh.rs` | Neural P2P mesh engine — especulativo, apenas testes |
| `v1.9.9-test/k_nano/scheduler/core_pair.rs` | `crates/k_nano/src/scheduler/core_pair.rs` | Core pair allocator + Bipole — substituído pelo scheduler em agent-core |
| `v1.9.9-test/agent-core/budget.rs` | `crates/agent-core/src/budget.rs` | AgentBudget/BudgetManager — nunca conectado ao scheduler |
| `v1.9.9-test/agent-core/hooks.rs` | `crates/agent-core/src/hooks.rs` | HookRegistry — nunca registrado |
| `v1.9.9-test/hermes/wasm.rs` | `crates/hermes/src/wasm.rs` | WASM bridge legado — substituído por wasmi_rt (código útil migrado) |
| `v1.9.9-test/hermes/wasm_exec.rs` | `crates/hermes/src/wasm_exec.rs` | VM Op custom legada — substituída por wasmi_rt |
| `v1.9.9-test/hermes/wasm_rt.rs` | `crates/hermes/src/wasm_rt.rs` | Runtime WASM custom — substituído por wasmi_rt |
 | `v1.9.9-test/hermes/wat_tests.rs` | `crates/hermes/src/wat_tests.rs` | Testes WAT — apenas testes, não necessário no bin |
| `v1.9.9-test/k_nano/hardware/` | `crates/k_nano/src/hardware/` | Topologia Xeon/EPYC/Client — especulativa, só StandardUma em prática |
| `v1.9.9-test/hermes/adaptation/` | `crates/hermes/src/adaptation/` | Cognitive adaptation engine — especulativa, nunca chamada em runtime real |

## Política

- Preservar histórico; não usar como dependência.
- Não adicionar estes caminhos ao `workspace.members`.
- Consultar a ADR-0042 antes de restaurar qualquer módulo.
- Código recuperado deve voltar por mudança explícita, revisão e `cargo nk`.
