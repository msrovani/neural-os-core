# SESSION_496 — GPU compute AIOS multi-vendor

**Sprint:** v1.9.99-s496 TEST  
**Data:** 2026-10-18  
**Trigger:** plano `gpu_aios_compute` — escada vendor-agnostic NVIDIA/AMD/Intel; lab Ampere+Intel = âncora, não escopo do produto.

## Goal

Observe→Plan posture→Act unlock disponível→fallback honesto (BAR Mapped/Streams → device se Ready → SMP → W2A8 CPU → SSE2). Sem hardcode Ampere-only; Mapped/Streams ≠ note_gpu_compute; stub pack ≠ Ready.

## Feito

### F0 — Matriz IsaTag
- `IsaTag::from_u32` cobre Sm52…Sm89, Gen9, Dg2, Gfx90c/1030/1036/1103; `0`/desconhecido → `None`.
- `backend::compute_isa_tag` usa `from_u32` (fecha Sm86=13 → None).
- Host: `isa_tag_roundtrip_all_vendors` 1/1.

### F1 — `compute_posture` (k_hal)
- Módulo `gpu/compute_posture.rs`: enums `ComputePosture` + `DeviceAwaitReason` tipados (`NeedsGsp`/`NeedsAcr`/`NeedsPsp`/`NeedsGuc`/`NeedsSignedPack`/…).
- Observe: GpuInfo / display_coex compute GPU, aperture, need, pack, stage, uploaded, ap_pollable.
- Plan→Act leve: slog + Tickv `hw/compute/posture`; `aios_adapt::refresh` chama `compute_posture::refresh`.
- Host: 3/3 (`await_reason_gsp`, `await_reason_acr`, `streams_when_need_gt_aperture`).

### F2 — StreamsW2a8
- `bar_compute`: se need > aperture → `enter_streams_mode` + `ensure_stream_layer(L)` (1 layer na janela); miss → CPU.
- Sem `note_gpu_compute`; `mark_uploaded` / `note_vram_weights_ready` só após upload real ou streams enter.
- `vram_served` = register **e** `VRAM_WEIGHTS_READY` (honesty: register ≠ uploaded).

### F3 — Loader stub packed
- `register_loader_resident` → `set_vram_external_weights(true)`.
- `load_llm_v6` / `read_ternary_tensor_with_scale`: avança offset sem alocar packed (RAM win).

### F4 — UnlockDAG Layer S (scaffolding)
- CapTokens: `GpuGspBooted=16`, `GpuPspAlive=17`, `GpuGucReady=18`.
- Recipes golden: ga107 (GSP), gfx1030 (PSP/MEC), dg2 (GuC/HuC/DMC) + preload FW **por recipe DID** no bin (nunca GP108 em Ampere).
- Device path Ready = residual por família (GSP/PSP/GuC fence); até lá posture `DeviceAwait` + BAR/CpuLadder.

## Prova (host)

| Claim | Estado |
|---|---|
| `cargo test -p k-hal --lib isa_tag_tests` | VERIFIED 1/1 |
| `cargo test -p k-hal --lib compute_posture` | VERIFIED 3/3 |
| `cargo check -p neural-kernel --release` | 0 erros |
| Metal `isa=` remember ≠ None / `compute_posture=` / Streams | UNKNOWN até re-image |
| Device fence GSP/PSP/GuC | UNKNOWN (Layer S) |

## Checklist genérico (qualquer boot futuro)

```
vendor=
isa=
compute_posture=
await=
compute_rung=
BARCOMPUTE|StreamsW2a8|lane VRAM
vram_external_weights|loader-VRAM
promote=.*ga107|gfx1030|dg2|gp108
```

## Honesty (ADR-0113)

- Mapped/Streams ≠ gpu GO / `note_gpu_compute`.
- stub/unsigned NKP ≠ Ready.
- `vram_served` exige upload real.
- Lab Ampere não define o limite do produto.
- Metal claims = UNKNOWN até stick + BOOT.LOG.

## Dual-GPU (item 5 — confirmado)

- Wire já usava `plan.compute_index()` p/ `init_vram_tier` / `init_bar_compute` / FW recipe.
- Hardening: recusa `is_integrated` em VRAM+BAR; slog `aperture_owner=` + `dual BAR target`; `best_compute_gpu` → `display_coex`.
- Host: `dual_lab_intel_display_ampere_compute` (UHD + GA107/25ac sm_86) 5/5 `display_coex`.
- Metal grep: `DUAL` / `aperture_owner=` / `dual BAR target` / `isa=sm_86` / sem Mapped na iGPU.

## Pendentes

1. Rebuild imagem + boots metal (falsificadores F0 + posture + Streams se Active > aperture).
2. Layer S por família quando HW + OK maintainer (GSP Ampere lab; ACR Pascal; PSP AMD; GuC Intel).
