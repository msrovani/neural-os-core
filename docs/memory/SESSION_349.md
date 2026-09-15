# SESSION_349 — Falcon3-3B WHPX measure + OOM UI pós-bench

**Sprint:** v1.9.99-s349 · **Bloco:** cortex / QEMU lab · **Data:** 2026-09-15

## Goal

Medir tok/s Falcon3-3B 1.58 (WHPX) com UI/FAT/HDA; fechar loader/OOM de boot; diagnosticar OOM `cortex_llm` ~4.4 GB na UI após bench OK.

## Feito

### Lab tok/s (WHPX)
- Script canónico: `tools/measure-falcon3-toks.ps1 -Accel whpx -RamGB 6 -Smp 4` (+ `-Window -WithFat -AudioBridge`)
- Modelo: `models\FALCON3.BIN` 22L/h=3072 via loader `@0x100000000`
- **Medido:** 2 toks / ~13.3 Ms → **milli≈150** (≈0.15 tok/s). `decode_tok/s` inteiro=0 é truncamento — usar `milli=`/`us=`
- TCG 3B: prefill timeout — N/A para tok/s

### Loader / heap (boot)
1. Probe QEMU-loader **mesmo em sandbox** (antes skip total)
2. Tamanho = **header v6**, não FAT (`FAT 2045126KB != header 1012764KB` → 7B mal-nomeado)
3. Parse **in-place** (sem `to_vec`+leak do blob ≈2× RAM)

### OOM UI T+793 (evidência)
```
[HEAP] refuse need=5692MB window=~2033MB (agente=cortex_llm)
[OOM/TALC] size=4764923932 align=4 agente=cortex_llm
```
- Bench já tinha passado; HUD dizia “2168MB RAM / ample headroom” (mentira vs pedido 4.4 GB)
- `size/4 = 1191230983` é **primo** → não é `rows×cols` limpo; classe wrap/`checked_mul` ausente + `max_seq` header **32768** (Instruct denso) vs lab 4096
- Máscara teórica 32768²×4 ≈ 4.0 GB — mesma ordem de grandeza
- Stamp `cortex_llm` pode ser enganoso se `submit` acorda AP/`poll_slice` ainda com tick do Cortex (InferQueue)

### Guards (esta sessão)
- `Tensor::new`/`zero`: `checked_mul` + cap elems; overflow → tensor vazio + slog (não wrap→OOM)
- `load_llm_v6`: heavy `max_seq>4096` → clamp 4096 + warn
- `embed_for_kv`: ctx_cap = generate (heavy≤512); mask com `checked_mul` + refuse

## Honesty
- HDA QEMU: codec degradado esperado (`Failed to configure capture path`)
- Ring3 P6 #PF boot = demo containment (não regressão tok/s)
- OOM root exato do `4764923932` (qual call site) ainda **aberto** se guards não cobrirem path — retest WHPX Window após rebuild

## Próximo
1. Rebuild + `measure-falcon3-toks.ps1 -Accel whpx -Window -WithFat` — confirmar sem OOM T+793
2. Se persistir: instrumentar `alloc` grande (>64MB) com backtrace/slog site antes do grow
3. InferWorker deve setar `tick_in_progress` próprio no `poll_slice` (stamp honesto)

## Lições
- Header `max_seq` de SKU denso **não** é contrato do 1.58bit lab — clamp no loader
- `layout.size` primo em f32 ⇒ overflow wrap ou byte-alloc; nunca assumir `a×b` sem `checked_mul`
- FAT size ≠ loader size: sempre preferir `v6_file_size` do magic @loader
