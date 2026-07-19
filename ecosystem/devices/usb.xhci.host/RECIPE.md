---
schema: 1
kind: device-recipe
name: xhci-host
package_id: usb.xhci.host
description: xHCI USB host U0–U2 tipados; filhos após EP0
goal: UsbHostSched + port ready; EP0 = residual U3
contexto: k_nano xHCI; DeviceClass UsbHost (não Video)
acionaveis: [init]
required_tokens: []
capabilities: [USB_HOST]
class: UsbHost
layer: L2
fat_short: LEGOXHCI.MD
bind:
  vendor_id: 0x0000
  device_id: 0x0000
  rev_mask: null
firmware: []
capabilities_required: [MapBar, DeviceIo]
trust_class: confirm
provenance: native_compiled
sandbox_status: none
honesty: no_fake_ready
requires: [PciEnumerated, ApicReady]
provides: [UsbHostReset]
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

Host xHCI — classe `UsbHost`. Filhos HID/MSC/UAC/BT após EP0 (U3+).

## Bind

Qualquer PCI class `0C03` (match ClassOnly na bind table).

## Stages / UnlockDAG

U0 reset → U1 sched → U2 port → U3 EP0 (residual).

## Pre-Flight

PCI xHCI; APIC; DeviceClass não Video.

## Success Criteria

Locate `LEGOXHCI.MD`; table ClassOnly; HalOffer UsbHost bind.

## Failure Policy

FAIL reset/sched; PARTIAL port; sem fingir UVC=host.

## Anti-Patterns

Classificar xHCI como Video; Ready BT sem UsbEp0.
