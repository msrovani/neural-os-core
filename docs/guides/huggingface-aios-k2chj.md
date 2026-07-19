# Guia — HuggingFace `aios-k2chj`

## Org

https://huggingface.co/aios-k2chj

## Catálogo

| Artefato | Tipo | Uso LEGO |
|----------|------|----------|
| [hw-expert-v3](https://huggingface.co/aios-k2chj/aios-k2chj-hw-expert-v3) | Model | `HWEXPRT.BIN` / classificador |
| [sdio-hwids](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids) | Dataset | Índice bind / treino |
| [pci-usb-ids](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids) | Dataset | Seed labels |
| [firmware-metadata](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata) | Dataset | `fw_id` hints (não blobs) |
| [regulatory-db](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-regulatory-db) | Dataset | WiFi regdb |

Slots futuros: `hw-expert-v4`, `device-recipe-seeds`.

## Download

```text
huggingface-cli download aios-k2chj/aios-k2chj-hw-expert-v3 --local-dir target/hf/hw-expert-v3
huggingface-cli download aios-k2chj/aios-k2chj-sdio-hwids --repo-type dataset --local-dir target/hf/sdio
```

## HF vs GitLab vs GitHub

| Onde | O quê |
|------|--------|
| HF | `.bitnet`, datasets metadata |
| GitLab linux-firmware | Blobs device |
| GitHub | Código + recipes |

## Publish

Usar `tools/publish_hf_dataset.py` (sanitize). Card: `license:`, link GitHub, “how to help” (3 bullets), honesty: modelo **não** autoriza `VERDICT=PASS`.

## Wire

Seguir [wire-hw-expert-to-lego.md](wire-hw-expert-to-lego.md).
