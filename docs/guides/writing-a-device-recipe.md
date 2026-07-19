# Guia — Escrever um DeviceRecipe

1. Classificar: behaved (VirtIO) vs rebel (FW-MAC/GPU).
2. Copiar [RECIPE.template.md](../specs/device-lego/RECIPE.template.md) ou golden.
3. Preencher bind, firmware+`blob_hash`, stages `requires`/`provides`.
4. Validar [RECIPE.schema.json](../specs/device-lego/RECIPE.schema.json).
5. Sem signature → draft Escalate; com `verify_trusted` → candidato ativo.
6. Cite Linux/WHENCE; sem MMIO inventado.
7. Test plan: markers serial + HW alvo.

Ver [DANGERS.md](../community/DANGERS.md) e [TRUST.md](../specs/device-lego/TRUST.md).
