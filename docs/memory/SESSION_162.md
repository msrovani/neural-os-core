# SESSION_162 — BitNet ladder 850: #PF fix + BPE SP32 (coerência parcial)

**Data:** 2026-07-19  
**Versão:** v1.9.1 TEST  
**Pista:** LLM Chat BitNet 850 → 1.3 → 2B → 3B (custo/tempo/memória/inferência/coerência)

---

## Goal

Desbloquear generate no BitNet **850M** (1bitLLM/bitnet_b1_58-large): sem #PF pós-FWD, com tokenizer HF 32k, e pelo menos texto BPE decodificado (não CHAR `ABAB`).

## Problemas resolvidos

| Sintoma | Causa raiz | Fix |
|---------|------------|-----|
| #PF após layer 23/24 (unembed vocab=32002) | `avx2_bitwise_matmul` `_mm_storeu_ps` OOB quando `n % 4 != 0` | Desactivar bitwise; `avx2_ternary_matmul_impl` + **cauda scalar** `n%8` (`crates/cortex/src/bitnet_avx2.rs`) |
| `LLM ABSENT` / loader errado | Loader QEMU hardcode 2B; Active vs FAT size | Loader size = blob chat presente; hub skip FAT se Active loader |
| Dangling weights | Zero-copy slice do loader → drop Vec | Copy + `Box::leak` |
| Layout rms=0 | Heurística rem/need | v4 força `has_basic_rms=true` (bin `cortex.rs`) |
| `ABABAB…` / `bpe=0` | BPE **depois** do LLM-TEST; CHAR vocab 0–99 em embed 32002 | Carregar BPB1 **antes** do LLM-TEST |
| Encode `last=29874` (`▁ol`+`a`) | Greedy longest ≠ BPE HF | Export **MRG1** (61249 merges) + encode merge-order |
| Vocab Llama 128k no 850 | `bpe_vocab.bin` 2B | `bpe_vocab_sp32.bin` (SentencePiece 32k) no ladder 850/13/3b |

## Evidência QEMU/WHPX

```
BPB1 LOADED vocab_n=32002 … merges=61249 sp32=1
GEN … bpe=1 first=1 last=433   # ola → [<s>, ▁o, la] = HF
LLM-TEST #1/3 prompt='ola' … response='ol tath wh Holach stall wholach H stallol'
```

- **#PF:** ausente no generate 850  
- **Tokenizer:** alinhado a `tokenizers` HF  
- **Coerência semântica:** ainda fraca (base LM / forward residual) — **não** claim Ready

## Artefactos / tools

| Path | Papel |
|------|-------|
| `tools/llm_ladder_bench.py` | Pack FAT32 + QEMU loader + parse LLM-TEST |
| `tools/export_bpe_bin.py --sp32` | BPB1 + MRG1 → `target/bpe_vocab_sp32.bin` |
| `tools/mkfat32.py` / `build_image.py` | Default dados **3072 MB**; `PACK_LLM=850\|13\|2b\|3b\|all` |
| `tools/prepare_extra_models.py` | 850/xl/3B HF → `.bitnet` |

## Lições

1. **AVX2 store:** sempre cauda scalar se `n` não múltiplo do lane.  
2. **BPE antes de generate** no boot.  
3. **32k ≠ 128k** — ficheiro BPB1 tem de casar `model.vocab_size`.  
4. **Greedy ≠ BPE** — merges obrigatórias para encode correcto.  
5. AirLLM (GGUF layer-wise) ≠ BitNet full PIO/loader — lembrar na ladder 2B/3B.

## Residuais (próximo)

- Forward host vs kernel (mesmos pesos) — suspeita layout/attn se logits “sopa BPE”  
- Sampling: stop `</s>`; argmax puro SP32 sem bias Llama clima  
- Ladder 13 / 2b / 3b com o mesmo harness  
- Pack `BPESP32.BIN` no FAT HW real

## Relacionado

SESSION_142 ModelHub; ADR-0019 Cortex BitNet; IDEA #466.
