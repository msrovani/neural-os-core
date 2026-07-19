---
schema: 1
kind: device-recipe
name: virtio-net
package_id: net.virtio
description: VirtIO-net L1 behaved gold — HalOffer Net
goal: Bind Net port via VirtIO transport sem FW blob
contexto: LEGO L1 bem-comportado; VirtIO=transporte BE
acionaveis: [init]
required_tokens: []
capabilities: [NET]
class: Net
layer: L1
bind:
  vendor_id: 0x1AF4
  device_id: 0x1041
  rev_mask: null
firmware: []
capabilities_required: [FeNet]
trust_class: confirm
provenance: native_compiled
sandbox_status: none
honesty: no_fake_ready
requires: [PciEnumerated]
provides: [NetIfaceVirtio]
stages:
  - id: offer_bind
    provides: [NetIfaceVirtio]
    success_observable: "HalOffer Net Bound"
    on_fail: FAIL
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

VirtIO-net (OASIS). API produto = HalOffer `Net`; transporte = VirtIO queues.

## Bind

PCI `1AF4:1041` (e variantes VirtIO-net do projeto).

## Firmware

Nenhum.

## RegMap

VirtIO PCI common config — ver `k_hal::virtio`.

## Stages / UnlockDAG

offer_bind → `NetIfaceVirtio`.

## HalOffer Port

`DeviceClass::Net` — tx/rx via net port.

## Internal edges

Independente de Wifi/GPU.

## Pre-Flight

PCI VirtIO presente; HalOffer init.

## Success Criteria

Net Bound + RX path (QEMU ok).

## Failure Policy

FAIL se device ausente; sem fake Connected WiFi.

## Anti-Patterns

Misturar VirtIO API com HalOffer no nome; claim Wifi.

## Test Plan

QEMU virtio-net; serial HalOffer / net init.
