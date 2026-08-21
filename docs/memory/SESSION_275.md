# SESSION_275 — Mesh P2P QEMU matrix: GOAL1/2/3 fechados (TCG)

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-21  
**Escopo:** estabilizar dual-QEMU mesh (`run-qemu-p2p-mesh.ps1`) — scheduler smash, TOFU settle, MicroPython MVP wasmi + SkillSync probe. Cita ADR-0081 (mesh) / ADR-0059 F6 (wasmi).

## GOALs

| GOAL | Sintoma | Fix |
|------|---------|-----|
| **1** | Stack smash do scheduler — `AgentRegistry` no bump **abaixo** da stack → overflow zera Vec/BTree (`#PF CR2=0x18`) | Stack **8MB + guard 64K** **antes** de `Box::leak(AgentRegistry)` em `neural-kernel/main.rs`; cache `name`/`auto_start` + defesa em `on_activate` no `agent-core`; `is_page_present` nos scans BPE/STT/Piper (QEMU-loader) |
| **2** | ROLE/SkillSync antes do TOFU bilateral → drops "peer desconhecido"; Worker clobberava Memory/Compute | `TOFU_SETTLE_TICKS=130` + `FORCE_HEARTBEAT`; ROLE/SkillSync/MKTP gated pós-`tofu_settled()`; `become_worker` preserva Memory/Compute; `clear_synced_for_resync` + re-push |
| **3** | MicroPython path usava stub wasmi; SkillSync sem smoke Master→Worker | MVP WASM (`tools/build_micropython_wasm.py` → `MICROPY.WASM`); load wasmi sem stub; probe `mesh_g3_probe` Master→Worker (`aplicada do Master`); `mkfat32` inclui `MICROPY.WASM` |

## Script mesh

`run-qemu-p2p-mesh.ps1`: `-Cores`, `-Accel` (WHPX→TCG fallback), `-Mem`, `-WithModels`/`-NoModels`, `-Instance A|B|Both`.

## Evidência

- **PASS:** TCG 2c / 6G / NoModels / Both — SCHED estável, A=Master B=Memory, `sec=0/0/0`, SkillSync OK (`mesh_g3_probe` aplicada).
- **FAIL:** WHPX + OVMF `#GP` neste host.
- **Não rodada:** matriz completa `{1,4,8}×WHPX×WithModels`.

## Gaps (honestos)

- ESP `build.rs` ainda falha → `mk_esp_fat` manual.
- `opt-level=3` / `-Z threads=16` → `STATUS_ILLEGAL_INSTRUCTION` neste host (`.cargo/config.toml` removeu threads=16; check com `CARGO_PROFILE_RELEASE_OPT_LEVEL=1` se necessário).
- MicroPython MVP ≠ full emcc — módulo mínimo `_start`/`python_eval`/`exec`.
- 6 falhas pré-existentes `hermes::wasm_build` (não regressão desta linha).

## Arquivos tocados

- `run-qemu-p2p-mesh.ps1`
- `crates/agent-core/src/lib.rs`
- `crates/neural-kernel/src/{main.rs,bei_init.rs}`
- `crates/k_nano/src/net/mesh.rs`
- `crates/hermes/src/{skill_sync.rs,micropython_wasm.rs}` + `assets/micropython.wasm`
- `crates/cortex/src/bpe.rs`, `crates/jarbas/src/audio/{stt,skills}.rs` (guards PRESENT)
- `tools/{build_micropython_wasm.py,mkfat32.py}`, `models/MICROPY.WASM`
- `.cargo/config.toml` (sem `-Z threads=16`)

## Verificação (pós-tarefa)

- `CARGO_PROFILE_RELEASE_OPT_LEVEL=1 cargo check --release` → **0 erros** (1 warning unused import ahci; `boot` ESP image creation failed — gap conhecido).
- `cargo test --workspace --exclude neural-kernel --exclude boot` → hermes **109 pass / 6 fail** pré-existentes (`wasm_build`: instantiate/sandbox); `micropython_wasm::embedded_mvp_wasm_is_valid_wasmi_module` PASS. Demais crates até hermes OK (agent-core 1, cortex 25).

## Lição

Ordem no bump importa: stack cresce para baixo — registry **depois** da stack+guard. TOFU settle é janela temporal, não só bind de pk: SkillSync cedo marca `synced` e nunca reenvia sem `clear_synced`.
