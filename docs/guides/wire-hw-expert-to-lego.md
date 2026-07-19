# Guia — Wire HW Expert → Device LEGO

```text
SDIO/HF datasets → labels v4 → train → HWEXPRT.BIN
  → boot Cortex load_model → HwCapabilityCard
  → draft RECIPE (Escalate) → HITL / verify_trusted
  → UnlockDAG stages → VERDICT medido
```

1. Baixar datasets HF **ou** extrair SDIO.
2. Merge pci.ids/usb.ids.
3. Rotular WiFi/GPU/USB (schema v4).
4. Treinar → FAT `HWEXPRT.BIN`.
5. Boot: se `NEEDS_FW` → sugerir golden da família (clone template).
6. Draft unsigned → HITL → bind L2.
7. Modelo **nunca** autoriza `VERDICT=PASS`.

Índice offline: `python tools/export_sdio_lego_index.py` (stubs Escalate).
