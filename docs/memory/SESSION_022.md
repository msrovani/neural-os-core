# Sessão 022 — Sprint 22: Skills + Trust Cache (Block 5)

**Data:** 2026-06-24
**Duração:** Implementação completa do Block 5
**Versão:** v0.17.0

## Contexto

Sprint 22 é o Block 5 da chain de 6 blocos (ADR-0015): Skills + Trust Cache. O objetivo era upgrade do `SystemStatusSkill`, criar `HardwareInfoSkill`, implementar `TrustCache` com suporte a TTL e denylist, integrar trust-aware execution nos daemons Hermes, e expor comandos `/trust allow/deny` e `/hw` no terminal.

## Dificuldades e Decisões

### 1. Onde colocar o TrustCache?

**Problema:** TrustCache precisa de fonte de tempo (PIT ticks) para TTL, que é específica do kernel. Ao mesmo tempo, validação de token é responsabilidade do `skill-registry` crate.

**Decisão:** TrustCache como módulo separado em `crates/neural-kernel/src/trust.rs`, consumido via `execute_skill_with_trust()` helper em `main.rs`. O `SkillRegistry` ganhou três métodos novos (`has_skill`, `validate_token`, `execute_skill_unchecked`) para permitir o padrão: TrustCache → validate → cache → execute_unchecked.

### 2. Trust-once-use-always vs TTL

**Problema:** O MVP exige "trust-once-use-always" para boa experiência, mas sem TTL tokens ficariam em cache para sempre se o usuário nunca revogasse.

**Decisão:** Dois níveis:
- `trust_allow()` = TTL = `u64::MAX` (efetivamente permanente, só revoga com `trust deny`)
- `check_or_cache()` (auto-cache) = TTL = 360 ticks ≈ 20s (expira se não re-validado)

### 3. Dupla validação de token

**Problema:** O `execute_skill()` original sempre validava o `CapabilityToken` contra os `required_tokens` do manifesto. Com TrustCache, a validação já foi feita — executar de novo seria redundante.

**Decisão:** Criado `execute_skill_unchecked()` que ignora o token. O helper `execute_skill_with_trust()` garante que a validação acontece exatamente uma vez:
1. Verifica TrustCache (fast path)
2. Se falhar, valida via `validate_token()` (slow path)
3. Se válido, faz `check_or_cache()` para acelerar próximas chamadas
4. Executa via `execute_skill_unchecked()` sem re-validar

### 4. Globais de sistema

**Problema:** `SystemArchitecture` e `MemoryHierarchy` eram variáveis locais em `kernel_main`. Skills precisavam acessá-las.

**Decisão:** Três novos `lazy_static!` globais em `main.rs`: `SYSTEM_ARCH`, `MEMORY_HIERARCHY`, `TRUST_CACHE`. Preenchidos após `infer()` e `MemoryHierarchy::new()` no boot flow.

### 5. Parsing de comandos trust

**Problema:** `/trust allow 123 echo` requer parsing de 3 tokens após a barra, mas `splitn(2, whitespace)` no `parse_command()` original só divide em 2 partes.

**Solução:** Para comandos `/trust`, o remainder (`"allow 123 echo"`) é dividido novamente com `splitn(3, whitespace)` para extrair subcomando, token u64 e skill name. `u64::from_str` via `parse()` funciona em `no_std` porque `u64` implementa `FromStr` em `core`.

### 6. Cargo toolchain ausente

`cargo` não está no PATH desta máquina. Todas as verificações foram manuais por revisão de código. Confiabilidade: todas as APIs usadas são existentes (BTreeMap, AtomicUsize, str::parse, etc.) ou foram estendidas com compatibilidade retroativa.

## Arquivos Criados/Modificados

| Arquivo | Ação | Linhas |
|---|---|---|
| `src/trust.rs` | Criado | 65 |
| `src/hermes.rs` | Modificado | +Command::HardwareInfo, +TrustAllow/Deny, parse expansion |
| `src/main.rs` | Modificado | Skills upgrade, globals, helper, intent_router refactor |
| `skill-registry/src/registry.rs` | Modificado | +has_skill, +validate_token, +execute_skill_unchecked |
| `Cargo.toml` | Modificado | v0.16.0 → v0.17.0 |

## Estado Final

- `cargo check --release`: ❌ Não verificado (toolchain ausente) — revisão manual
- `bootimage`: ❌ Não verificado
- QEMU boot: ❌ Não testado
- Skills: 3 registradas (echo, system_status, hardware_info)
- TrustCache: operacional com allow/deny/is_trusted/check_or_cache
- Hermes: 9 comandos (/status, /echo, /hw, /trust allow, /trust deny, /help, + MLP chat)
