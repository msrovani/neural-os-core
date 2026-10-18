---
schema: 1
kind: device-recipe
name: nvidia-ampere-ga107
package_id: gpu.nvidia.ga107
description: NVIDIA Ampere GA107 — GSP path (≠ Pascal gp108)
goal: FW hint e promote NEEDS_FW honestos para DID 25ac; compute Ready só com canário+pack
contexto: Metal dual-GPU (Intel display + NVIDIA compute sm_86); ADR-0105/0112
acionaveis: [on_demand]
required_tokens: [1]
capabilities: [COMPUTE, NEEDS_FW]
provenance: native_compiled
sandbox_status: none
trust_class: escalate
content_hash: "pending"
signature: "pending"
---

## Contexto

Ampere GA107 (ex. 10de:25ac laptop). Backend = GSP, não ACR Pascal/gp108.
Display dual: iGPU Intel; compute = este dGPU.

## Goal

Promote `NEEDS_FW` com hint `nvidia/ga107` até blobs GSP no FAT; nunca sugerir gp108.
BAR Mapped (ADR-0112) pode servir pesos sem GSP; device W2A8 = Layer S.

## Acionaveis

on_demand — SelfHeal / HalOffer após PCI observe.

## Workflow

1. Observe DID/arch (PMC_BOOT_0 Ampere)
2. Plan OpProfile Dp4aW2A8 + recipe ga107
3. Act: load GSP se presente; senão CPU/SMP/BAR ladder
4. Verify: BAR0 MMIO PMC ≠0; vram_size sanificado

## Pre-Flight

- [ ] Recipe match 10DE:25AC
- [ ] hint ≠ nvidia/gp108
- [ ] read_bar_size não explode

## Success Criteria

Log sem `firmware_hint:nvidia/gp108` para 25ac; `pkg=gpu.nvidia.ga107` quando matched.

## Failure Policy

Sem GSP → escalate + CPU ladder; Quarantine se BAR0 validate fail; nunca hang.
