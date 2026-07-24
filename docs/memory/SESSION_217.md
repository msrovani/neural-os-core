# SESSION_217 — E1a Crate Promotion + P001/Boot/Checkpoint/Drift (24 Jul 2026)

## Objetivo
Promover cortex crate (E1a), corrigir k-nano drift, P001 SKILL_REGISTRY shadow, boot path validation, expandir SelfHeal checkpoint.

## Escopo

### E1a — Cortex Crate Promotion (69 erros → 0)
- Funções bin-specific (`generate_via_model`, `dispatch_expert`, `dispatch_hw_control`) movidas do crate p/ bin
- Path rewrites: `crate::ATA_DRIVER` → `k_nano::ATA_DRIVER`, `crate::fat32::` → `k_nano::fat32::`, etc.
- 6 funções argmax u16→u32 (BPE API)
- `gguf_streaming` removido do crate (net-dependent); stub no bin atualizado
- `generate_via_model` + `generate_via_model_with_decoder` adicionados ao cortex crate (s/ Trinity routing)
- `CURRENT_MODEL` feito `pub` no cortex crate
- k-hal xpu.rs u16→u32 token types

### P001 — SKILL_REGISTRY Shadow Fix
- **Bug**: main.rs tinha `static ref SKILL_REGISTRY` privado com 13 skills reais — shadowing o `pub` de k_nano (stub vazio)
- **Fix**: Removido o `lazy_static!` privado; criado `register_builtin_skills()` que registra as 13 skills no `k_nano::SKILL_REGISTRY`
- 4 referências `crate::SKILL_REGISTRY` → `k_nano::SKILL_REGISTRY`
- Imports em agents.rs corrigidos

### Boot Path — Agency + Safety I4
- **Agency fallback**: `register_agency_agents` cria 2 AgentSpecs (SystemDiagnostics, HwMonitor) quando PACKAGE_HUB.agency_specs() vazio
- **Safety I4 Merkle**: `verify_counter` em SafetyAgent; a cada 100 ticks chama `AUDIT_TRAIL.lock().verify()` + `.entry_count()` e loga resultado
- **AuditTrail::entry_count()** adicionado em k_ai/audit.rs (pré-requisito)

### Checkpoint SelfHeal — Expandir
- 5 novos campos no `Checkpoint`: `heap_start`, `heap_size`, `page_table_pml4_addr`, `driver_state_hash`, `checkpoint_version`
- `save_checkpoint()`: captura CR3/PML4 (via `x86_64::registers::control::Cr3::read()`), heap addr fixo (0x_4000_0000_0000), FNV-1a hash de ATA+E1000 init flags
- `restore_checkpoint()`: log diagnóstico com heap/PML4/driver hash — ainda não restaura page tables (P09 pendente)
- `checkpoint_version = 2`

### k-nano Drift Quick Wins
- **env.rs**: `is_online()` movido de bin p/ k_nano (verifica E1000/RTL8139/VIRTIO_DEV `.is_some()`); bin virou `pub use k_nano::env::*`
- **block_dev.rs**: `impl BlockDevice for UsbMassStorage` mantido no bin (tipo local difere de k_nano — usa `crate::xhci::BulkEndpoint` que não é o mesmo)
- **Lição**: Não mover impl de trait para crate quando o tipo struct também tem drift — o tipo no bin é diferente do do crate

### Cross-crate Fixes (hermes + neural-kernel)
- hermes/agents.rs: `ToString` import, `k_ai::gguf`→`cortex::gguf`, Scrape arm `AgentTickResult` fix (Result<String,String> em vez de `return String` de fn `AgentTickResult`)
- neural-kernel/agents.rs: `ToString` import, Scrape handler stub
- neural-kernel/cortex.rs: `use alloc::string::String`
- neural-kernel/gguf_streaming.rs: 3 stubs adicionados (`is_http_model_spec`, `hot_swap_from_net`, `hot_swap_from_ata`)

## Verificação
`cargo check --release --target x86_64-unknown-none` → **0 erros** em todas as crates (boot crate não conta — host tool, não no_std)

## Arquivos Modificados (20)
- crates/cortex/src/cortex.rs, lib.rs, bpe.rs, model_hub.rs, ngram_spec.rs
- crates/k_hal/src/gpu/xpu.rs
- crates/k_nano/src/env.rs, block_dev.rs
- crates/k_ai/src/self_heal.rs, audit.rs, lib.rs (gguf.rs removido)
- crates/hermes/src/agents.rs, safety.rs, hermes.rs, skill_loader.rs
- crates/neural-kernel/src/main.rs, agents.rs, cortex.rs, env.rs, block_dev.rs, gguf.rs, gguf_streaming.rs, bpe.rs, model_hub.rs
- TODO.md

## Lições Críticas
1. **SKILL_REGISTRY shadow**: `lazy_static! { static ref }` privado no bin shadowing `pub` de crate = silenciosamente invisível p/ outras crates. Sempre usar o singleton da crate base.
2. **Drift de tipo struct**: Não mover `impl Trait for Struct` para crate se o bin tem sua própria cópia do struct — tipos diferentes não compartilham impls. Verificar com `fc.exe /A` antes.
3. **`return` em match arm**: `return String` dentro de fn que retorna `AgentTickResult` = erro de tipo. Usar `Result<String,String>` para early-exit dentro de match arm.
4. **`ToString` em no_std**: Importar explicitamente `use alloc::string::ToString;` — não está no prelude.
5. **Boot path Agency**: Agency specs vazio quando sem AGENT.md assinados — fallback com 2 specs básicos garante >0 agentes no boot log.
