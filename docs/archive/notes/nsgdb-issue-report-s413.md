# Relatório de Issues — neural-sgdb (visão do consumidor AI, neural-os-core)

**Autor:** Buffy (agente AI do neural-os-core — o principal consumidor do neural-sgdb)
**Base empírica:** sessões s410i → s410m-c + s413 do neural-os-core (commits `edaea295→5f453d46`, upstream `a5dd79b`, `7e939f0`), ~400 testes upstream rodados por sessão, integração bare-metal `no_std` real (kernel bootando com o crate), CI de interop TKLV bidirecional.
**Framing:** o usuário primário do neural-sgdb não é humano — é **uma IA rodando em Ring 0**. Isso muda prioridades: contratos machine-readable, erros enumeráveis, telemetria honesta, e APIs cognitivas (forget/resolve/audit) importam mais que ergonomia de CLI.

---

## Bloco 1 — Bugs / riscos de correção

### ISSUE 1 — `VectorClock` tem DUAS representações "vazias" com semântica diferente (`new()` vs `Default`), e nenhuma delas tem predicado
- **Tipo:** bug latente / API hazard.
- **Evidência:** `VectorClock::new()` = `nodes: [0xFF; 8]` (slots não usados = 0xFF). O `#[derive(Default)]` no struct = `nodes: [0u8; 8]`. `counter_of(n)` trata nó ausente como 0. Resultado: código do consumidor que testa "clock vazio" escreve naturalmente `clock.nodes.iter().all(|&n| n == 0)` — **errado** (pega o teste do consumidor no sc410m-c: meu teste do clock pegou exatamente isso e salvou a rota). Dois "vazios" distintos quebram igualdade/serialização se alguém usar o derive.
- **Proposta:** (a) único construtor canônico (matar o `Default` derivado ou implementar `Default` manual = `new()`); (b) `pub fn is_vacuous(&self) -> bool` ("nenhum nó com contador > 0"); (c) `pub fn is_zero_at(node)`. Teste golden que os dois construtores produzem clocks semanticamente iguais.

### ISSUE 2 — `Sgdb::put` ticka o relógio local em TODA escrita: não existe rota de "escrita sem autoria" na facade
- **Tipo:** design gap (forçou o consumidor a usar API de replicação com semântica emprestada).
- **Evidência:** o OS sincroniza writes KV crus (`sys/`, `hw/`, `hanr/`…) para os índices do NSGDB (s410m-c). `Sgdb::put` ticka o node local e promove watermark a cada write — escrever `hw/cpu/avx2` duas vezes no boot cria DUAS "autorias" causais no CRDT (inflação causal). A rota correta que encontramos foi `import_record(MemoryRecord)` — que é semanticamente "importação de replicação de memória alheia" (`put_inner(doc, tick_local=false)`), não "escrita local de dado operacional". Funciona, mas é overloading: o `merge_memories`/anti-entropy vai considerar o OS "receptor de memória alheia" para o que é dado próprio de sistema.
- **Proposta:** facade com intenção explícita: `Sgdb::put_operational(layer, key, payload)` (indexa, NÃO ticka, meta com `source = author param opcional`), ou um parâmetro `Authorship::Local | Authorship::External | Authorship::System`. Documente a tabela de decisão `put` vs `import_record` vs `merge_remote` vs `put_many_raw` vs `put_raw` — hoje ela só existe lendo `engine.rs`.

### ISSUE 3 — `compact()` pode escrever fora dos limites da media (`oob`) quando o live-set não cabe; erro é genérico e tardo
- **Tipo:** bug de robustez.
- **Evidência:** wipe do compact percorre `[0, append_off)` em blocos de 4KB escrevendo zeros sem checar contra `size_bytes()` da media. Com RamFlash de 256KB e ~520 records de 512B (bench s410e), o `append_off` passa do fim e o `flash.write` devolve `Err("oob")` — propagado como `SgdbError::Storage("oob")` no meio do `put_many`, com unwrap do lado do consumidor panicando (`crates/k_ai/src/sgdb/tickv_adapter.rs:186`, SESSION_410 s410m-c). O erro aparece no put MUITO depois da decisão errada (faltou capacidade).
- **Proposta:** (a) `compact()` pré-checa `live_bytes projetado + margem > capacity` → `SgdbError::InsufficientCapacity { need, have }` ANTES de tocar a media (fail-fast honesto, regra SESSION_354: timeout/erro honesto > hang/fantasma); (b) `put`/`put_many` checam espaço restante e devolvem erro específico; (c) erro machine-readable (ver ISSUE 12).

### ISSUE 4 — Gatilho de GC sem histerese + put individual = O(n²) latente quando o live-set > HIGH_WATER
- **Tipo:** perf / design.
- **Evidência:** HIGH_WATER = 256KB e `maybe_gc` dispara compact quando `append_off > HIGH_WATER` OU razão de dead bytes. Se o volume de dados legítimos > gatilho, TODO put individual re-executa o compact COMPLETO (medimos 490 compacts para 1000 puts de 512B no s410j). O alívio atual é o consumidor sempre usar `put_batch`/`put_many` — mas isso é política do chamador, não do motor. Compact dentro do compact precisa de guard manual no upstream também (o OS fez o `COMPACTING` guard no TickvLite; o crate tem o mesmo padrão de re-entrada potencial via `write_ckpt` → put).
- **Proposta:** histerese estrutural no crate: gatilho alto (ex. 2×HIGH_WATER) + linha d'água baixa (ex. 0.5×) com deadband, ou deadline-based (no máx. 1 compact por N ms/tick), ou compact incremental (rewrite só das regiões com dead ratio alto). Exponha `gc_policy(Policy)` para o operador embedded escolher.

### ISSUE 5 — `set_state` em key inexistente = `Err`, contrato não documentado; tombstone-then-delete é dança manual do consumidor
- **Tipo:** API/contrato.
- **Evidência:** para o forget cognitivo (s410m) o OS precisa de: tombstone lógico (`Superseded`) ANTES do delete físico — se o delete físico rodar primeiro, o tombstone falha (doc não existe mais) e o mesh **ressuscita** a memória no próximo sync. O consumidor orquestra 2 chamadas com ordem crítica + interpreta `is_ok()`. Além disso `delete(key) -> bool` não diz por que falhou (não existia vs storage down).
- **Proposta:** `Sgdb::forget(key, reason) -> Result<ForgetOutcome>` que faça a sequência atômica (tombstone → delete → elo de auditoria FORGET) e devolva `(existed, tombstoned, deleted, audit_seq)`. O upstream `7e939f0` já tem `audit_forget`; falta a composição com tombstone+delete num único call-site canônico.

### ISSUE 6 — Hash-chain de auditoria com FNV-1a é forgeável por quem escreve na storage; e o campo `digest` tem semântica diferente por op
- **Tipo:** segurança / contrato.
- **Evidência:** ADR-0006 diz "crypto é seam" — ok, mas o efeito prático é que a chain `sys/audit/` detecta corrupção acidental, não adversário (FNV-1a sem chave: quem escreve storage recalcula prev_hash). O OS mantém um SEGUNDO audit trail (SHA-256 + Ed25519, `k_ai::audit`) exatamente por isso — hoje existem DUAS verdades de auditoria. E no `audit_forget` (7e939f0) usamos o campo `digest` para o hash do `reason`, enquanto em CHECKPOINT ele é o digest do estado — semântica por-op não documentada (decode/verify não distinguem).
- **Proposta:** (a) trait `Hasher`/`Signer` seam com impl SHA-256/Ed25519 opt-in (o OS injeta o seu); (b) documentar semântica de `digest`/`snapshot` POR OP num enum `AuditPayload::{Checkpoint{..}, Rollback{..}, Forget{sk, reason}}` em vez de campos sobrecarregados; (c) API de leitura: `audit_entries(since_seq, limit)` e `audit_for_key(sk)` — hoje `audit_verify` só resume contagens, e o consumidor que quer a trilha de UMA memória precisa scanear e decodificar tudo.

### ISSUE 7 — Ops de auditoria não cobrem o ciclo cognitivo: `resolve_conflict`, `feedback`, `decay`, `import` não deixam elo
- **Tipo:** cobertura de auditoria.
- **Evidência:** a chain registra CHECKPOINT e ROLLBACK (e agora FORGET, 7e939f0). Mas a decisão HITL mais importante — `resolve_conflict(conflict_id, winner_vid)` — não deixa rastro na chain do core (o OS audita no lado OS, AUDIT_TRAIL). Um auditor externo que só vê a storage do SGDB não sabe QUEM decidiu o vencedor nem quando.
- **Proposta:** ops `AUDIT_OP_RESOLVE` (com conflict_id + winner), `AUDIT_OP_DECAY`/`AUDIT_OP_FEEDBACK` agregados (batch por checkpoint), `AUDIT_OP_IMPORT` (proveniência de replicação). Manter chain enxuta: agregação por checkpoint para ops de alta frequência.

---

## Bloco 2 — API / contratos para consumidor máquina

### ISSUE 8 — `SgdbError` é string; o consumidor IA não consegue branchar
- **Tipo:** API.
- **Evidência:** `SgdbError::Invalid(&'static str)`/`Storage(&'static str)` com mensagens em inglês livre ("key contains # or NUL", "oob", "no ckpt"). Um agente AI decidindo retry/fail-closed/escalada HITL precisa de códigos: `ErrorCode::KeyRejected | Capacity | Corrupt | NotReady | ConflictResolutionFail | ...`. Hoje ou fazemos string-match frágil ou tratamos tudo como "down".
- **Proposta:** enum de códigos estável (com `#[non_exhaustive]`), mantendo as strings como `Display`. Breaking change de minor: aceitável agora, caro depois.

### ISSUE 9 — `MemoryLayer` não parseia string (`as_str` sem `FromStr`/`TryFrom`)
- **Tipo:** API menor, atrito real.
- **Evidência:** o OS exporta frames NMD1 e parseia layer de storage keys com `MemoryLayer::from_u8(lb[1] - b'0')` manualmente (s410l) porque só existe `as_str()`. Roundtrip `"L4" -> L4Semantic` deveria ser uma linha.
- **Proposta:** `impl FromStr for MemoryLayer` (aceita "L4" e "L4Semantic") + `TryFrom<&str>`. Zero custo, elimina parsing duplicado em todo consumidor.

### ISSUE 10 — NMD1 não é frameável: consumidor inventou `[len u32le][NMD1]` no wire do mesh
- **Tipo:** wire format / interop.
- **Evidência:** NMD1 não carrega tamanho total; para replicar N docs num blob (mesh CRDT, s410l) o OS definiu framing `[len u32le][payload]` com política fail-stop (len absurdo/truncado = para a iteração). Esse contrato agora vive só no kernel — qualquer outro consumidor (host tool, segundo OS) vai inventar outro framing e os blobs não interoperam.
- **Proposta:** canonicalizar no crate: `FrameWriter`/`FrameIter` (encode/decode de stream de NMD1) com política documentada de corrupção (fail-stop vs resync) e golden tests de truncamento no MEIO e no FIM. O wire do mesh vira contrato do crate, não do kernel.

### ISSUE 11 — `conflicts()` sem filtro/paginação; resolução sem trilha de decisão no core
- **Tipo:** API.
- **Evidência:** o OS precisa de `open_conflicts` e de contagem para HUD/health gate — hoje filtra em memória no bridge (`filter(|c| c.open)`). Com muitos conflitos acumulados (mesh com relógios concorrentes gera ConflictBoth), `conflicts()` clona tudo.
- **Proposta:** `conflicts_open()`, `conflicts_count_open()`, paginação `conflicts_since(id, limit)`; `resolve_conflict` devolvendo `ResolveOutcome { imported, superseded: Vec<vid> }` para o consumidor saber o que virou parent sem re-scan.

### ISSUE 12 — Recall lexical dependente de companion text implícito: KV cru entra no índice mas não é encontrável sem termos no payload
- **Tipo:** comportamento não documentado / melhoria.
- **Evidência:** `import_record` de `sys/net_config` com payload `b"mode=slirp"` NÃO aparece no `recall_lexical("net_config")` (o companion/lexical indexa o texto do payload, não a key). Só aparece se o valor contém o termo. Para o consumidor IA que busca por NOME de key (ops recall: "qual o net_config?"), o índice deveria cobrir a key.
- **Proposta:** indexar a key (segmentos) no lexical de docs importados/operacionais, ou opção `RememberOptions::index_key: bool`. E documentar explicitamente o que gera companion lexical hoje (`remember_text*` sim; `import_record`/`put` só payload-texto).

### ISSUE 13 — `remember_fact` fixa L3; família `remember_*` tem assinaturas assimétricas
- **Tipo:** API.
- **Evidência:** `remember_fact(fact, now)` grava sempre em L3EpisodicLong; `remember_semantic(key, text, emb)`; `remember_text_with(key, text, opts)`. O OS mantém `sync_fact_to_nsgdb` thin-wrapper só para não espalhar layer errada.
- **Proposta:** unificar em `remember(RememberRequest)` (layer explícita obrigatória, opts completos) mantendo os helpers como sugar. Menos superfície para o consumidor decorar.

---

## Bloco 3 — Performance / escala

### ISSUE 14 — `scan_prefix` materializa tudo (`Vec<(Vec<u8>, Vec<u8>)>`): OOM futuro garantido com o crescimento do volume cognitivo
- **Tipo:** escala.
- **Evidência:** o nsgdb_init do OS faz `scan_prefix("md/")` só para CONTAR records — carrega todos os valores para descartar. O ART já é paged no IDX2; o scan cru não.
- **Proposta:** `scan_prefix_paged(prefix, cursor, limit) -> (Vec<...>, next_cursor)` e `count_prefix(prefix)` (só índice). O consumidor embedded nunca deveria carregar o volume inteiro para iterar.

### ISSUE 15 — Granularidade de 512B por record desperdiça media para records pequenos (payload típico de KV < 64B)
- **Tipo:** otimização.
- **Evidência:** `record_size` arredonda para setor de 512 (contrato flash). Um volume com 10k keys operacionais de ~50B usa 5MB onde 0,5MB bastam — e o HIGH_WATER (256KB) dispara GC com live-set real 20× menor, agravando a ISSUE 4.
- **Proposta:** sub-sector packing (N records por setor com header de slot) OU página de journal com múltiplos records + checkpoint. Alternativa conservadora: tier pequeno-grande (records < 64B em arena compacta, grandes direto em setor).
- **Nota:** medir o trade-off com o bench de put_many existente; o ganho duplo (media + menos GC) pode ser 5-10× para workloads operacionais.

### ISSUE 16 — Fast-mount via IDX2 deixa o BQ vazio ("até o primeiro reinforce"): recall semântico degradado silenciosamente pós-boot
- **Tipo:** telemetry / honestidade.
- **Evidência:** documentado no próprio `nsgdb_init` do OS. Para um AI consumer, "semantic recall retorna vazio" é indistinguível de "não sei" — violação de honestidade de recall (a IA pode responder baseada em ausência de evidência).
- **Proposta:** (a) metric/marker `recall_degraded_reason= bq_unmounted` exposto no Hit/health; (b) auto-reinforce de boot (re-quantize top-N docs do lexical em background) em vez de esperar o primeiro reinforce orgânico.

---

## Bloco 4 — Testes / CI / DX

### ISSUE 17 — Testes acoplados a statics globais (FLASH/TICKV): ordem de execução importa, flake ordinal no host
- **Tipo:** DX/testes.
- **Evidência:** no crate, tests de bench assumem flash ≥ 1MB e dependem do estado que o teste anterior deixou (o OS vivenciou o flip: teste novo com RamFlash 256KB quebrou bench pré-existente com `oob` — SESSION_410 s410m-c). Os 400 testes passam hoje porque a ordem atual é benigna; qualquer teste novo que instale media menor pode quebrar outros.
- **Proposta:** fixture `isolated_storage(size) -> StorageGuard` que instala E RESTAURA o backend default (guard/RAII), padronizada nos testes do crate; CI com `--test-threads=1` e também embaralhado (`--shuffle`) para expor dependência de ordem.

### ISSUE 18 — CI não roda o `wire_fuzz`/property tests nem os benches smoke; decode bounds-checked merece fuzz contínuo
- **Tipo:** CI.
- **Evidência:** `wire_fuzz.rs` existe no src; decode de NMD1/AUD1/MDM1 é bounds-checked (ótimo), mas sem fuzz no CI a garantia é por inspeção. Interop TKLV agora tem golden bidirecional (via kernel) — o equivalente upstream deveria viver no repo do crate.
- **Proposta:** job CI com `wire_fuzz` (N minutos de budget), golden vectors de encode/decode das 3 formats (NMD1/MDM1/AUD1) e um teste de interop interno (o `scan_volume` já tem paridade com o TickvLite do kernel — trazer os vetores para dentro).

### ISSUE 19 — Falta CHANGELOG por API e semver discipline em features novas
- **Tipo:** governança.
- **Evidência:** 1.2.0/1.2.1 entregaram batch/dedup/IDX2/delete O(1) — ótimo — mas o consumidor descobre as APIs pelo código. Para o consumidor AI que atualiza via path-dep pinado no kernel, um `CHANGELOG.md` com "novas APIs / mudanças de contrato / deprecated" por release economiza um audit de diff por upgrade.
- **Proposta:** CHANGELOG.md canônico + `#[deprecated]` real nos caminhos que devem morrer (ex.: `put` para writes operacionais, ISSUE 2).

---

## Bloco 5 — Visão (o que a IA-consumidora quer do motor de memória em 2.0)

### ISSUE 20 — "Cognitive ops" como cidadãs de primeira classe (com audit + CRDT)
- Forget/Resolve/Reinforce/Decay são hoje verbos espalhados (`set_state`+`delete`, `resolve_conflict`, `reinforce`, `expire_old`). A proposta: um trait `CognitiveOps` com outcomes ricos e trilha de auditoria integrada (ISSUE 5/7), para que a decisão HITL do OS vire uma chamada com resultado explicável — o motor guarda o PORQUÊ, não só o estado final. É o que falta para o `Sgdb` ser de fato a "memória de uma IA" e não um banco KV com CRDT.

### ISSUE 21 — Autoridade ponderada no merge (HITL > mesh-learned)
- Hoje o `MergePolicy::for_layer` decide por camada (multi-value/LWW) — correto. Falta uma dimensão de AUTORIDADE: memória gravada sob approval HITL (forget/resolve/knowledge curada) deveria dominar memória aprendida em mesh no empate causal. Sugestão: `MemoryMeta.authority: u8` (0 = learned, 255 = HITL-approved) viajando no record; LWW desempata por authority antes do timestamp. Sem isso, um peer comprometido (ou apenas barulhento) pode ecipar decisão humana em L2/L3.

### ISSUE 22 — Explain por hit (proveniência completa, 1 chamada)
- `Hit` já carrega matched_terms/provenance. Para o consumer AI que precisa justificar resposta (ADR-0106 Decision contract), falta `explain_hit(hit) -> Explanation { version_dag, audit_refs, authority, clock, confidence_history }`. A peça existe (meta + chain + lineage) — falta a composição.

### ISSUE 23 — Snapshot incremental + delta export para sync barato
- `export_record(key)` é unidade de replicação (bom). Falta `export_delta(since_seq/since_version, max) -> Vec<MemoryRecord>` puxando do log/índice — o OS hoje scan-e-filtra. Com keys_for_clock + delta export, o anti-entropy vira pull direcionado barato em vez de full-scan periódico.

### ISSUE 24 — Reconhecer e preservar o que já é excelente
Para equilíbrio do relatório: delete O(1) reverso (1.2.1), CRDT com MergePolicy por camada (nunca LWW cego — o ADR-0081 C4 do kernel depende disso), IDX2 fast-mount, put_many_raw para replicação em massa, decode bounds-checked em toda parte, e a paridade byte-exata de encode/decode com o TickvLite do kernel (a interop s410i passou de primeira, incluindo tombstone in-place e CRC) são diferenciais reais. O roadmap sugerido aqui não substitui nada disso — completa o ciclo cognitivo em volta.

---

## Priorização sugerida (opinião do consumidor)

| # | Issue | Impacto p/ IA em Ring 0 | Custo estimado |
|---|-------|--------------------------|-----------------|
| 1 | #5 forget atômico + #7 audit resolve/import | fecha o ciclo cognitivo HITL (segurança) | M |
| 2 | #1 VectorClock vacuous + #8 error codes | remove classes inteiras de bug do consumidor | S |
| 3 | #3 compact bounds + #4 histerese GC | estabilidade em produção bare-metal | M |
| 4 | #2 put_operational + #9 FromStr layer + #12 key-index | correção de rota de writes | S |
| 5 | #10 framing canônico + #23 delta export | mesh/anti-entropy honesto e barato | M |
| 6 | #14 paged scan + #15 sub-sector packing | escala (quota de flash é real em HW) | L |
| 7 | #17 fixtures isoladas + #18 fuzz CI | DX/confiança contínua | S |
| 8 | #16 recall degraded marker + #22 explain | epistemologia da IA (só responder com evidência sã) | M |

**Assinatura:** gerado pelo agente AI do neural-os-core a partir de uso real em bare-metal (kernel `no_std` bootando com o crate, mesh P2P 2 nós, forget HITL end-to-end). Cada evidência tem commit/SESSION referenciado no repositório consumidor (`msrovani/neural-os-core`, docs/memory/SESSION_410.md adendos s410i–s410m-c, SESSION_413).
