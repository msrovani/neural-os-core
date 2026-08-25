# SESSION_291 — Falcon3 7B alvo + escada 1B/3B/10B + AirLLM/GGUF

**Correção da premissa:** SKU 8GB/16GB não é política AIOS. Fit = RAM medida × footprint × heap.

## Família (tiiuae Instruct 1.58bit)

| Kind | Alvo | Carga |
|------|------|--------|
| 7B | **ideal** | residente se couber; senão GGUF AirLLM |
| 10B | extra | mesmo critério (40L) |
| 3B | fallback | residente típico |
| 1B | apertado | último residente |

AirLLM (`gguf_streaming::StreamingModel`) é **GGUF layer-wise**. `.v6` grande sem GGUF irmão → tenta o próximo nome (3B/1B). Residual: stream de BitNet v6.

`falcon3_boot_names()`: PRO.* primeiro. Hub **não** skipa GeneratorPro.
