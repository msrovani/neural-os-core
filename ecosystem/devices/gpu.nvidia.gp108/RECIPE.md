---
schema: 1
kind: device-recipe
name: nvidia-pascal-gp108
package_id: gpu.nvidia.gp108
description: NVIDIA GP108 Pascal — Degrau ACR/FW (≠ compute Ready)
goal: Blobs FAT + ACR path; GpuCompute só após canário
contexto: LEGO L2 rebelde; ADR-0048 Pascal gate
acionaveis: [on_demand]
required_tokens: []
capabilities: [COMPUTE, NEEDS_FW]
class: Gpu
layer: L2
fat_short: LEGOGP08.MD
bind:
  vendor_id: 0x10DE
  device_id: 0x1C82
  rev_mask: null
firmware:
  - fat_name: ACR_BL.BIN
    role: ACR_BL
  - fat_name: ACRLOAD.BIN
    role: ACR_LOAD
  - fat_name: GPCCS_IN.BIN
    role: GPCCS
capabilities_required: [MapBar, DeviceIo, FeCompute]
trust_class: escalate
provenance: native_compiled
sandbox_status: none
honesty: no_fake_ready
requires: [PciEnumerated, FatReadable]
provides: [GpuAcrBooted]
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

GP108 (GTX 1050-class). Stages D0–D5; este golden cobre FW/ACR. Compute Ready = canário separado.

## Bind

PCI `10DE:1C82` (ajustar DID se outro Pascal no lab).

## Stages / UnlockDAG

D0 BAR → D1 FW → D2 ACR (`GpuAcrBooted`) → … → D5 canário (`GpuCompute`).

## Pre-Flight

FAT ACR/GPCCS; dGPU presente; sem GSP no git.

## Success Criteria

Locate `LEGOGP08.MD`; table hit; NeedsFw honesto sem blobs; Partial≠Ready.

## Failure Policy

NeedsFw / AWAITING_REAL_HW; sem fake golden compute.

## Anti-Patterns

Claim GpuCompute só com ACR; embutir GSP.
