---
name: neural-device-lego
description: >-
  Author or review Neural DeviceRecipe / LEGO HW (ADR-0056): RECIPE.md trust,
  UnlockDAG stages, SDIO index ethics, firmware blob_hash. Use when editing
  device recipes, HalOffer bind, ath10k/NVIDIA unlock, or docs/specs/device-lego.
---

# Neural Device LEGO

## When to use

- Creating/editing `RECIPE.md` or `docs/specs/device-lego/`
- Binding PCI/USB devices via HalOffer / `device_recipe.rs`
- SDIO export, HF datasets, or firmware FAT for a chip
- Claims about Ready / Connected / VERDICT=PASS

## Mandatory reads

1. `docs/architecture/0056-neural-device-lego.md`
2. `docs/specs/device-lego/RECIPE.template.md` + `TRUST.md` + `UNLOCK_DAG.md`
3. `.cursor/rules/neural-device-lego.mdc`
4. Community: `docs/community/DANGERS.md` + `LICENSES.md`

## Workflow — new recipe

1. Clone a golden under `docs/specs/device-lego/examples/` (virtio-net = behaved; qca6174 = rebel).
2. Fill frontmatter: `kind: device-recipe`, `package_id`, `honesty: no_fake_ready`, match VID/DID.
3. Stages with `success_observable` / `on_fail`; Ready only when provides are measured.
4. `firmware[].blob_hash` for any L2 that needs FW; cite WHENCE / linux-firmware path.
5. Without valid `content_hash` + Ed25519 `signature` → draft **Escalate** only (PackageHub).
6. Path: `/mnt/neural/ecosystem/devices/<name>/RECIPE.md`.

## Honesty badges

| Claim | Allowed when |
|-------|----------------|
| Install / Present | PCI enum / recipe match |
| NeedsFw | FAT miss or hash fail |
| Ready | success_observable + provides |
| Connected / RF | link / scan evidence on target HW |
| VERDICT=PASS | serial log on that machine |

Install ≠ Ready. SDIO ≠ bring-up. Unsigned ≠ Auto bind.

## SDIO / HW Expert

```powershell
python tools/export_sdio_lego_index.py --dry-run
python tools/train_hw_expert_v4.py --dry-run
```

Index/labels only. Never invent MMIO from `.inf`. Never republish `.sys`.

## Firmware

```powershell
python tools/download_firmware.py --list
$env:FW_FAT_CHIPS="gp108,ath10k"
python tools/build_image.py
```

GSP stays out of git (`target/firmware/` only).

## Code touchpoints

- `crates/k_hal/src/device_recipe.rs` — H1 promote (trusted + fw_hash)
- `crates/k_hal/src/device_cap.rs` — UsbHost / Bluetooth
- `crates/hermes/src/package_hub.rs` — `PackageKind::DeviceRecipe`

## Anti-patterns

- Mass stub recipes · fake PASS · map QCA6174 as ath9k · GSP in git · VirtIO≡HalOffer
