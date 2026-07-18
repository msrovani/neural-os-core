# SESSION_142 — Multi-model hub (TinyStories / 3B / GGUF-WASM / RustCoder)

**Data:** 2026-07-18  
**Foco:** Modelos extras sem quebrar CURRENT_MODEL / Trinity.

## Entrega

| Modelo | Integração |
|--------|------------|
| TinyStories 1M–15M | Slot `tinystories`; FAT `TINYSTOR.BIN` / `TINY.BIN`; intent smoke/story |
| BitNet 3B pro | Slot `generator_pro` ou Active grande + `mark_pro_alias`; complex conversation |
| 850M fast | Slot `generator_fast`; FAT `BITNET850.BIN` / `MICRO.*`; fallback se Pro miss |
| GGUF via WASM | `hermes::gguf_wasm::register_gguf_wasm_skill` → SkillMarket `wasm_skill_name` |
| RustCoder 2B/3B | Mesmo expert `rust_coder`; FAT `RUSTCDR3.BIN` → `RUSTCDR2.BIN` → `RUSTCDR.BITNET` |

## APIs
- `cortex::load_models_multi` / `register_model_slot`
- `model_hub::{select_generator_slot, hub_status, fat_names_for}`
- `gguf_wasm::register_gguf_path(path, wasm_bytes)`

## Compat
- `set_model` / Trinity expert list inalterados (router MoE indices estáveis)
- Sub-rota generator → ModelHub no `dispatch_expert`
- GGUF catalog skill **não** chama CURRENT_MODEL (isolado)

## FAT 8.3
`TINYSTOR.BIN`, `BITNET850.BIN`, `BITNET3B.BIN`, `RUSTCDR2.BIN`, `RUSTCDR3.BIN`
