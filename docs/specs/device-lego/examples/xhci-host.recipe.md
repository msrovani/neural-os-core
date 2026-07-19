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
stages:
  - id: u0_reset
    provides: [UsbHostReset]
    on_fail: FAIL
  - id: u1_sched
    requires: [UsbHostReset]
    provides: [UsbHostSched]
    on_fail: FAIL
  - id: u2_port
    requires: [UsbHostSched]
    provides: [UsbPortReady]
    on_fail: PARTIAL
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

PCI class `0C03` xHCI. USB 3.x = SuperSpeed só se medido.

## Bind

Match por classe PCI host (VID/DID específicos por máquina).

## Firmware

Nenhum para host controller.

## RegMap

xHCI MMIO — `k_nano::xhci`.

## Stages / UnlockDAG

U0–U2; U3 EP0 abre UAC/BT/CDC.

## HalOffer Port

`UsbHost` (não mapear só como Video).

## Internal edges

UsbMsc → FatReadable → WiFi/Gpu FW.

## Pre-Flight

Platform sync antes de init_xhci.

## Success Criteria

HID/MSC poll OK; UAC pode PARTIAL.

## Failure Policy

Sem claim USB3.2 sem link speed.

## Anti-Patterns

DeviceClass Video-only para host genérico.

## Test Plan

Serial xHCI init; stick MSC.
