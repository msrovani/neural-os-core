# SESSION_224: ADR-0076 Implementação Pesada — 23 entregas

**Data:** 2026-07-27  
**Foco:** Implementação completa das ondas ADR-0076 (Cross-OS Ecosystem) + Rename JARVIS→JARBAS  
**Workspace:** neural-os-core v1.9.99-adr0076  
**Build:** `cargo check --release` = 0 erros

---

## Resumo

Implementação massiva de 23 entregas do plano ADR-0076, cobrindo Skill Manifest, WASM runtime, telemetria, segurança, agentes, syscalls, Ring-3, proof-gated mutations, kernel HNSW, e rename global JARVIS→JARBAS.

## O que foi feito

### Ondas 1-8 completas
- Skill Manifest FYY canônico com parser, 12 testes, 25 manifests de agentes nativos
- WASM: host functions 1→6, WASI Preview 1, WAT test suite (18 testes)
- Telemetry ring SPSC 4096 slots + shell trace cmd
- Membrane two-layer gate + Permission Gate com HITL
- Live capsule lifecycle + Cascading capability revoke + Goal-aware scheduler
- Intent Bus + Glass Box inspect + Syscalls 13→9 + GEMM benchmark

### Itens de linha de produção
- SYS_MAP_FB real (page table walk no syscall dispatch)
- Proof-gated mutations via ruvix-proof crate
- Kernel HNSW via ruvix-vecgraph crate (patches no_std)
- Ring-3 Userspace: ELF loader + ProcessManager + SYS_DEMAND_PAGE + TRY_ENTER_RING3=true

### Renomeação
- JARVIS → JARBAS (Just Another Really BADASS Intelligence System)
- 16 arquivos modificados: todos os structs (JarvisEngine→JarbasEngine, etc.), strings, constantes

## Lições Críticas Aprendidas

### 1. Fixers paralelos sobrescrevem lib.rs
Múltiplos fixers concorrentes sobrescrevem `lib.rs` e perdem `pub mod` declarations de módulos criados por outros fixers. **Sempre verificar** se módulos como `membrane`, `native_agents`, `telemetry` ainda estão declarados após qualquer execução de fixer. Padrão: `grep "pub mod (membrane|native_agents|skill_manifest|wat_tests|intent_bus|telemetry)"` no lib.rs de cada crate.

### 2. ruvix-vecgraph precisa de patches no_std
A crate `ruvix-vecgraph` publicada no crates.io tem:
- `f32::sqrt()` em módulo SIMD que não compila em no_std — precisa gate `#[cfg(feature = "std")]`
- Falta `use alloc::vec::Vec` em alguns módulos
- Dependências `ruvix-types` e `ruvix-region` puxam `std` se `default-features = true`

Solução: patches locais em `patches/` + `[patch.crates-io]` no workspace Cargo.toml.

### 3. Merge conflicts em arquivos de configuração
O `rust-toolchain.toml` e `wasm_build.rs` tiveram diff markers (`<<<<<<< HEAD`) deixados por fixers. **Sempre verificar** se há diff markers após operações paralelas:
```
grep -r "<<<<<<< HEAD" crates/
grep -r ">>>>>>> " crates/
```

### 4. AgentTickResult::Continue não existe
O enum `AgentTickResult` em `agent-core/src/lib.rs` tem apenas `Pending | Done | Crashed`. Um fixer adicionou `AgentTickResult::Continue` que não compila. **Usar `Pending`** para agentes que continuam rodando.

### 5. `pub use` de macros entre crates
`pub use k_nano::{kjson, klogc}` re-exporta macros de `#[macro_export]`. Se k_nano não compila, as macros ficam indisponíveis em neural-kernel, causando erro em cascata. **Sempre compilar k_nano primeiro** antes de neural-kernel.

### 6. unsafe blocks omitidos em allocator
Fixers adicionaram chamadas a funções `unsafe` (`SlabAllocator::init`, `Talc::claim`) sem bloco `unsafe {}`. **Sempre verificar** que chamadas unsafe estão dentro de `unsafe { }` ou função `unsafe`.

### 7. CapabilityToken::Ed25519 espera IdentityPayload
`CapabilityToken::Ed25519` em `event-bus/src/capability.rs` espera `IdentityPayload { public_key: [u8; 32], signature: [u8; 64] }`, não `[u8; 64]`. Para tokens simples, usar `CapabilityToken::Legacy(1)`.

### 8. PowerShell -replace é regex
`$content -replace $old, $new` no PowerShell trata `$old` como REGEX, não string literal. Padrões com `.*` ou caracteres especiais corrompem strings. **Usar `.Replace()` method ou escapar** para substituições literais.

## Arquivos Modificados

| Tipo | Arquivos |
|------|----------|
| Novos | `kernel_hnsw.rs`, `proof_gate.rs`, `elf_loader.rs`, `process.rs` |
| Modificados | `skill_manifest.rs`, `wasmi_rt.rs`, `wasi_host.rs`, `wat_tests.rs`, `telemetry.rs`, `membrane.rs`, `permission_gate.rs`, `quarantine.rs`, `intent_bus.rs`, `shell.rs`, `package_hub.rs`, `native_agents.rs`, `syscall.rs`, `user_mode.rs`, `address_space.rs`, `allocator.rs`, `agent-core/src/lib.rs`, `agents.rs` |
| Renomeados | 16 arquivos com JARVIS→JARBAS (proactive.rs, soul.rs, jarvis.rs, audio/jarvis.rs, audio/voice.rs, display/agent.rs, display/avatar.rs, display/compositor.rs, etc.) |
| Patches | `patches/ruvix-vecgraph/`, `patches/ruvix-region/` |
| Docs | `CHANGELOG.md`, `STATE.md`, `0076-cross-os-ecosystem.md`, `INDEX.md` |

## Próximos Passos (ADR-0076 §7)
- SYS_PIN_DMA real — após Ring-3 estável com DMA isolado 🔗
- Testar boot QEMU: Jarbas OK, Desktop OK, soft power off OK
