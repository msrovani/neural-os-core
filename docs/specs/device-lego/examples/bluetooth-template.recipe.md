---
schema: 1
kind: device-recipe
name: bluetooth-template
package_id: bt.template
description: Stub Bluetooth — path combo WiFi ou dongle USB
goal: Documentar requires; HCI mínimo futuro; sem fake pair
contexto: DeviceClass Bluetooth; código ausente — template comunidade
acionaveis: [on_demand]
required_tokens: []
capabilities: []
class: Bluetooth
layer: L2
bind:
  vendor_id: 0x0000
  device_id: 0x0000
  rev_mask: null
firmware: []
capabilities_required: []
trust_class: escalate
provenance: hermes_created
sandbox_status: none
honesty: no_fake_ready
requires: [UsbEp0]
provides: []
stages:
  - id: hci_probe
    requires: [UsbEp0]
    success_observable: "HCI read local version"
    on_fail: PARTIAL
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

Path A: combo no mesmo PCI WiFi (irmão). Path B: dongle pós-`UsbEp0`.

## Bind

Preencher VID/DID real.

## Firmware

`*_bt.bin` se WHENCE-ok + hash.

## RegMap

HCI transport — TBD com cite.

## Stages / UnlockDAG

hci_probe → scan/pair depois.

## HalOffer Port

`Bluetooth`.

## Internal edges

Falha BT ≠ falha Wifi.

## Pre-Flight

UsbEp0 ou WifiFwAlive (combo).

## Success Criteria

HCI version lido; pair = futuro.

## Failure Policy

PARTIAL; sem fake Connected.

## Anti-Patterns

Stub Agent em massa; Ready sem HCI.

## Test Plan

Serial quando BE existir.
