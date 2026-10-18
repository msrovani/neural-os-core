# SESSION_489 — LOO Load Usable (HW-first)

**Sprint:** v1.9.99-s489  
**Data:** 2026-10-18  
**Premissa:** QEMU = lab/smoke; aceite VERIFIED de nichos mid = **metal** (`usb_hw.img` / notebook). Não elevar `PIO_QEMU`; path canônico = FAT/NeuralFS + `register_bytes`.

## Problema

MoE/`resolve_route` → `ensure_slot_loaded` escolhia o slot certo, mas o adapter **não materializava** blob coerente:

1. `validate_bitnet_blob` recusava `model_type!=0` → matava HwExpert no demand path.
2. LOO usava `load_model` + `register_model_slot` (LLM-only) em vez de `register_bytes` (kind-aware).
3. Gate `loaded_model_header().is_none()` impedia AlwaysTiny com Active residente.
4. `set_hwexpert_v4_model` não `mark_slot(HwExpert)` → `slot_real` mentia.
5. Lab confundia `PIO_QEMU=8MB` com política de produto (RustCoder ~321MB = path metal).

## Feito (código)

| Item | Onde |
|---|---|
| `validate_for_slot(slot, data)` — HwExpert type=1; LLM type=0+probe; Vision/Reranker magic+versão | `cortex/model.rs` |
| NeuralFS + FAT LOO → `validate_for_slot` → `register_bytes` | `neural-kernel/main.rs` |
| AlwaysTiny HwExpert sem exigir Active vazio; boot residency pós-loader | `main.rs` |
| `mark_slot(HwExpert)` em `set_hwexpert_v4_model` | `cortex/cortex.rs` |
| PCI inject + family/agent names usáveis em `generate_from_slot(HwExpert)` | `cortex/model_hub.rs` |
| Metal `pio_cap` = `pio_hw.max(footprint)` p/ RustCoder/Agent/Learner/Reranker; QEMU = 8MB + warn skip | `main.rs` |
| `fat_names_for(HwExpert)` inclui `HW_EXPERT.BITNET`; RustCoder `RUSTCDR3.*` | `model_hub.rs` |
| ADR-0078 § materialize HW-first | `docs/architecture/0078-*.md` |

## Prova

| Tier | Resultado | Status ADR-0113 |
|---|---|---|
| Host | `validate_for_slot_hwexpert_vs_llm` + gates + residency + `resolve_route` OK; `cargo check --release -p cortex -p neural-kernel` 0 erros | **VERIFIED** |
| QEMU | Seam LOO + HwExpert ≤8MB exercitável; mid = `PRESENT … skip PIO (lab_PIO_QEMU)` → niche_miss→Active | smoke (não basta p/ mid) |
| Metal | Chat Active; `identifique`→HwExpert; `gere codigo`→RustCoder via FAT LOO | **UNKNOWN** até boot `usb_hw.img` |

### Aceite metal (operador)

```powershell
# Empacota Active + HwExpert + RustCoder (mkfat32 já lista HW_EXPERT.BITNET / RUSTCDR3.BIN)
$env:PACK_LLM="falcon3-1b,falcon3"   # ou 2b/none conforme RAM
python tools/build_image.py --hw --unified --build-boot --size 8192
# Rufus DD → notebook, Secure Boot OFF
# Grep BOOT.LOG / serial: register_bytes slot=hw_expert|rust_coder; hw_identify family=…
```

## Honestidade

- Código = path produto metal. Declaração mid-niche **VERIFIED** exige evidência de runtime no silício — não WHPX.
- Não subir `PIO_QEMU`; QEMU-loader para RustCoder fora de escopo.

## Lições

1. Validador LLM-only no demand path é anti-MoE: slot-aware (`validate_for_slot`) é o contrato.
2. Cap de lab ≠ teto de produto: separar `qemu_hv` de `pio_hw`/footprint.
3. AlwaysTiny com LLM residente: gate por “Active vazio” mata o expert no path real.

## Correção pós-auditoria (mesma sessão)

Inventário `tools/audit_model_blobs.py`:

| Problema | Ação |
|---|---|
| `AGENT.v6` / `LEARNER.v6` **size_mismatch** (header≠arquivo; sem GGUF p/ reexport) | Quarentena → `*.size_mismatch`; mkfat32 `find_valid_llm` + `bitnet_llm_size_ok` |
| `target1/FALCON3.BIN` / `models/FALCON3.V6` L=28 (~7B mal-nomeado) | → `*.misname_L28`; `find_falcon3` já filtrava L=22 |
| footprint RustCoder 260 &lt; 321MB | hint → **322** (`model_fit.rs`) |
| `find_falcon3_1b` não via `FALCON3_1B.V6` | nomes + header v7 em `_v6_layers_hidden` |
| Learner sem blob válido | fallback **Falcon3-1B** no pack |

Re-audit: **REJECT=0** nos blobs ativos. Regenerar Agent/Learner = `convert_gguf_to_bitnet.py` quando houver GGUF.

## Pendentes

- Boot metal + slog `LOADED via register_bytes` HwExpert + RustCoder → promover metal a VERIFIED.
- Reconverter AGENT/LEARNER a partir de GGUF (Qwen 2.5 3B / 0.5B) com writer canônico.
- Validate/evict runtime sob pressão (s485) no mesmo aceite.
- #PF storm falcon3 (pré-existente, watchpoint).
