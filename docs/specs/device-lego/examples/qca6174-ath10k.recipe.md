---
schema: 1
kind: device-recipe
name: qca6174-ath10k
package_id: wifi.qca6174.ath10k
description: Note 1050 QCA6174 ath10k A3 BMI→fw_ready
goal: FW_IND_INITIALIZED após BMI_DONE; sem fake Ready/Connected
contexto: LEGO L2 rebelde; Linux ath10k QCA6174 hw3.0
acionaveis: [on_demand]
required_tokens: []
capabilities: [WIFI, NEEDS_FW]
class: Wifi
layer: L2
bind:
  vendor_id: 0x168C
  device_id: 0x003E
  rev_mask: null
firmware:
  - fat_name: AT10K_F6.BIN
    role: FW_IMAGE
    blob_hash: ""
  - fat_name: AT10K_B2.BIN
    role: BOARD
    blob_hash: ""
  - fat_name: AT10K_BD.BIN
    role: BOARD
    blob_hash: ""
capabilities_required: [MapBar, DeviceIo, FeWifi]
trust_class: escalate
provenance: native_compiled
sandbox_status: none
honesty: no_fake_ready
requires: [PciEnumerated, FatReadable]
provides: [WifiFwAlive]
stages:
  - id: wake
    provides: [SocAwake]
    success_observable: "step=wake status=OK"
    on_fail: FAIL
  - id: target_init
    requires: [SocAwake]
    success_observable: "step=target_init status=OK"
    on_fail: FAIL
  - id: bmi_lz
    requires: [SocAwake, FatReadable]
    success_observable: "step=bmi_lz"
    on_fail: PARTIAL
  - id: bmi_done
    requires: [SocAwake]
    provides: [WifiFwAlive]
    success_observable: "VERDICT=PASS fw_ready=1"
    on_fail: PARTIAL
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

QCA61x4A / QCA6174 PCIe — Linux `ath10k` (não Ath9k SoftMAC). Cite: wireless.docs.kernel.org ath10k.

## Bind

`168C:003E` (também família `0041` se aplicável). **Não** mapear como Ath9k.

## Firmware

`firmware/ath10k/QCA6174/hw3.0/` → FAT `AT10K_*.BIN`. Preencher `blob_hash` pós-download.

## RegMap

QCA6174: CE0 `0x34400`, FW_INDICATOR `0x3a028`, RTC wake — ver `ath10k_ce_bmi.rs` / Linux.

## Stages / UnlockDAG

wake → target_init → BMI LZ → BMI_DONE → `WifiFwAlive`. Scan/assoc = A4 (não nesta recipe).

## HalOffer Port

`DeviceClass::Wifi`; FE Hermes WifiAgent.

## Internal edges

Combo BT = nó irmão futuro; não bloquear WiFi.

## Pre-Flight

`python tools/build_image.py` com AT10K; BAR map; Cap.

## Success Criteria

`[ATH10K] VERDICT=PASS fw_ready=1` no Note.

## Failure Policy

PARTIAL com reason; nunca Connected sem RF.

## Anti-Patterns

Ath9k path; fake Ready; offsets sem cite.

## Test Plan

Note 1050 serial; QEMU sem radio = skip runtime.
