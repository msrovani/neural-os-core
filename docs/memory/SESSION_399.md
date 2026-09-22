# SESSION_399 — Onda 5: §5 Áudio + §7 IA (loop TECNOLOGIAS)

**Canvas:** `docs/architecture/canvas-ondas34-gpu-storage-rede.md` (ctba.) + achados exp-8.

## Fixes aplicados (fixer)
- **H1** infer_queue race submit/claim: HEAD só avança se `occupied==true` antes do CAS; revalidação pós-CAS.
- **H2** Piper 22050→16000 (pipeline 16k coerente). **H3** Jarvis TTS com `TTS_PUSH_DROPPED` + `pos=pushed` (fim do drop silencioso).
- **M1** STT arquivo com `stt_blob_size()` derivado do header (sem 512KB fixo); **M2** `SttEngine::w()` fallback `&[]` (sem tensor errado); **M3** UB u8→i16 com `chunks_exact` em mixer/capture/skills; **M4** UAC iso usage=(data) data-only; **M6** H2O `sort_unstable` + take (O(n log n)); **M7** BITNET3B alias → Falcon3 Daily3B; **M9** projection round-to-nearest-even; **M10** pipeline_g5 TSC + 8x8 matmul; **M5** doc wakeword: detector RMS+MLP padrão 2 picos, sem dataset.
- **L1** piper conv1d dead deletado; L2 doc; L4 LOADED_HEADER `spin::Mutex`; SSE2 canary confirmado.

## Upstream (lib-6b)
wasmi/talc/smoltcp/ed25519/embedded-graphics no teto · BitNet-embeddings/VibeASR (I2_S AVX2) inspiradores para BGE e STT · Falcon3 oficial no bitnet.cpp · Medusa/EAGLE-3 com ngram embutido no llama.cpp · H2O/KV-quantização alinhados.

## Verificação
`cargo check -p cortex -p jarbas -p k-hal` 0 errors; sem DUP novo. Commit + tag `v1.9.99-s399`.
