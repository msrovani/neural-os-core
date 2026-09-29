# SESSION_418 — ADR-0112: BAR Compute (VRAM universal vendor-agnostic p/ W2A8)

**Motivo:** demanda do maintainer — usar QUALQUER GPU (NVIDIA/AMD/Intel, iGPU/dGPU) para (a) armazenar pesos na VRAM e (b) acelerar o W2A8 do Falcon3, em 6h. A "muralha" conhecida: compute GPU exige firmware (ACR/PSP/GuC) + command streamer por geração + ISA de shader (ADR-0048/49/50 AWAITING_HW). A saída "por cima do muro": **a VRAM não precisa executar nada para servir o compute**.

## O insight

A aperture BAR é memória PCIe comum para a CPU. Três ganhos sem executar NADA na GPU:
1. **RAM liberada** — pesos (544MB/1B a ~1.5GB/3B W2) saem do heap bump (janela ~2030MB, OOM s415-417) → decode longo sem OOM (ganho estrutural imediato).
2. **Prefetch overlap** — GEMV no host lendo pesos via BAR com `prefetcht0` do tile j+1: latência PCIe (~1µs) esconde atrás do compute (~50µs/tile); ring de ativações em duplo buffer mantém o fluxo.
3. **Caminho único vendor-agnostic** — mesma matemática W2A8 (quant signed, round-half, col-major i8), bytes idênticos, só muda a origem (BAR vs RAM). Device path (CE/SDMA/BCS) = upgrade posterior com o mesmo contrato.

## Implementação

- **`k_hal/gpu/vram_stream.rs`** (novo): `StreamStage` honesto (Off→Mapped→StreamsW2a8→ComputeDevice); stream ring (4 slots × 8×9216 i8 = 288KB) no buddy da aperture; **canário round-trip golden** (pattern 64KB write+read-back + TSC → GB/s×10) — VRAM D3/barramento morto falha AQUI; `stream_x_slot` (host→VRAM sfence); telemetria lock-free.
- **`k_hal/gpu/bar_compute.rs`** (novo): `upload_layer_weights` (determinístico: alinhado 2MB pós-ring, dedupe por shape — 1 matriz por família cabe na aperture de 256MB sem ReBAR); `SHAPE_INDEX` (k,n)→mat; `gemv_from_vram` (quant host igual ao reference golden + `read_volatile` no BAR + prefetch ahead); `vram_ternary` = TernaryFn com slot alternante (duplo buffer).
- **`cortex/compute.rs`**: novo lane `VRAM_TERNARY` no dispatcher — ANTES do GPU device (não exige firmware), com `N_VRAM` na telemetria.
- **`cortex/cortex.rs`**: seam `register_vram_upload_hook` (mesmo padrão do model header hook — cortex não depende de k_hal) disparado no `set_model`; `current_model_layers_snapshot()` (refs sem clone, lifetime provado pelo slot static).
- **`main.rs`**: pós `init_vram_tier` OK → `init_bar_compute(g)` + registra o hook; `register_compute_if_ready` registra o lane se `bar_compute_enabled()`.
- **Honestidade:** stage Mapped NÃO chama `note_gpu_compute` — SYS_HEALTH gpu segue UNKNOWN; o slog diz "pesos residentes... GEMV lê via BAR", nunca "GPU acelerando".

## Compat vendor (zero código por vendor no stage Mapped)

NVIDIA BAR1 / AMD dGPU BAR0=VRAM (detect já resolve) / Intel iGPU = DRAM compartilhada → `Off` honesto (sem ganho inventado) / dGPU D3 → `init_vram_tier` recusa antes de tocar (SESSION_260). QEMU VGA dummy → canário falha → off.

## Verificação

- `cargo check --release -p boot` (bare-metal): **0 erros** (fix de asm: `prefetcht0 [reg]` — `({})` não parses no AT&T do LLVM).
- Testes: k-hal **232/232**, cortex **105/105**, k-nano **232/232**, hermes **257/257** (`--test-threads=1`).
- **Pré-existentes provados com stash:** `k_ai` STATUS_STACK_BUFFER_OVERRUN e `hermes` STATUS_PRIVILEGED_INSTRUCTION paralelo falham com E sem as mudanças (crash do harness host; lição nova no AGENTS).
- `gen_test_gguf.py` re-run p/ o teste de integração do cortex (gguf faltava após clean).

## Residual (upgrade path ADR-0112 §7)

1. `StreamsW2a8` — 2º canário com duplo buffer ativo + overlap medido.
2. `ComputeDevice` — CE Pascal (channel existe em nvidia_pascal_ce.rs) executa a GEMV lendo pesos residentes; SDMA RDNA / BCS Gen9 idem.
3. Lab: medir round-trip real na GTX 1050 (256MB aperture) + efeito no decode longo (RAM/estabilidade, NÃO tok/s).
4. iGPU mantém `Off` (DRAM compartilhada — ganho seria zero).
