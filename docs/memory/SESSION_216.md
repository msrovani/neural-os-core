# SESSION_216 — SGDB Agent (A-026): bridge EventBus ↔ SGDB + versionamento de skills

**Data:** 2026-07-24
**Sprint:** v1.9.10-emagrecer-plan
**Commit:** (pendente)

## Objetivo

Criar um agente nativo EventDriven que exponha operações do SGDB (ADR-0063) via EventBus, eliminando a necessidade de chamadas diretas à API Rust para operações complexas como versionamento de skills, rollback, e consultas semânticas.

## Motivação

- **"Tudo é Agente"** — chamar `k_ai::sgdb::put_kv()` de qualquer lugar acopla o caller ao crate `k_ai` e exige recompilação. Um agente SGDB permite que qualquer skill/WASM/agente publique `SGDB_CMD` e receba `SGDB_RESULT`, sem linker.
- **Versionamento de skills** — não existia antes. Agora skills têm histórico navegável no SGDB (`skill/hist/{name}/v{N}`), rollback via evento.
- **Extensível** — novos comandos não precisam de API Rust, só de mais um `match` no agente.

## O que foi feito

### arquivo: `crates/hermes/src/sgdb_agent.rs`

229 LOC, 0 warnings, 0 erros.

```
SGDB_CMD ──→ SgdbAgent (EventDriven) ──→ k_ai::sgdb::*
                  │
                  └──→ SGDB_RESULT
```

### Comandos implementados

| Comando | Input | Ação | Output |
|---|---|---|---|
| `store_version\|name\|source` | nome + source | Move curr→hist/vN, escreve novo curr, update head | `ok|version=v2 old=1` |
| `rollback\|name\|version` | nome + "v2" | Restaura hist/name/v2 para curr | `ok|v2` |
| `list_versions\|name` | nome | ART prefix scan `skill/hist/{name}/` | `ok|v1,v2,v3` |
| `list_skills\|` | — | Lê `sys/skill_index` | `ok|[...]` |
| `store_skill\|name\|desc` | nome + desc | `put_skill_blob` + append `sys/skill_index` | `ok|registered n=5` |
| `recall\|query\|k` | texto + K | `prompt_slice` (fallback textual; embedding TBD) | `ok|(contexto)` |

### Schema de chaves no SGDB

```
skill/curr/{name}        → source atual (String)
skill/hist/{name}/v{N}   → source da versão N (String)
skill/head/{name}        → "v{N}" (ponteiro de versão)
sys/skill_index          → "skill1,skill2,skill3" (índice de skills conhecidos)
```

### Arquivos modificados

- **`crates/hermes/src/sgdb_agent.rs`** — NOVO: agente EventDriven (229 LOC)
- **`crates/hermes/src/lib.rs`** — +1 linha: `pub mod sgdb_agent;`
- **`crates/neural-kernel/src/main.rs`** — import + registro no AgentFleet (após BrowserAgent)

### Registro no boot

```rust
k_nano::slog_bin!("Boot", "register", "SgdbAgent");
registry.register(Box::new(sgdb_agent::SgdbAgent::new()));
```

Registrado no AgentFleet após BrowserAgent e antes de WifiAgent.

## Verificação

`cargo check --release` — **0 erros, 0 warnings** (fixado unused import `alloc::boxed::Box`)

## Decisões de design

1. **Hot-path direto, não via agente:** `put_kv`/`get_kv`/`put_hanr` continuam chamada direta — não faz sentido pagar latência de EventBus pra operação de 1µs.
2. **Payload pipe-delimitado:** sem serde, sem `serde_json_core`, sem aloc extra. Primeiro campo = comando, resto = args. Suficiente pro MVP.
3. **Rollback com snapshot automático:** ao restaurar `v2`, o `curr` atual NÃO é perdido — ele é salvo como `v{N+1}` antes. Rollback nunca é destrutivo.
4. **`recall` como stub textual:** recall semântico real precisa de embedding (cortex::projection). Hoje faz `prompt_slice` como fallback. Quando o embedding estiver pronto, o comando `recall` usa `recall_semantic`.
5. **Índice de skills:** `sys/skill_index` é CSV simples, não JSON. Parse manual no agente. Quando houver mais de 20 skills, migrar para ART index.

## Lições

- O padrão BrowserAgent (EventDriven, subscribe to REQUEST, publish RESPONSE) é maduro e serviu como template → implementação rápida.
- `agent_core::Agent` trait requer `Send` — struct com `event_bus::Receiver` é `Send` por construção.
- Pipe-delimited é feio mas eficaz pra no_std sem serde. Se um dia precisar de tipos complexos, usar `serde_json_core` (já disponível no workspace).
