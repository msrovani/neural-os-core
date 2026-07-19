# DATA_MAP — o que é aproveitável

| Dado | Aproveitável? | Vira no Neural |
|------|---------------|----------------|
| VID/DID PCI/USB | Sim | Bind / HW Expert |
| Classe pack SDIO (WLAN/Video…) | Hint | `DeviceClass` |
| Nome `.inf` Windows | Fraco | Normalizar → `family_id` |
| pci.ids / usb.ids | Sim | Descrição + seed |
| linux-firmware WHENCE | Sim | `fw_id` + download guide |
| Blob FW (GitLab) | Se redistribuível | `firmware/` + FAT + `blob_hash` |
| GSP NVIDIA | Só `target/` | Opt-in FAT; não PR git |
| RegMap / BMI / ACR | **Não** do SDIO | Recipe + Linux + medição |
| Connected / scan RF | **Nunca** do dataset | Só serial HW |

Ética: extrair HWID metadata; não republicar `.sys`.
