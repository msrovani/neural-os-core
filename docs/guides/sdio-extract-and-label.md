# Guia — Extrair e rotular SDIO HWIDs

## Goal

Produzir JSON de HWIDs sanitizado (sem `.sys`) e labels tipados para HW Expert v4 / índice LEGO.

## Pre-Flight

- Python 3.10+
- 7-Zip CLI (`7z.exe` no Windows — py7zr não cobre BCJ2)
- DriverPacks locais (não commitados)

## Commands

```powershell
python tools/extract_sdio_hw.py --dir <path-SDIODRIVERS>
python tools/download_hw_databases.py   # pci.ids + usb.ids merge
```

## Expected

- Contagem HWIDs > 0 antes de apagar `.7z`
- JSON só strings HWID / metadados — **zero PE**

## Labels (schema v4)

Campos: `vid`, `did`, `family`, `fw`, `caps`, `next_action`, `agent`  
Espelho: `tools/train_hw_expert_v4.py` + `k_ai::hw_capability`.

Prioridade: WiFi/GPU/USB. Ex.: `168C:003E` → `atheros_wifi` / ath10k (não Ath9k).

## Failure

- 0 `.inf` extraídos → usar `7z x -r`
- Pack corrompido → pular e reportar

## PR checklist

- [ ] Sem `.sys` / PE no artefato
- [ ] Counts no corpo do PR
- [ ] Licença/fonte citada ([LICENSES.md](../community/LICENSES.md))
- [ ] Labels perigosos: ver [DANGERS.md](../community/DANGERS.md)

## HF

Dataset canônico: https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids
