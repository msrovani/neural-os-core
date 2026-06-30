# GGUF Format Research — v0.60.2

## O que é GGUF
GGUF (GGML Universal Format) é o formato padrão para modelos quantizados do ecossistema llama.cpp.
Substitui o GGML (deprecated) e GGJT. Suporta Q4_0, Q5_1, Q8_0, etc.

## Estrutura do Arquivo GGUF

```
╔══════════════════════════════════════════════════╗
║ Header (24 bytes fixos)                         ║
║  - magic: u32 = 0x46554747 ("GGUF" little-end.) ║
║  - version: u32 (atual: 3)                      ║
║  - tensor_count: u64                            ║
║  - metadata_kv_count: u64                       ║
╠══════════════════════════════════════════════════╣
║ Metadata Key-Value Pairs (metadata_kv_count)     ║
║  Cada par:                                       ║
║  - key_length: u64 + key: [u8; key_length]       ║
║  - value_type: u32 (GGML_TYPE)                    ║
║  - value_data: (varia por type)                   ║
╠══════════════════════════════════════════════════╣
║ Tensor Info Array (tensor_count)                 ║
║  Cada entry:                                     ║
║  - name_length: u64 + name: [u8; name_length]    ║
║  - n_dims: u32                                   ║
║  - dimensions: [u64; n_dims]                     ║
║  - tensor_type: u32 (GGML_TYPE)                  ║
║  - offset_from_start: u64 (para dados binários)  ║
╠══════════════════════════════════════════════════╣
║ Padding (até alinhamento do offset mínimo)        ║
╠══════════════════════════════════════════════════╣
║ Tensor Data (dados binários brutos)              ║
║  Ordem: mesma dos Tensor Info entries            ║
╚══════════════════════════════════════════════════╝
```

## Tipos de Tensor (GGML_TYPE)

| Type | Value | Bits/Weight | Descrição |
|------|-------|-------------|-----------|
| GGML_TYPE_F32 | 0 | 32 | Float32 |
| GGML_TYPE_F16 | 1 | 16 | Float16 |
| GGML_TYPE_Q4_0 | 2 | 4.5 | 4-bit block quantizado (block_size=32, scale f16) |
| GGML_TYPE_Q4_1 | 3 | 5.0 | 4-bit block quantizado (block_size=32, scale + min f16) |
| GGML_TYPE_Q5_0 | 6 | 5.5 | 5-bit block |
| GGML_TYPE_Q5_1 | 7 | 6.0 | 5-bit block + min |
| GGML_TYPE_Q8_0 | 8 | 8.5 | 8-bit block |
| GGML_TYPE_Q8_1 | 9 | 9.5 | 8-bit block + min |

## Estrutura Q4_0 (mais comum)
Block de 32 pesos com scale f16:
```
struct BlockQ4_0 {
    scale: f16,          // 2 bytes
    weights: [u8; 16],   // 16 bytes (4 bits/weight × 32 = 16 bytes)
};
// Total: 18 bytes por 32 pesos = 4.5 bits/weight
```

## Estrutura Q8_0
Block de 32 pesos com scale f16:
```
struct BlockQ8_0 {
    scale: f16,          // 2 bytes
    weights: [i8; 32],   // 32 bytes
};
// Total: 34 bytes por 32 pesos = 8.5 bits/weight
```

## Desafios para no_std

1. **Endianness**: GGUF é sempre little-endian. Nosso kernel x86-64 também — OK.
2. **Heap**: Modelos pequenos (7B Q4_0 ≈ 3.5 GB) exigem >5GB heap.
   - Solução: mapear arquivo via ATA/USB em vez de carregar inteiro na RAM
   - Stub: carregar modelo via ATA PIO + page table mapping
3. **f16**: Rust no_std não tem tipo f16 nativo.
   - Solução: `u16` + função `f16_to_f32()` manual
4. **Block dequantization**: Cada tipo requer função específica.
   - Q4_0: `f32_result = (weight_4bit - 8) * scale`
   - Q8_0: `f32_result = weight_i8 * scale`

## Plano de Implementação

### Fase 1 (now): Parser do header + metadata (~150 LOC)
- `gguf.rs`: `GgufHeader`, `GgufMetadata`, `GgufTensorInfo`
- `load_gguf_header(data: &[u8]) -> Result<GgufFile>`
- Testar com modelo .gguf real (baixar modelo pequeno)

### Fase 2 (próximo): Q4_0 dequantization (~200 LOC)
- `dequantize_q4_0(block: &[u8]) -> [f32; 32]`
- `dequantize_tensor(info: &GgufTensorInfo, data: &[u8]) -> Tensor`
- Converter pesos GGUF → PackedTernaryTensor (reuso do pipeline .bitnet)

### Fase 3 (futuro): Streaming do ATA/USB (~150 LOC)
- `GgufStream` que lê blocos sob demanda
- Page table mapping para >4GB

## Referências
- https://github.com/ggerganov/ggml/blob/master/docs/gguf.md
- https://github.com/ggerganov/llama.cpp/blob/master/gguf-py/gguf/constants.py
- `tools/gguf_explorer.py` (futuro)
