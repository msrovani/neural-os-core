---
schema: 1
kind: device-recipe
name: example-device
package_id: example.device
description: Template DeviceRecipe — preencher antes de assinar
goal: Bring-up medido sem fake Ready
contexto: LEGO L2 no K³CHJ HalOffer
acionaveis: [on_demand]
required_tokens: []
capabilities: []
class: Wifi
layer: L2
bind:
  vendor_id: 0x0000
  device_id: 0x0000
  rev_mask: null
firmware: []
capabilities_required: [MapBar, DeviceIo]
trust_class: escalate
provenance: hermes_created
sandbox_status: none
honesty: no_fake_ready
requires: [PciEnumerated]
provides: []
stages: []
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

Chip, bus, família Linux equivalente.

## Bind

VID/DID e exclusões.

## Firmware

FAT short names + `blob_hash`.

## RegMap

Nome→offset (só bring-up); cite fonte.

## Stages / UnlockDAG

`requires` / `provides` + `success_observable` / `on_fail`.

## HalOffer Port

Classe L1 e ops mínimas.

## Internal edges

Filhos USB, irmãos WiFi↔BT, coex display/compute.

## Pre-Flight

Blobs? Cap? BAR? Tokens pais?

## Success Criteria

Por stage; Ready global só com provides críticos.

## Failure Policy

PARTIAL vs FAIL; nunca Connected sem RF.

## Anti-Patterns

Proibições desta recipe.

## Test Plan

Serial markers + HW alvo.
