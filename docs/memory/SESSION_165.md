# SESSION_165 — ADR-0059 F3–F7 + Cleanup

**Data:** 2026-07-21
**Release:** v1.9.2 TEST (mesma leva — sem bump)
**Marco:** F3–F7 da ADR-0059 Runtime App Factory implementados; `cargo check --release` pendente.
**ADR:** [0059-runtime-app-factory.md](../architecture/0059-runtime-app-factory.md)

---

## Resumo

Implementação completa dos itens restantes da ADR-0059 (F3–F7 + limpeza) para o Runtime App Factory:

- **F3 (bridges):** `wasm.rs` → `wasmi_rt::run_wasm`; `WasmExecutor` removido (~150 LOC); `evolve.rs` hot-swap/rollback rewired
- **F4 (decode harness):** `decode_harness.rs` — reconhecedor de padrões (Add/Echo/Default) gera WASM → valida no wasmi
- **F5 (promote):** `DynamicSkill::with_wasm()` + `promote_skill_to_wasm()` chain completo
- **F6 (MicroPython.wasm):** `micropython_wasm.rs` → `wasmi_rt::run_wasm` + fallback stub dev
- **F7 (ring gate):** já existente — só atualização de doc
- **Cleanup:** headers de deprecação em `wasm_rt.rs`/`wasm_exec.rs`; `PackageSignature` em `package_hub.rs`

## Arquivos alterados

| Arquivo | Tipo | Descrição |
|---------|------|-----------|
| `crates/hermes/src/wasm.rs` | **Rewrite** | Bridge `WasmSkill` → `wasmi_rt::run_wasm`; remove `WasmExecutor` (~150 LOC) |
| `crates/hermes/src/wasmi_rt.rs` | **Edit** | Add `run_wasm()` (generic i32 executor) + `generate_wasm_module()` |
| `crates/hermes/src/evolve.rs` | **Edit** | Hot-swap/rollback rewired (wasmi sandbox + `DynamicSkill`); `Vec<Op>`→`Vec<u8>` |
| `crates/hermes/src/skill_opt.rs` | **Edit** | Add `promote_skill_to_wasm()` |
| `crates/hermes/src/micropython_wasm.rs` | **Edit** | `WasmExecutor`→`wasmi_rt::run_wasm`; fallback stub dev |
| `crates/hermes/src/decode_harness.rs` | **Novo** | F4 pattern recognizer + WASM generator |
| `crates/hermes/src/dynskill.rs` | **Novo** | Re-export `DynamicSkill` from skill-registry |
| `crates/hermes/src/package_hub.rs` | **Edit** | Add `PackageSignature` with `compute()` hash |
| `crates/hermes/src/wasm_rt.rs` | **Edit** | Deprecation header ADR-0059 |
| `crates/hermes/src/wasm_exec.rs` | **Edit** | Deprecation header ADR-0059 |
| `crates/hermes/src/lib.rs` | **Edit** | Add `pub mod decode_harness; pub mod dynskill;` |
| `crates/skill-registry/src/dynskill.rs` | **Edit** | Add `wasm: Option<Vec<u8>>` + `with_wasm()` |
| `docs/architecture/0059-runtime-app-factory.md` | **Edit** | Update acceptance criteria status |
| `docs/memory/SESSION_165.md` | **Novo** | Esta sessão |

## Detalhes das implementações

### F3 — Bridges (wasm.rs → wasmi_rt)

`WasmExecutor` (~150 LOC com 25+ opcodes) removido. `WasmSkill::execute()` agora:
1. Converte payload → argumentos i32 via `payload_to_args()`
2. Chama `wasmi_rt::run_wasm()` com o bytecode WASM real
3. Fallback de 0 args se `main()`/`_start()` não aceitar argumento

`evolve.rs`:
- `VersionEntry.bytecode`: `Vec<Op>` → `Vec<u8>` (bytecode WASM real)
- `hot_swap()`: testa no wasmi (`_start`/`main`) → registra como `DynamicSkill::with_wasm()`
- `rollback()`: restaura bytecode anterior via `DynamicSkill`
- `WasmOrigin` enum adicionado (Generated/Compiled/External)
- `WasmSkillRuntime` dependência removida

### F4 — Decode Harness (PONYTAIL)

`decode_harness.rs`:
- `SkillPattern` enum: Add, Echo, Default
- `recognize()`: prefix matching (add/sum/+, echo/print/say, etc.)
- `generate_from_pattern()`: gera WASM bytecode válido
- `generate_add_wasm()`: `(func (export "main") (param i32 i32) (result i32) ...)`
- `decode_and_generate()`: pipeline completo reconhece→gera→valida no wasmi
- `self_test()`: `add(3,5)=8` PASS

**Ponytail:** full WAT assembler (~800 LOC) postergado. Upgrade quando `wat` crate suportar no_std.

### F5 — Promote

- `DynamicSkill.wasm: Option<Vec<u8>>` + `with_wasm(name, desc, instr, wasm)` constructor
- `promote_skill_to_wasm()` em `skill_opt.rs`: gera bytecode → valida no wasmi → registra no `SKILL_REGISTRY`
- `promote_ephemeral_to_wasm()` em `evolve.rs`: simplificado para registrar via SkillRegistry

### F6 — MicroPython.wasm

- `load_micropython_wasm()`: tenta VFS real → fallback `wasmi_rt::generate_wasm_module()`
- `MicroPythonSandbox`: `WasmExecutor` → validação via `wasmi::Module::new()`; `eval()` usa `wasmi_rt::run_wasm()`
- WASI mapping e `MicroPythonSkill` intactos

### F7 — Ring Gate

- `isolation_ring_available()=false` (hardcoded, sem runtime toggle)
- B (`WasmJit`) e C (`NativeRustSubset`) retornam `FactoryOutcome::AwaitingIsolation` com log
- Só A (`WasmInterp`) executa via `wasmi_rt::run_i32_2()`

### Cleanup

- `wasm_rt.rs`: header de deprecação ADR-0059 (MemoryPool/HybridRegistry/PluginHub mantidos)
- `wasm_exec.rs`: header de deprecação ADR-0059 (definições `Op`/`WasmExec` mantidas)
- `package_hub.rs`: `PackageSignature` com `SimpleHasher` (xorshift) para AgentWasm

## Pendente

- **`cargo check --release`** no ambiente do usuário (toolchain `nightly-x86_64-unknown-none`)
- **WAT assembler full:** postergado (PONYTAIL) — upgrade quando `wat` crate tiver no_std
- **Persistência em disco:** residual ADR-0059 post-gate

## Lições

1. **`generate_skill_wasm()`** em `wasm_rt.rs` retorna `(Vec<Op>, WasmSkillManifest)` — não bytecode WASM real. Precisou criar `wasmi_rt::generate_wasm_module()` separado.
2. **`Vec<u8>` vs `Vec<Op>`:** a migração do Op VM para wasmi exige trocar tipo do bytecode em `VersionEntry`, `hot_swap()`, etc.
3. **Export section encoding:** no WASM binary, após o nome da exportação vêm `kind` (1 byte) + `func_idx` (LEB128) — o byte nulo no final do nome não serve como `kind`.
4. **`crate::DynamicSkill` vs `crate::dynskill::DynamicSkill`:** módulo precisa ser `pub mod dynskill` no lib.rs, não `pub use`.
