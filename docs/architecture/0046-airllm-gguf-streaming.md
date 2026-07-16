# ADR-0046: AirLLM + GGUF Streaming — Modelos LLM Layer-by-Layer

**Data:** 2026-07-16
**Status:** Proposta — análise completa, não implementado
**Depende de:** ADR-0028 (GGUF format research), ADR-0041 P9 (GGUF mmap), Sprint 96 (GGUF parser + GgufBackedModel)
**Sprint:** 108+

---

## 1. Contexto

### 1.1 O problema

Nosso heap é **512MB**. O modelo atual (BitNet 2B) ocupa ~202MB em ternário. Modelos maiores — Llama 8B (4.7GB Q4_0), Qwen 32B (18GB Q4_0), DeepSeek 671B — não cabem de forma alguma. Mesmo com compressão ternária, um modelo 7B teria ~700MB, excedendo o heap.

### 1.2 O que queremos

O pipeline visionado: Cortex/Hermes descobre um modelo LLM viável, baixa (a quente), converte, carrega e usa — **sem reboot** e **sem exigir que o modelo inteiro caiba em RAM**.

### 1.3 O que AirLLM mostra

AirLLM ([lyogavin/airllm](https://github.com/lyogavin/airllm), 22.7k stars) roda modelos **70B em GPU 4GB** — não porque comprime, mas porque **só mantém 1 layer por vez na GPU**:

```
Forward passo a passo (AirLLM):
  1. Carrega embedding → GPU (permanece)
  2. Layer 0: carrega pesos do disco → GPU → forward → descarta
  3. Layer 1: carrega pesos do disco → GPU → forward → descarta
  ...
  N. Unembed: carrega → GPU → gera token
  Próximo token: repete 2..N
```

A RAM/VRAM necessária = **1 layer + embeddings + KV cache**, não o modelo inteiro.

---

## 2. Decisão

Adotar o **GGUF Streaming Model** — variante do `GgufBackedModel` que mantém os pesos em disco (ATA/FAT32) e carrega **1 layer por vez** durante o forward.

### 2.1 Arquitetura

```
┌──────────────────────────────────────────────────────────────────┐
│                        CortexAgent                              │
│  generate_streaming(prompt) → String                            │
├──────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐    ┌────────────────────────────────────┐  │
│  │ GGUFHeaderCache │    │        GGUFStreamingModel          │  │
│  │ (em RAM, ~4KB)  │    │                                    │  │
│  │ • num_layers    │    │  forward_with_kv(tokens, cache):   │  │
│  │ • hidden_size   │    │    for each layer N:               │  │
│  │ • tensor_offsets│    │      buf = ata_read(layer_offset[N])│  │
│  │ • layer_sizes   │    │      weights = dequant(buf)        │  │
│  └─────────────────┘    │      h = transformer_layer(h, w)   │  │
│                          │      drop(weights)                 │  │
│  ┌─────────────────┐    └────────────────────────────────────┘  │
│  │   KV Cache      │                                           │
│  │ (em RAM fixa)   │    ┌────────────────────────────────────┐  │
│  │ • k_dim × layers│    │  PrefetchEngine (DMA background)   │  │
│  │ • v_dim × layers│    │  enquanto computa layer N:         │  │
│  └─────────────────┘    │    ata_read_async(layer_offset[N+1])│  │
│                          └────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 Fluxo de carga (cold start)

```
GGUFStreamingModel::load(path):
  1. ata_open(path)
  2. ata_read(0, 4KB) → parser header GGUF
     → num_layers, hidden, num_heads, vocab
     → tensor_info: [(name, offset, size, type)] para todas as layers
  3. Alloca KV cache: k_dim × num_layers × num_heads (RAM fixa)
  4.embed = ata_read(embed_offset, embed_size)
  5. Pronto para generate()
```

### 2.3 Forward streaming

```
fn forward_streaming(&self, tokens: &[u32], kv_cache: &mut KvCache) -> Vec<f32> {
    // Embedding (carregado uma vez)
    let mut h = self.embed.forward(tokens);

    for n in 0..self.num_layers {
        // Layer N: seek + read + dequant + compute
        let (offset, size, qtype) = self.layer_info[n];

        // Se prefetch da layer N já chegou, usa; senão, seek+read síncrono
        let buf = self.prefetch.take(n)
            .unwrap_or_else(|| ata_read(offset, size));

        // Dispara prefetch da PRÓXIMA layer em background DMA
        if n + 1 < self.num_layers {
            let (next_off, next_sz, _) = self.layer_info[n + 1];
            self.prefetch.submit(next_off, next_sz);
        }

        // Dequantiza
        let weights = match qtype {
            GgufType::Q4_0 => dequantize_q4_0(&buf),
            GgufType::F32 => tensor_from_f32(&buf),
            // Q5_0, Q8_0, etc. — progressivo
        };

        // Forward desta layer
        h = transformer_layer(h, &weights, &kv_cache.layers[n]);
        drop(weights);  // libera ~2-10MB
    }

    // Unembed
    h = self.unembed.forward(h);
    h
}
```

### 2.4 Hot swap — pipeline de download + conversão

```
HermesAgent recebe:
  "usa o modelo Qwen3-32B via GGUF"

1. NetAgent.http_get("huggingface.co/.../Qwen3-32B-GGUF/Q3_K_M.gguf")
   → stream para ATA: fs.write_file("QWEN32B.GGUF", data)

2. GGUFStreamingModel::load("QWEN32B.GGUF")
   → lê header, detecta arquitetura (Qwen), preenche layer_info

3. set_model(Box::new(streaming_model))
   → CortexAgent.generate() agora usa modelo novo

4. (Opcional) Se RAM suficiente:
   Background: converte Q4_0 → ternário (.bitnet)
   → load_model() → set_model() → streaming_model descartado
```

---

## 3. Implementação

### 3.1 O que já existe (reutilizar)

| Componente | Arquivo | Linhas | Status |
|-----------|---------|--------|--------|
| Parser GGUF (header, metadata, tensor info) | `gguf.rs` | 1-260 | ✅ Completo |
| Q4_0 dequantization | `gguf.rs` | 273-309 | ✅ Completo |
| f16 → f32 | `gguf.rs` | 259-270 | ✅ Completo |
| ATA read (PIO, setorial) | `ata_pio.rs` | — | ✅ Completo |
| Transformer forward (RMS, QKV, FFN, RoPE) | `cortex.rs` | 330-800 | ✅ Completo |
| KV cache alloc | `cortex.rs` | — | ✅ Completo |
| Model trait (generate, embed_dim, etc.) | `cortex.rs` | 1709-1717 | ✅ Completo |
| `/model` command (agents.rs) | `agents.rs` | 822-827 | ✅ Completo |

### 3.2 O que precisa ser implementado

| Componente | LOC | Descrição |
|-----------|-----|-----------|
| `GGUFStreamingModel` struct | ~80 | layer_info: Vec<(offset, size, type)>, embed, unembed |
| `GGUFStreamingModel::load(path)` | ~100 | Lê header, preenche layer_info, carrega embed+unembed |
| `forward_streaming()` | ~150 | Loop de layers com seek+read+dequant+forward |
| `PrefetchEngine` | ~100 | DMA background, double buffer, submit/take |
| Hot swap pipeline (download → load) | ~100 | NetAgent stream → ATA → detect → load |
| Dequant para mais tipos (Q5_0, Q8_0, F16) | ~100 | Cada tipo precisa de função de dequant |
| **Total** | **~630** | |

### 3.3 Impacto no desempenho

O gargalo passa de **matmul CPU** para **I/O ATA**:

| Operação | Latência aproximada |
|----------|-------------------|
| ATA seek | ~3ms (PIO, setorial) |
| ATA read 10MB (1 layer Q4_0) | ~20ms (PIO mode) |
| Dequant Q4_0 10MB → f32 | ~5ms (CPU) |
| Forward 1 layer (hidden=4096) | ~10ms (CPU ternário) |
| **Total sem prefetch** | **~38ms/layer × 32 layers = ~1.2s/token** |
| **Total com prefetch** | **~15ms/layer × 32 layers = ~0.5s/token** |

Com prefetch (DMA sobrepõe I/O com compute), o custo por token ≈ max(I/O, compute) + seek, não a soma.

Para modelos menores (8B: ~16 layers, ~4MB/layer): ~200ms/token com prefetch.

### 3.4 Prioridade de tipos GGUF suportados

| Tipo | Bits/weight | Tamanho 8B | Status |
|------|------------|-----------|--------|
| **Q4_0** | 4.5 | ~4.7GB | ✅ Dequant pronto |
| **Q5_0** | 5.5 | ~5.7GB | 🔧 Precisa dequant |
| **Q8_0** | 8.5 | ~8.5GB | 🔧 Precisa dequant |
| **F16** | 16 | ~16GB | ✅ f16→f32 pronto |
| **F32** | 32 | ~32GB | ✅ Raw f32 pronto |
| **Q2_K** | 2.6 | ~2.7GB | ⏳ Futuro (llama.cpp específico) |
| **Q3_K** | 3.5 | ~3.6GB | ⏳ Futuro |
| **IQ4_NL** | 4.5 | ~4.5GB | ⏳ Futuro |

---

## 4. Pipeline Hot Swap Completo

```
                    Cortex/Hermes descobre que precisa de um LLM melhor
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 1. DESCOBERTA                                                       │
│    HermesAgent.scan_huggingface() → lista modelos compatíveis      │
│    Filtro: GGUF, Q4_0, <100GB, arquitetura conhecida (Llama/Qwen)  │
└─────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 2. DOWNLOAD (a quente, sem reboot)                                  │
│    NetAgent.http_get(url, stream=true)                              │
│    fs.write_file("MODEL.GGUF", chunk)  ← ATA PIO write             │
│    Barra de progresso no display                                    │
└─────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 3. DETECÇÃO                                                         │
│    GGUFStreamingModel::load_header("MODEL.GGUF")                    │
│    → num_layers, hidden, arch (Llama, Qwen, DeepSeek...)            │
│    → Verifica: cabe no KV cache? layers conhecidas?                 │
└─────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 4. CARGA                                                            │
│    set_model(Box::new(GGUFStreamingModel::new("MODEL.GGUF")))       │
│    → CortexAgent.generate() agora usa o modelo novo                 │
│    → Sem reboot, sem parar o scheduler                              │
└─────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 5. (OPCIONAL) CONVERSÃO BACKGROUND                                  │
│    Se RAM suficiente para o modelo ternário:                        │
│    GgufBackedModel::convert_to_bitnet() → .bitnet                   │
│    → load_model() → set_model()                                     │
│    → streaming substituído por carga total (10-50× mais rápido)     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 5. Comparação: Carga Total vs Streaming vs AirLLM

| Aspecto | Carga total (hoje) | GGUF Streaming (nossa) | AirLLM original |
|---------|-------------------|----------------------|-----------------|
| **Modelo max** | ~1.5GB Q4_0 | **Qualquer** (70B, 405B, 671B) | Qualquer |
| **RAM por forward** | Modelo inteiro | **1 layer (~10MB)** | 1 layer (~VRAM) |
| **Latência/token** | ~50ms | ~200-500ms (ATA bound) | ~100ms (GPU bound) |
| **Swap a quente** | `load_model()` inteiro | `set_model()` instantâneo | `from_pretrained()` |
| **Requer GPU** | ❌ | ❌ | ✅ (CUDA) |
| **Prefetch** | N/A | ✅ DMA background | ✅ threading |
| **Dequant suportado** | Q4_0, F32 | Q4_0, F32 (+ progressivo) | 4bit, 8bit block-wise |
| **Formato** | .bitnet + GGUF | **GGUF nativo** | HuggingFace safetensors |
| **Manutenção** | 595 LOC (gguf.rs) | +630 LOC novos | Python + PyTorch |

---

## 6. Riscos e Mitigações

| Risco | Impacto | Mitigação |
|-------|---------|-----------|
| ATA PIO seek lento (~3ms) | +3ms/layer = +96ms/token (32 layers) | Prefetch cobre seek no background |
| ATA PIO sem DMA no QEMU | Sem prefetch real | WHPX acelera; HW real tem DMA |
| Modelo >4GB não cabe em FAT32 (256MB disk) | Não dá para baixar | Usar QEMU-loader `-device loader` para modelos grandes; ou particionar ATA |
| KV cache cresce >512MB (seq_len longo) | OOM | Limitar max_seq; usar cache eviction |
| Tensor names diferentes por arquitetura | Parser não encontra `blk.0.attn_q` | Mapear por `config.json` (AutoModel dispatch) |
| Dequant lento em CPU | Token/s baixo | Q4_0 já é rápido (shift + mask + LUT); AVX2 acelera |

---

## 7. Conclusão

O GGUF Streaming Model permite que neural-os-core rode **qualquer modelo LLM do ecossistema llama.cpp** independentemente do tamanho, usando apenas ~10MB de RAM por forward. O custo é latência (ATA I/O bound), mitigado por prefetch DMA.

A implementação é incremental sobre código já existente:
- Parser GGUF: ✅ pronto (595 LOC)
- Q4_0 dequant: ✅ pronto
- ATA read: ✅ pronto
- Transformer forward: ✅ pronto
- **Novo**: `GGUFStreamingModel` + `forward_streaming()` + `PrefetchEngine` + hot swap pipeline → ~630 LOC

O pipeline de hot swap (download → detect → load → use) é a peça que faltava para o HermesAgent buscar e ativar modelos LLM externos sem reboot, realizando a visão de auto-upgrade cognitivo.

---

## 8. Referências

- **AirLLM:** [lyogavin/airllm](https://github.com/lyogavin/airllm) — Layer-wise inference, 70B em 4GB GPU
- **GGUF format:** [ggml/docs/gguf.md](https://github.com/ggerganov/ggml/blob/master/docs/gguf.md)
- **ADR-0028:** `docs/architecture/0028-gguf-format-research.md` — Pesquisa inicial do formato
- **ADR-0041 P9:** `docs/architecture/0041-k2chj-capability-rings.md` — GGUF mmap PoC
- **Llama.cpp:** [ggerganov/llama.cpp](https://github.com/ggerganov/llama.cpp) — GGUF reference implementation
- **gguf.rs:** `crates/neural-kernel/src/gguf.rs` (595 LOC) — Parser + GgufBackedModel atual
- **gguf_mmap.rs:** `crates/neural-kernel/src/gguf_mmap.rs` (275 LOC) — P9 file-backed mmap PoC
