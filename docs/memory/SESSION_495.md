# SESSION_495 — Escada W2A8 AIOS (metal Ampere)

**Sprint:** v1.9.99-s495 TEST  
**Data:** 2026-10-18  
**Trigger:** plano `w2a8_aios_ladder` — metal `10de:25ac` Ampere sm_86; âncora `logs/BOOT_neural.txt` (vram lixo, recipe gp108, `ap_pollable feature_off`, `w2a8_pack=0`).

## Goal

Desbloquear a escada Observe→Plan→Act→Verify→Remember: BAR/SMP/CPU-W2A8 primeiro; GSP device = Layer S (fora do entregável).

## Feito

### P0.1 BAR size / roles
- `k_nano::pci::read_bar_size`: `mask_hi=0` não explode 64-bit; size sanitizado.
- `k_hal::gpu::detect`: VRAM ∈ [64MB,64GB]; NVIDIA BAR0 = MMIO; `backend` skip se `bar2==bar0`.

### P0.2 Recipe / SelfHeal
- `nvidia_firmware_hint(did)` → Ampere `nvidia/ga107`, Pascal `gp108`, Turing `tu106`.
- `firmware_hint_for` / SelfHeal usam DID-aware; recipe bind `gpu.nvidia.ga107` + `ecosystem/devices/gpu.nvidia.ga107/RECIPE.md`.

### P0.3–P0.4 CPU W2A8 + SMP
- `ap-pollable` default ON em `neural-kernel` + `boot` Cargo.toml.
- `install_wake_fn` **antes** de `try_enable_ap_workers_from_feature` (evita `no_wake`).
- `big_ladder_boot_probe` 64×64 em MatmulDiag (abre gaps só no probe).

### P0.5 Telemetria
- `compute_rung_label` / `slog_compute_rung` (npu→bar_mapped→gpu→smp→w2a8_cpu→sse2).
- Remember leve: Tickv `hw/compute/rung` se ready + slog `/hw/compute/rung=…`.

### P1 BAR Mapped
- Wire já canônico: `init_vram_tier` → `init_bar_compute` → hook upload → `register_vram_ternary_seq` se enabled.
- Lane OFF honesto se modelo > aperture (sem ReBAR); Mapped **não** seta `gpu=GO`.

### P2 Pack NKP sm_86
- `tools/pack_nkp_lab.ps1 -Sm sm_86` → `target/nkp-lab/NKP_W2A8_SM86.BIN` (CpuStub unsigned; `w2a8_pack=1` ≠ Ready).
- `mkfat32` já embute `NKPW2A886.BIN` se o artefato existir.

## Prova (host)

| Claim | Estado |
|---|---|
| `cargo test -p k_ai --lib hw_capability::tests` | VERIFIED 2/2 |
| `cargo check -p neural-kernel --release` | 0 erros |
| NKP stub `NKP_W2A8_SM86.BIN` em `target/nkp-lab/` | VERIFIED (unsigned CpuStub) |
| Metal vram_size ∈ [256MB,16GB] / PMC em MMIO | UNKNOWN até re-image + boot |
| `ap_pollable status=OK` + `N_SMP>0` matmul big | UNKNOWN até metal |
| `compute_rung=` + sem `pkg=gpu.nvidia.gp108` p/ 25ac | UNKNOWN até metal |
| Device fence / Ready W2A8 GPU | UNKNOWN (Layer S; pack≠Ready) |

## Falsificadores (grep BOOT.LOG pós metal)

```
vram=.*MB
BAR0 validation|bar0=0x
firmware_hint|promote=.*gp108|ga107
ap_pollable status=OK
compute_rung=
w2a8_pack|NKP
big ladder probe
BARCOMPUTE|lane VRAM
```

## Honesty (ADR-0113)

- Código wired ≠ propriedade metal.
- Mapped ≠ `note_gpu_compute` / SYS_HEALTH gpu GO.
- Stub NKP unsigned nunca Ready.
- GSP Ampere / fence = backlog Layer S (não portado nesta sessão).

## Pendentes

1. Rebuild `usb_hw.img` (ou pipeline NRFS) + gravar stick + 1–2 boots metal.
2. Grep falsificadores acima → promover P0.1–P0.4 a VERIFIED se casarem.
3. Layer S: GSP boot / fence (referência nova-core, não Linux port).
