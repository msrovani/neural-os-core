# SESSION_245 — Auditoria Técnica: Segurança 6.1–6.4 (modelo de confiança)

## Problema
Auditoria técnica (relatório externo) encontrou 4 lacunas de segurança no modelo
de confiança documentado:

- **6.1** — Dois portões de validação de skills: `verify_artifact_md` (ADR-0052,
  rigoroso: schema/kind/seções/content_hash/assinatura/sandbox) e `verify_skill_md`
  (fraco: só nome/descrição/tokens/tamanho/injection). Skills auto-geradas entravam
  na frota pelo portão fraco, contornando o contrato documentado. "Portão rigoroso
  + porta lateral aberta = sem portão."
- **6.2** — Vocabulário de anéis R0–R3 sem correspondência com fronteira de
  segurança: tudo executa em Ring 0 (CPL=0). Docs não diziam isso.
- **6.3** — `CapabilityToken::Ed25519(_) => true` sem verificar nada (cerimonial);
  `Legacy(1)` era o tráfego real.
- **6.4** — `mix_session_seed()` misturava RDTSC + ticks de forma determinística —
  previsível no boot; chaves de sessão (mesh/TLS) derivam daí. Já existia
  `k_nano::hw_rng` (RDRAND + ChaCha20 fallback) não usado nesse caminho.

## Diagnóstico
- **6.1**: `verify_skill_md` (hermes/src/self_evolve.rs:164) era implementação
  separada e mais fraca. Os generators (`skill_gen`, `skill_observer`,
  `matrix_learn`, prompt LLM) emitiam frontmatter mínimo (name/desc/required_tokens).
  Seeds embedded (`skills/*/SKILL.md`, 4 arquivos) passavam pelo mesmo portão fraco.
  Fluxo `verify_and_register`: verify fraco → sign → register (assinatura adicionada
  DEPOIS da verificação).
- **6.3**: event-bus é crate leaf (só ticket-lock + libm) — não pode verificar
  Ed25519 sem dep nova; `IdentityPayload` carrega pk+assinatura sem mensagem
  vinculada (impossível verificar em is_valid).
- **6.4**: `hw_rng::HardwareRandom` (RDRAND primário, fallback ChaCha20) já existia
  em k_nano; `mix_session_seed` não o usava. Gate ADR-0082: `probe_done()` +
  `cpu_features().rdrand` antes de executar RDRAND.

## Fix
1. **6.1 — portão único ADR-0052**:
   - `verify_skill_md` agora **delega** para `verify_artifact_md(PackageKind::Skill,
     content)` → `VerifyVerdict`. Um contrato, uma implementação (schema 1, kind
     skill, name sanitizado, goal/contexto/acionaveis/tokens/provenance/
     sandbox_status, 7 seções `## `, injection patterns, content_hash, assinatura
     Ed25519 via `k_nano::identity::verify_trusted`).
   - `verify_and_register`: **SIGN FIRST** → verificação estrita do conteúdo selado
     → register. Fail-closed: se assinar falhar, sealed==raw e a verificação estrita
     rejeita.
   - Generators emitiam contrato completo: `skill_gen::generate_skill`,
     `skill_observer::generate_skill_md`, `matrix_learn` (string + register path),
     `llm_skill_prompt` (template instrui as 9 chaves + 7 seções).
   - Seeds embedded via **`register_trusted_skill`** (trusted-by-compilation,
     precedente SESSION_230) — parse sem gate de assinatura; `register_skill`
     virou verify estrito + `parse_and_store` privado.
   - Callers extras achados e corrigidos: `/learn` em hermes/src/agents.rs e
     neural-kernel/src/agents.rs (formato mínimo → contrato completo); LLM path
     no bin (pre-check em conteúdo cru removido → `verify_and_register` direto).
2. **6.2 — docs honestos**: AGENTS.md agora diz explicitamente que anéis R0–R3
   são **organização de código (camada lógica de dependência), NÃO fronteira de
   segurança do processador** — tudo roda CPL=0; isolamento efetivo = wasmi (A)
   + Ring3 gated (ADR-0077, não registrado). Aplicado em "Core Architecture" §3
   e no bloco K³CHJ Workspace Structure.
3. **6.3 — fail-closed**: `CapabilityToken::Ed25519(_) => false` com comentário
   (payload sem mensagem vinculada + crate leaf sem crypto → não verificável =
   inválido). Nada no código constrói Ed25519 hoje — sem regressão.
4. **6.4 — RDRAND**: `mix_session_seed` usa `hw_rng::HardwareRandom::fill_bytes`
   quando `probe_done() && cpu_features().rdrand` (gate ADR-0082), RDTSC+ticks
   viram stir secundário apenas. Fallback sem RDRAND: comportamento anterior.

## Verificação
- `cargo clean -p neural-kernel && cargo check --release`: **0 erros**
  (3 warnings pré-existentes: syscall.rs cast, ruvix-vecgraph CapRights,
  cortex/trinity unused String — fora do escopo).
- Gate novo é fail-closed por design: conteúdo com `..`, nova linha na description,
  nome fora do charset, ou seção `## Pre-Flight Verification` (agora exige
  `## Pre-Flight` exato) é rejeitado.
- **Boot QEMU TCG no-disk** (`-smp 2 -accel tcg`, sem disco de dados): fases
  completas (AgentFleet 54 + Runtime), scheduler vivo, `session_pk` gerada
  (RDRAND OK), seeds `signed=true (trusted-by-compilation)`, auto-skill PnP
  `verified+registered`. skill_writer continua rejeitada — comportamento
  pré-existente (contém "ignore all" no corpo; mesmo no boot pré-auditoria).

## Follow-ups achados na validação (corrigidos)
- **Bug latente do gate estrito (unquote)**: `sign_artifact_md` grava
  `signature: "hex"` **com aspas**; `check_signature_content` lia sem `unquote`
  → `parse_hex_sig` via 130 chars (com aspas) → rejeitava TODO artefato assinado
  (`missing_or_bad_signature`). O gate fraco antigo nunca exercitava assinatura,
  por isso o bug nunca aparecia. Fix: `unquote` na leitura da assinatura
  (consistente com `content_hash`). A auditoria citava o gate como "implementado"
  sem nunca tê-lo rodado end-to-end com artefato assinado — agora rodou.
- **Regressão GDT do f41aa03 (sessão concorrente)**: GDT do x86_64 0.14.13 é fixo
  em 8 slots; o refactor adicionava 1 code + 8 TSS (2 slots cada) = 17 slots →
  panic `gdt.rs:111` em todo boot. Fix: 1 TSS compartilhado (TSS_ARRAY[0]),
  design pré-f41aa03 com boot conhecido OK; ISTs per-AP permanecem alocadas.
  Boot pós-fix: 0 panic, Runtime completo.

## Lições
- **Portão duplicado mais fraco = sem portão.** Se existe verificação canônica
  (verify_artifact_md), qualquer caminho paralelo (verify_skill_md) deve delegar
  a ela — nunca reimplementar subconjunto.
- **Sign-then-verify vs verify-then-sign**: contrato exige content_hash+assinatura
  → assinar ANTES de verificar; senão o conteúdo cru falha o contrato.
- **Seeds embedded ≠ runtime content**: seeds são trusted-by-compilation (precedente
  SESSION_230), precisam de caminho próprio (`register_trusted_skill`) para o gate
  estrito não quebrar o boot.
- **Gate que rejeita por `..` no corpo inteiro**: `verify_artifact_md` rejeita
  conteúdo com `..` (path traversal check) — seeds com "..." (web_scrape/skill_writer)
  contornam via trusted path; conteúdo runtime com `..` é rejeitado (fail-closed).

## Pendências / observações (fora do escopo)
- `chauchy_fallback` do hw_rng usa constantes fixas (determinístico se RDRAND
  falhar) — fraqueza pré-existente.
- 6.3: `IdentityPayload` sem mensagem vinculada — se um dia Ed25519 for usado,
  precisa de challenge/message binding + verificação na camada que cria o token.
- 3 warnings pré-existentes (syscall.rs, ruvix-vecgraph, cortex/trinity).
- Working tree tinha mudanças concorrentes de outra sessão (main.rs NVMe/AHCI/ATA/
  USB-MSC) — commit da auditoria foi separado (só os 10 arquivos + docs).
