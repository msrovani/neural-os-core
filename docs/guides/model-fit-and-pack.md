# Model Fit e pack FAT (FitPolicy Neural)

Inspirado em [llmfit](https://github.com/AlexsJones/llmfit), sem portar o binário host para o kernel.

## Host — filtrar `PACK_LLM` antes do mkfat

```powershell
# Dry-run com RAM simulada
python tools/llmfit_pack_filter.py --dry-run --ram-mb 4096 --pack all

# Gate no pack: só empacota degraus Good+ (ou Marginal se nenhum Good)
$env:FIT_GATE = "1"
$env:PACK_LLM = "all"
python tools/mkfat32.py --size 3072 --output target/disk_qemu.raw
```

O filter **nunca sobe** degrau além do `PACK_LLM` pedido — só remove o que não cabe. Se `llmfit` estiver no `PATH`, o JSON inclui `advisory_llmfit` (não altera o pack).

Classes: Perfect (≤50%) · Good (≤80%) · Marginal (≤95%) · TooTight / Deny (>95% ou blob ausente).

## Guest — MemoryAgent + ModelHub

- `cortex::model_fit` (re-export `k_ai::model_fit`): footprints BitNet 2-bit + `score_fit` (fórmulas no cortex para evitar ciclo com ModelHub).
- MemoryAgent oneshot loga `[FIT] class=... usage=...`.
- `select_generator_slot` evita Pro/Fast se `TooTight`/`Deny` e cai para o maior slot aceitável carregado.

VRAM: honesty — se inventário ainda reporta 0, o fit usa só RAM.
