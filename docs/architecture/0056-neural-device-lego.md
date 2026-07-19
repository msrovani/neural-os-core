# ADR-0056: Neural Device LEGO

**Data:** 2026-07-18  
**Status:** Accepted  
**Lifecycle (INDEX):** `fazendo`  
**Ideias:** #407b, #464 (Device LEGO)  
**Relacionadas:** ADR-0041 (HalOffer), 0048–50 (GPU), 0051–0053 (PackageHub/trust/market), NeuralFS §12

## Contexto

Precisamos de conectores genéricos (HW bem-comportado) e um contrato para HW rebelde (WiFi FW-MAC, GPU) sem fingir módulos ELF em Ring 1. A comunidade e IAs devem contribuir com specs AI-Friendly, não com `.sys`.

## Decisão

1. **L0 Bus** — MMIO/IRQ/DMA leases só em k-nano/k-hal nativo.
2. **L1 Class Port** — HalOffer + ports; VirtIO = transporte BE, não nome da API.
3. **L2 DeviceRecipe** — kind `device-recipe` em `/mnt/neural/ecosystem/devices/<name>/RECIPE.md`; FW em `ecosystem/firmware/`.
4. **Trust** — ADR-0052/0053: `content_hash` + Ed25519; `blob_hash` nos blobs; unsigned = Escalate.
5. **UnlockDAG** — stages com `requires`/`provides`; Partial honesto; NVIDIA/USB/BT modelados.
6. **IDE/AI-Friendly** — schema JSON + template + goldens + Cursor rule.
7. **SDIO/HW Expert** — índice/roteador; não bring-up.
8. **Rede v3 (slot)** — estender `/market fetch` (ADR-0053) para recipes/FW quando houver volume; `LegoCatalogSource`.
9. **DeviceClass** — acrescentar `UsbHost` e `Bluetooth` (xHCI deixa de ser Video-only).

## Fora de escopo (agora)

ELF hot-load R1; MMIO livre em WASM; 171K recipes auto; App Store de drivers; partnership OEM falsa.

## Critérios

- [x] ADR + specs em `docs/specs/device-lego/`
- [x] Community hub + CALL + DANGERS + LICENSES
- [x] NeuralFS §12 + PackageHub kind
- [x] DeviceClass UsbHost/Bluetooth
- [x] Bind runtime H1: `device_recipe` + FAT gate + UnlockDAG tokens + HalOffer deny NeedsFw/Escalate
- [x] 4 goldens em `ecosystem/devices/` + FAT `LEGO*.MD` (`pack_device_legos.py`) + boot selftest
- [ ] Market fetch recipes e2e (volume gate) — residual v3

## Specs

Ver `docs/specs/device-lego/` e `docs/community/`.
