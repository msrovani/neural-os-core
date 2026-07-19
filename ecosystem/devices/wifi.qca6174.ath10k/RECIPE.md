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
fat_short: LEGOATHK.MD
bind:
  vendor_id: 0x168C
  device_id: 0x003E
  rev_mask: null
firmware:
  - fat_name: AT10K_F6.BIN
    role: FW_IMAGE
  - fat_name: AT10K_B2.BIN
    role: BOARD
  - fat_name: AT10K_BD.BIN
    role: BOARD
capabilities_required: [MapBar, DeviceIo]
trust_class: escalate
provenance: native_compiled
sandbox_status: none
honesty: no_fake_ready
requires: [PciEnumerated, FatReadable]
provides: [WifiFwAlive]
license: AGPL-3.0
content_hash: ""
signature: ""
---

## Contexto

QCA6174 ath10k — BE nativo `wifi_ath10k` A3. Recipe amarra FAT + UnlockDAG.
H1 Cap gate: `GOLDEN_RECIPES` + presença `AT10K_*`.

## Bind

PCI `168C:003E` (QCA6164 `0041` irmão).

## Stages / UnlockDAG

wake → target_init → bmi_lz → bmi_done → fw_ready (`WifiFwAlive`).

## Pre-Flight

FAT com AT10K_*; Note ou HW com 003E; sem claim RF.

## Success Criteria

Locate `LEGOATHK.MD`; table hit; promote Ok só com blobs; PASS só com serial fw_ready.

## Failure Policy

NeedsFw se blob ausente; PARTIAL se BMI sem FW_IND; nunca fake Connected.

## Anti-Patterns

Mapear como ath9k; Ready sem WifiFwAlive.
