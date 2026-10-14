# SESSION_472 — #701 Modelo 1B QEMU (download, converter, medir)

Data: 2026-10-13. Medição de baseline do Falcon3-1B 1.58b nativo.

## Download
- Fonte: `tiiuae/Falcon3-1B-Instruct-1.58bit` (HF)
- `model.safetensors`: 1,357,042,252 bytes (~1.26 GB)
- SHA256: `3d536c681c4722e5263d413d85d243a1302217a245e4b55de93ece8a6ef30b15`
- Local: `target1/falcon3-1b/`

## Arquitetura (config.json)
| Parâmetro | Valor |
|-----------|-------|
| hidden_size | 2048 |
| num_hidden_layers | 18 |
| num_attention_heads | 8 |
| num_key_value_heads | 4 |
| intermediate_size | 8192 |
| vocab_size | 131072 |
| max_position_embeddings | 8192 |
| head_dim | 256 |
| rope_theta | 1000042 |
| tie_word_embeddings | false |
| hidden_act | silu |
| is_bitnet_config | true (nativo 1.58b) |
| lm_head | denso (não convertido) |

## Conversão .V7
- Saída: `target1/FALCON3_1B.V7` (726,856,228 bytes = 693.2 MB)
- Header: `act=SILU embed=Q6_K feat=0x0c tie=False theta=1000042.0`
- Tokenizer BPB1 embutido: 2,963 KB, vocab=131072, merges=128810
- Gate: byte-exato + BPB1 round-trip OK

## Suite Q1-Q6 (Host CPU, greedy + rep 1.15 + mask + falcon frame)
| Teste | Prompt | Veredito |
|-------|--------|----------|
| Q1 | "Qual é o seu nome?" | **COERENTE** |
| Q2 | "1+1=" | **DEGENERADO** (tem "2" mas verboso) |
| Q3 | JSON {"resultado": 2+2} | **DEGENERADO** (retorna 10.56) |
| Q4 | "O céu é" | **DEGENERADO** (sem "azul") |
| Q5 | "Como está o tempo?" | **DEGENERADO** |
| Q6 | "Olá, como vai?" | **COERENTE** |

**Resultado: 2/6 (VERMELHA)** — esperado para 1B (suite calibrada p/ 3B). Host tok/s: ~2.6-2.8 (Q1/Q6).

## QEMU Boot (4G/4c target)
- **Acelerador**: WHPX falhou (`Ignoring request for interrupt vector 0`) → fallback **TCG 2c**
- **Boot**: SUCESSO — Runtime + PostRuntime (BOOT SCORE OK)
- **Modelo carregado**: **Falcon3-3B** (989 MB) do QEMU loader @ `0x100000000`, **NÃO** o 1B do FAT32
  - Log: `LLM LOADED llama8b.bin (QEMU@4G in-place) size=1012764KB`
  - Loader varre endereços fixos (0x100000000, 0x13DF00000)
- **Prefill TCG 2c**: Layer 0/22 = **239 s**, Layer 1/22 = **234 s** — inutilizável
- **Decode tok/s**: não medido (TCG ~0.01 tok/s esperado)

## Comparação

| Métrica | 3B (S360/S455) | 1B Host Suite | 1B QEMU TCG |
|---------|----------------|---------------|-------------|
| Tamanho | 989 MB | 693 MB (.V7) | Não carregado |
| Host tok/s | ~2.5-3.0 | **2.6-2.8** (Q1/Q6) | N/A |
| QEMU WHPX tok/s | ~0.15-0.16 | Não medido (WHPX falhou) | N/A |
| QEMU TCG prefill L0 | ~8-10 s | N/A | **239 s** |
| Boot SMP | 4-6 cores | 2 cores (TCG) | 2 cores |

## Achados-chave
1. Pipeline conversão funciona (.V7 passa gate byte-exato + BPB1 + header)
2. 1B coerente no host (Q1/Q6 ~2.7 tok/s), mas Q2-Q5 falham asserções (calibração 3B)
3. WHPX indisponível neste host → TCG 2c; 3B carrega do loader QEMU
4. Para testar 1B no QEMU: desabilitar loader models OU colocar 1B no endereço do loader OU build `--mini`
5. TCG inutilizável para inferência (239s/layer → ~1.5h prefill completo)

## Próximo
#700 I2_S AVX2 path — requer HW com AVX2 (WHPX `-cpu host` expõe). Medir speedup real vs scalar.