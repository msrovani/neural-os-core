# SESSION_256 — neural-sgdb: extração do SGDB como projeto comunitário standalone (2026-08-09)

**Escopo:** Extração do núcleo do SGDB (`k_ai::sgdb`, ADR-0063) para crate/projeto
separado `neural-sgdb` (github.com/msrovani/neural-sgdb), dual-mode `no_std`+`std`,
zero deps, licença MIT OR Apache-2.0, para uso da comunidade.
**Status:** ✅ Fechada — v0.1 extraído + CRDT p2p + benchmarks + MCP server.
**Repo novo:** `C:\DEV\neural-sgdb` (4 commits: `f14c2e4`, `d62da15`, `9e1080d`, `378e633`).

---

## 1. Decisão de topologia (HITL)

Usuário decidiu **Modo 1**: repo separado, evolução independente. O neural-os-core
**mantém** `k_ai::sgdb` interno (AGPL); o neural-sgdb é projeto comunitário próprio
(MIT OR Apache-2.0). **Sem fiação** (nem path dep nem versão) neste momento — a
porta futura é versão publicada no crates.io. Ponto de compatibilidade entre os
dois: **formato de documento NMD1** (byte-idêntico, testado).

Alternativas descartadas: workspace crate no monorepo (entrelaça AGPL↔MIT e exige
CLA para contribuidores externos); path dependency (acorrenta evolução).

## 2. O que foi extraído

| Componente | Origem (`k_ai::sgdb`) | Status |
|---|---|---|
| `art.rs` ART Node4/16/48/256 + SSE | 836 LOC | ✅ portado |
| `memory_doc.rs` NMD1 (contrato de formato) | 289 LOC | ✅ verbatim |
| `bq.rs` + `hamming_dispatch.rs` (AVX-512/AVX2/scalar) | 352 LOC | ✅ portado (seam caps) |
| `engine.rs` instance-based | 255 LOC | ✅ adaptado (sem ENGINE global) |
| `layers.rs` → facade `Sgdb` | 285 LOC | ✅ fundido em sgdb.rs |
| `store.rs` namespaces OS (hanr/pkg/audit/sys/hw) | 212 LOC | ❌ não portado (OS-only) |
| `crdt_sync.rs` | 288 LOC | ✅ feature `p2p` (transporte plugável) |
| `bench.rs`/`metrics.rs`/`e2e_smoke.rs` | — | ✅ substituídos (testes/bench novos) |

**Novos no repo:** `Storage` trait + `InMemory` + `FileStorage` (append-log CRC32,
crash-safe), facade `Sgdb`, `CrdtMemorySync` + `Transport` + `UdpTransport`,
`examples/bench.rs`, `examples/mcp_server.rs`.

## 3. Seams substituídas (kernel → crate)

| Seam kernel | Substituição |
|---|---|
| `k_nano::storage` (TickvLite) | `Storage` trait + InMemory/FileStorage |
| `TIMER_TICKS` | parâmetro `now: u64` |
| `platform_probe::{allow_avx2, allow_avx512}` | `cpu_caps()` (std auto-detect; no_std `set_cpu_caps`) |
| `slog_kai!` | macro `sgdb_log!` (no_std no-op / std eprintln) |
| `mesh/udp_broadcast/EventBus` (crdt) | `Transport` trait + `UdpTransport` std |

## 4. Verificação

- `cargo test`: **20 testes + doc-test** (default) — ✅
- `cargo test --features p2p`: **24 testes + doc-test** — ✅
- `cargo check --no-default-features --target x86_64-unknown-none`: **limpo** — ✅
- Bench (AVX2, host): ART get P50≈200ns, insert P50≈800ns, BQ top-5 ≈310µs em
  10k×1024 dims, recall@5 BQ vs FP32-exact = **100%** — ✅
- MCP server smoke (initialize→tools/list→remember→recall→ping→-32601): ✅

## 5. Lições (registradas no AGENTS.md)

1. **Subagentes não escrevem fora do workspace:** 2 fixers retornaram VAZIO ao
   tentar criar arquivos em `C:\DEV\neural-sgdb` (fora de `C:\DEV\neural-os-core`).
   Sandbox de escrita do subagente é o workspace; orquestrador executou direto.
   Sintoma: task "completed" com resultado vazio e zero arquivos — verificar
   sempre o alvo de escrita antes de delegar.
2. **`f32::sqrt` NÃO existe no core** para `x86_64-unknown-none` (confirmado
   empiricamente com rustc). O kernel usa `libm::sqrtf` por isso. No crate
   zero-deps, Newton-Raphson de 10 iterações (~6 LOC). `deny(warnings)` no_std
   eleva dead-code a erro — `#[allow(dead_code)]` explícito onde o port deixa
   API não usada.
3. **MCP (Model Context Protocol) para expor memória a agentes:** handshake
   legado `2025-11-25` (initialize→initialized→tools/list→tools/call), JSON-RPC
   2.0 sobre stdio UMA mensagem por linha `\n`, stdout SÓ JSON-RPC (logs→stderr),
   `-32601` em `server/discover` faz client moderno fazer fallback p/ initialize.
   Claude Code envia `tools/list` sem esperar `notifications/initialized` — não
   gatear tools no initialized. Embeddings: crate standalone não tem BGE — demo
   por trigramas hash rotulado como tal.
4. **CRDT rate-limit com `Option<u64>`** (não sentinela 0): primeiro sync em
   `now=0` com guard `!= 0` nunca mais rate-limitaria (no kernel o tick nunca é
   0; no crate host pode ser).
5. **Limitação herdada do ART upstream:** chaves onde uma é prefixo de outra
   não são suportadas (split silencioso). Kernel nunca insere isso (sufixos de
   largura fixa); documentado no README/`docs/api.md`.

## 6. Roadmap do neural-sgdb (5/6)

- [x] Núcleo portátil · [x] Storage trait + seams · [x] CRDT p2p ·
  [x] Benchmarks · [x] MCP server
- [ ] Interop TKLV byte a byte com o OS — **adiado**: exige o leitor TickvLite
  do OS disponível no host para verificação honesta (NMD1 documento já interop).

## 7. Referências

- ADR-0063 (SGDB), docs/api.md do neural-sgdb (contrato de API + mapa de migração)
- Repo: github.com/msrovani/neural-sgdb — commits `f14c2e4` (v0.1), `d62da15` (p2p),
  `9e1080d` (bench+MCP), `378e633` (chore .db)
