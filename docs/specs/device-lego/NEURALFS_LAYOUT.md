# NeuralFS layout — Device LEGOs

PackageHub só fala com `/mnt/neural/ecosystem/` (não exFAT flat).

## Paths

| Peça | Kind | Path |
|------|------|------|
| DeviceRecipe | `device-recipe` | `/mnt/neural/ecosystem/devices/<name>/RECIPE.md` |
| Anexos | — | `.../devices/<name>/references/` |
| Firmware | `firmware` | `/mnt/neural/ecosystem/firmware/` |
| HW Expert | `model` | `/mnt/neural/ecosystem/models/` |
| BE R1 nativo | — | **bin** `k_hal` (não NeuralFS) |

```text
/mnt/neural/ecosystem/
  devices/
    qca6174-ath10k/RECIPE.md
    nvidia-gp108/RECIPE.md
    virtio-net/RECIPE.md
  firmware/
  models/
```

## Boot FAT ≠ NeuralFS

Bring-up precoce: short names no FAT (`AT10K_F6.BIN`).  
Pós-mount: catálogo PackageHub em `devices/` + `firmware/`.  
Sem mount: RAM + `persisted=false`.

## Depois da instalação

Install ≠ Ready.

1. Persistido → 2. Discover PCI → 3. Match recipe → 4. HalOffer+Cap → 5. UnlockDAG → 6. Runtime Port → 7. Heal/update → 8. Quarentena se hash/sig quebra.

FAQ: *“Instalei e não tem WiFi”* → checklist stages + serial.

## Slot rede v3 (encaixe — não implementar pleno agora)

Local first → `/market search` → `/market fetch` allowlist (HF `aios-k2chj`, mirror, NetFs peer) → sandbox → re-sign → NeuralFS → retoma DAG.

Interface futura: `LegoCatalogSource { Local, HttpAllowlist, NetFsPeer, SkynetMesh }`.

Base: ADR-0053 market net; SelfHeal I3; IDEA #134/#315.27. Volume gate antes de ranking Cortex.
