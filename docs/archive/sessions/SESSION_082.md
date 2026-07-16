> **Histórico** — Arquivado em 2026-07-16.
> Origem: `docs/memory/SESSION_082.md`
> Fonte atual: `docs/memory/SESSION_INDEX.md` · `docs/memory/STATE.md`

# SESSION 082 — RustCoder Expert: Treino + Integração Trinity MoE

**Data:** 2026-07-08
**Sprint:** 97
**Objetivo:** Treinar modelo expert Rust (hidden=128, 6 layers) com 60.8K amostras de código Rust e integrar como expert carregável no TrinityRouter.

## Resumo
- **41.200 amostras** carregadas (Rust-Coder: 10.800 + Rust-Code-Suite: 30.400)
- **1.599.872 parâmetros**, hidden=128, 6 layers, 8 heads, FFN=512, vocab=99
- **Loss 1.40 → 0.34** em 10 épocas na GTX 1050
- Modelo exportado como `tools/rust_coder.bitnet` (444 KB, formato bitnet v2)

## Dificuldades

### 1. CUDA + Python 3.14 incompatível
PyTorch não tem wheels oficiais para Python 3.14 (muito recente). Solução: usar nightly `torch-2.9.0+cu126` que tem build cp314. `CUDA_VISIBLE_DEVICES='0'` necessário explicitamente.

### 2. Compute Capability GTX 1050 (sm_61)
Nightly `cu128` dropou suporte para sm_61. `torch-2.9.0+cu126` ainda suporta Pascal.

### 3. Export format incompatível
Primeira versão do script exportava v4 com tensores nomeados — formato incompatível com o kernel. Solução: reescrever export para v2 puro (igual train_hw_model.py). 444 KB.

### 4. Mask device no forward pass
`torch.triu` criava mask na CPU, mas tensores estavam na CUDA → RuntimeError. Solução: `device=x.device` no mask creation.

### 5. Dataset encoding
Arquivo JSONL tem caracteres não-UTF-8. Solução: `open(..., encoding='utf-8', errors='replace')`.

## Decisões Arquiteturais

### RUSTCODER_MODEL global
Nova static `RUSTCODER_MODEL: spin::Mutex<Option<Box<dyn Model>>>` em cortex.rs — separada do CURRENT_MODEL principal. Expert não substitui o LLM geral, apenas complementa para consultas de código Rust.

### Fast-path no HermesAgent
Quando Trinity classifica como "rust_coder", HermesAgent tenta `generate_via_rustcoder()` primeiro. Se o modelo não estiver carregado (None), cai no fluxo normal do cortex.think().

### Loading da FAT32
Kernel lê `RUSTCDR.BITNET` da partição FAT32 durante boot, logo após carregar BITNET.BIN. Fallback silencioso se arquivo não existir.

## Arquivos Alterados
- `cortex.rs`: RUSTCODER_MODEL + set/generate functions
- `agents.rs`: Fast-path RustCoder no HermesAgent
- `main.rs`: Loading RUSTCDR.BITNET da FAT32
- `build_image.py`: Copia rust_coder.bitnet → RUSTCDR.BITNET
- `finetune_rust_llm.py`: Script de treino completo (reescrito do zero)

## Próximos Passos
- [ ] Treino completo com 60.800 amostras (hoje 41.200 — dataset de 50K não carregou completamente)
- [ ] Adicionar mais experts: disk_diag, security, hw_identify com modelos especializados
- [ ] Trinity com router_weight real (ML-based routing em vez de keyword matching)
