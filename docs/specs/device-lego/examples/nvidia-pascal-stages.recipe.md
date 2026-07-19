---
schema: 1
kind: device-recipe
name: nvidia-gp108-pascal
package_id: gpu.nvidia.gp108
description: NVIDIA Pascal gp108 degraus ACR→canário (lab)
goal: Publicar tokens UnlockDAG; Ready compute só com canário PASS
contexto: ADR-0048; BE em k_hal/gpu; display_coex
acionaveis: [init]
required_tokens: []
capabilities: [COMPUTE, DISPLAY, NEEDS_FW]
class: Gpu
layer: L2
bind:
  vendor_id: 0x10DE
  device_id: 0x1C82
  rev_mask: null
firmware:
  - fat_name: FW_FECS_BL.BIN
    role: FECS
    blob_hash: ""
capabilities_required: [MapBar, DeviceIo, FeCompute]
trust_class: escalate
provenance: native_compiled
sandbox_status: none
honesty: no_fake_ready
requires: [PciEnumerated, FatReadable]
provides: [GpuBarMapped]
stages:
  - id: d0_bar
    provides: [GpuBarMapped]
    success_observable: "GPU BAR mapped"
    on_fail: FAIL
  - id: d1_fw
    requires: [GpuBarMapped, FatReadable]
    provides: [GpuFwPresent]
    on_fail: NeedsFw
  - id: d2_acr
    requires: [GpuFwPresent]
    provides: [GpuAcrBooted]
    success_observable: "AcrStage HsBooted"
    on_fail: PARTIAL
  - id: d5_canary
    requires: [GpuAcrBooted]
    provides: [GpuCompute]
    success_observable: "canary PASS"
    on_fail: PARTIAL
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

Pascal GP108 lab. `GpuAcrBooted ≠ GpuCompute`. Cite ADR-0048 / Nouveau.

## Bind

Exemplo `10DE:1C82` — ajustar ao DID real do lab.

## Firmware

FECS/GPCCS/ACR via `firmware/nvidia/gp108/` — sem GSP no git.

## RegMap

Ver `k_hal/gpu/nvidia*.rs`.

## Stages / UnlockDAG

D0→D1→D2→…→D5; DisplayOwner paralelo (display_coex).

## HalOffer Port

`Gpu` / compute port.

## Internal edges

Falha compute não reseta DisplayOwner.

## Pre-Flight

FW preload ATA/USB; plan LegacyAcr.

## Success Criteria

Canário vector_add PASS em HW; senão AWAITING_REAL_HW.

## Failure Policy

PARTIAL se ACR ok sem canário.

## Anti-Patterns

Claim Ready sem canário; GSP no PR.

## Test Plan

`[GPU-HW] VERDICT=` serial.
