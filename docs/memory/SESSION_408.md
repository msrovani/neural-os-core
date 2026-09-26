# SESSION_408 — ADR-0110 item código-seguro: `TaskType::LayaIntent = 8`

**Motivo:** implementar o único item de código aprovado da ADR-0110 revisada (§2.2/§9.2). Mapa de integração via @explorer (exp-4); execução via @fixer (fix-1); reconciliação pelo orquestrador.

## Mudança (1 arquivo, +14/−0)
- `crates/k_nano/src/net/noproto.rs:60`: `LayaIntent = 8` após `Shutdown = 7` (`TaskType` repr(u8)); teste `laya_intent_discriminant_and_header_size` (Shutdown==7, LayaIntent==8, header==37B).
- Proibições respeitadas: sem `k_ai::laya`/scorer, sem toque em SecurityAgent/detectores/correlate, sem reuso de `Intent` do chat, sem produtores/consumidores — variante ignorada por padrão, zero mudança de fio.

## Verificação
- `cargo test -p k-nano --lib noproto` → **ok, 1 passed** (target isolado `target/adr110`).
- `cargo check -p k-nano --release` → **0 erros** (14 warnings pré-existentes).
- Suite lib completa do k-nano aborta em harness (`STATUS_PRIVILEGED_INSTRUCTION`) — **pré-existente**, fora do escopo (testes executam instrução privilegiada no host Windows).
- Full workspace `cargo check --release` não rodado (proporcional: enum aditivo + sem matches exaustivos no repo).

## Não implementado (bloqueado, honesto)
- Hook de triagem no `correlate()`: sem tipo de decisão do domínio de segurança (`cortex::Intent` é do chat, sem Malicious/Normal) e sem scorer — mapear `Decision→SecurityAlert` é indefinido. Bloqueado por IDEA #611 (dataset+gate) + tipo novo.
- Lifecycle ADR-0110 permanece PROPOSED/`por_fazer` (sem evidência de classificador).
- `Cargo.lock`: churn de 1 linha (neural-sgdb path-dep 1.1.28→1.2.1) causado pelo próprio cargo durante validação — ambiental, mantido e declarado.
