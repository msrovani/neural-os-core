# SESSION 022 — Block 5: Skills + Trust Cache + ISO

**Data:** 2026-06-23  
**Versão:** v0.17.0  
**Sprint:** 22 (Block 5)

## Objetivo

Implementar TrustCache, atualizar SystemStatusSkill para consumir MHI, criar HardwareInfoSkill, e refinar detecção de arquitetura.

## Progresso

### TrustCache (`crates/skill-registry/src/trust_cache.rs`)

- `TrustEntry` struct: token, granted_at (timer ticks), ttl_ticks
- `TrustCache` struct: BTreeMap<u64, TrustEntry>, AtomicU64 next_id
- `grant(token, current_ticks, ttl_override)` — insere com TTL
- `revoke(token)` — remove entry
- `is_trusted(token, current_ticks)` — verifica se não expirou
- `DEFAULT_TTL_TICKS = 1800` (~100s a 18.2 Hz)
- Exportado via `crates/skill-registry/src/lib.rs`

### SystemStatusSkill upgrade

- Agora chama `memory::global_hardware_context()` que retorna `[ratio, allocated_count]`
- Consome `mhi::MemoryHierarchy::new()` para exibir RAM por tier
- `hardware_context_tensor()` modificado: `[1]` agora retorna `allocated_count` (antes `0.0`)

### HardwareInfoSkill

- Nova skill: lê `GLOBAL_ARCH` (SystemArchitecture guardado pós-boot)
- Reporta CPU cores, GPU status, heap size, power mode, MHI tiers/bandwidth
- Registrada no SKILL_REGISTRY junto com EchoSkill e SystemStatusSkill

### GLOBAL_ARCH

- `lazy_static! { static ref GLOBAL_ARCH: spin::Mutex<Option<SystemArchitecture>> }`
- Populado após `SystemArchitecture::infer()` no boot flow
- Acessível por skills e daemons

### Boot flow

- `[ARCH]` agora loga contagem de PCI devices

## Dificuldades e Correções

1. **`hardware_context_tensor()[1]` inútil** — estava fixo em 0.0. Alterado para retornar `allocated_count as f32`.
2. **TrustCache sem acesso a TIMER_TICKS** — o crate skill-registry não pode importar `crate::interrupts::TIMER_TICKS` do kernel. Solução: passar `current_ticks` como parâmetro em vez de usar extern function.

## Resultados

- `cargo check --release`: ✅ 0 errors, 17 warnings (todos pre-existentes)
- `cargo bootimage --release`: ✅ Bootimage criado
- QEMU boot: ✅ 6 tasks, pipeline completo, novas skills registradas

## Arquivos

| Arquivo | Ação |
|---|---|
| `crates/skill-registry/src/trust_cache.rs` | Criado |
| `crates/skill-registry/src/lib.rs` | Modificado |
| `crates/neural-kernel/src/main.rs` | Modificado |
| `crates/neural-kernel/src/memory.rs` | Modificado |
| `Cargo.toml` | v0.16.0 → v0.17.0 |
| `CHANGELOG.md` | Modificado |
| `docs/memory/STATE.md` | Modificado |
| `docs/memory/SESSION_022.md` | Criado |
| `AGENTS.md` | Modificado |
