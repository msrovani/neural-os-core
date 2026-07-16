# LEGACY — snapshots preservados

Código histórico retirado do workspace ativo sem perda de informação.
Nada nesta pasta participa do build padrão.

| Caminho | Origem | Motivo |
|---------|--------|--------|
| `k_ia/` | `crates/k_ia/` | Nome e implementação anteriores ao crate ativo `k_ai` |
| `jarvis/` | `crates/jarvis/` | Nome e implementação anteriores ao crate ativo `jarbas` |
| `v1.5-neural-kernel-src/` | `crates/neural-kernel/src/` | Snapshot do monólito v1.5 |
| `v1.5-dead-k2chj/` | crates K²CHJ | Módulos mortos removidos na Ponytail Audit |

## Política

- Preservar histórico; não usar como dependência.
- Não adicionar estes caminhos ao `workspace.members`.
- Consultar a ADR-0042 antes de restaurar qualquer módulo.
- Código recuperado deve voltar por mudança explícita, revisão e `cargo nk`.
