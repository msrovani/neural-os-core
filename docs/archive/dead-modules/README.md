# Dead modules — arquivo de código legado/planejado (Sessão AION · missão §15)

> ## ⚠️ SINAL — LEIA ANTES DE PLANEJAR QUALQUER FEATURE NOVA
>
> **Antes de planejar ou implementar uma feature nova, consulte este índice.**
> Várias features já foram **planejadas e escritas** e estão arquivadas aqui.
> Não re-planeje o que já existe: se algo abaixo corresponde ao que você quer,
> **restaure e wire** (`git mv` de volta + `pub mod`), em vez de reescrever.
> (Missão §2: `UNKNOWN != 0`; §15: reduzir complexidade sem perder intenção.)

## Por que estes arquivos estão aqui

Nenhum é compilado nem chamado em runtime — verificado por varredura de
declaração `mod` (nenhum está declarado no crate, exceto os marcados **[tier2]**,
que estavam declarados mas com **zero callers**). Movidos de `crates/<crate>/src/`
para cá para limpar o build, o `grep` e o `tools/check_duplication.py` **sem
apagar a intenção**.

**Recuperar um módulo:**
```
git mv docs/archive/dead-modules/<crate>/<file>.rs crates/<crate>/src/<file>.rs
# + adicionar `pub mod <file>;` em crates/<crate>/src/lib.rs (ou no mod pai)
```

## hermes

| Arquivo | O que faria (intenção original) |
|---|---|
| `adaptation.rs` | Cognitive Adaptation Engine (LEGACY): geração de política de adaptação |
| `wifi_agent.rs` | WiFi Continuous (A-023) — o real hoje é `k_hal` SoftMAC + WifiAgent |
| `optimizer.rs` | A-020 OptimizerAgent (self_optimizing) |
| `sgdb_agent.rs` | Bridge EventBus → SGDB (ADR-0063 + versionamento de skills) |
| `wasi_host.rs` | Host stubs WASI Preview 1 (`wasi_snapshot_preview1`, ADR-0076 F5) |
| `quarantine.rs` | Gate de quarentena dual-LLM: sanitiza input não-confiável antes do LLM (ADR-0076 3.3) |
| `native_agents.rs` | Manifestos dos 25 agentes nativos (A-001..A-025) |
| `jail.rs` | JAIL: sandbox com Membrane + wasmi + audit Merkle (ADR-0076 F4) |
| `gguf_wasm.rs` | GGUF via skill WASM (isolado do LLM do kernel) |
| `link_watcher.rs` | Saúde eth/wifi, failover, histerese (FE only — ADR-0041) |
| `orchestrator.rs` | Orquestração multi-agente em grafo (seq/concurrent/handoff) |
| `intent_bus.rs` | Intent Bus: intenções tipadas (agentes emitem intents, não syscalls — ADR-0076 4.4) |
| `expert_skills.rs` | Skills nativas dos experts Trinity (disk_diag/security) |
| `actor_registry.rs` | Actor Registry (#209): subagentes com permission model + task state machine |
| `rss_agent.rs` | Agente RSS |
| `search_agent.rs` | SearchAgent (#307): busca HTTP (DuckDuckGo) via E1000 |
| `app_store.rs` | AppForge + Marketplace (#186/#246): catálogo + install + Ed25519 |
| `email_agent.rs` | Agente de e-mail |
| `mcp_client.rs` | MCP Client: bridge CrossOsDiscoverer ↔ MCP Server (ADR-0076 F3/F6) |
| `elf_loader.rs` | Loader ELF cross-OS (detect+entry smoke; NÃO é loader de produção — WASM = ADR-0059) |
| `crdt.rs` **[tier2]** | CRDT LWW/multi-value no_std (ADR-0100 T-048/T-049) — duplicata de `k_ai::sgdb::crdt_sync` |
| `mcp_server.rs` **[tier2]** | MCP Server (#172): JSON-RPC 2.0 sobre EventBus — o vivo é `hermes::mcp` |

## k_ai

| Arquivo | O que faria |
|---|---|
| `workflow_learner.rs` | Workflow Learner (#157-#158): analisa padrões de uso e prediz recursos |
| `skill_snapshot.rs` | Tool-State Save Game (#315.15): snapshot/rollback de estado de skill |
| `fine_tuning_pipeline.rs` | FineTuningPipeline (#312b): fine-tuning on-device (LEARNER.BIN) |
| `agency_importer.rs` | Compat do importer de divisões importadas |

## k_nano

| Arquivo | O que faria |
|---|---|
| `fw_cfg.rs` | Persistência de sessão via QEMU fw_cfg (Labor 46) |
| `verify.rs` | Verificador da OpCode VM (eBPF-style) — `@dead`, WASM substituiu |
| `io_scheduler.rs` | I/O Scheduler: Deadline + write coalescing |
| `p2p.rs` | Orquestração P2P Master↔Nodes — o vivo está em `net/mesh` |

## neural-kernel

| Arquivo | O que faria |
|---|---|
| `bench.rs` | Framework de benchmark — `@dead`, nunca integrado ao scheduler |
| `tracer.rs` | Span tracer distribuído — `@dead`, SelfCritique substituiu |

## Relacionado (NÃO arquivado, corrigido em vez disso)

- `hermes/src/agents.rs` — **probe de lab** do `AutoLearnAgent` (fabricava 3 eventos
  `security` no tick de produção): **removido** (missão §3, evidência falsa).
- `tools/check_duplication.py` — acusava ~20/33 **falsos positivos**: **corrigido**
  (agora ignora `#![...]` e `use ...`).
- `neural-kernel/src/labor_smokes.rs` — smokes que só emitem veredito sem testar
  (`AWAITING_HW` hardcoded): **candidato a arquivo/colapso** (missão §15 tier 5.1).
