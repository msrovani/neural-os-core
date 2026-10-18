# SESSION_491 — Equiparação TII UAE → ModelHub (ADR-0078 §2.2)

**Sprint:** v1.9.99-s491 · **Data:** 2026-10-18

## Decisão

Formalizar o mapeamento das coleções [tiiuae](https://huggingface.co/tiiuae/collections) nas 8 gavetas do ModelHub + fora-hub, como equiparado na análise de produto (não download de blobs nesta sessão).

| Slot | Fonte | Função |
|---|---|---|
| Active | TII Falcon3-3B Instruct 1.58bit (alt. Falcon-E-3B) | chat default |
| GeneratorPro | TII Falcon3-7B / 10B 1.58bit | diagnóstico / heal |
| Learner | TII Falcon3-1B (alt. Falcon-E-1B) | fine-tune on-device |
| Vision | TII SigLino (legado SigLIP); OCR/Perception via pipe | encoder visual |
| Agent | *(vazio / não-TII)* — H1 só após loader | agentic → miss→Active |
| RustCoder | DeepSeek Coder BitNet | código |
| HwExpert | SDIO próprio | HWID AlwaysTiny |
| Reranker | BGE | RAG cross-encoder |

**Fora:** Falcon-H1/Mamba → não empacotar; Falcon ASR → STT; Laya/Jev → intent/router.

## Feito

1. ADR-0078 §2.2 reescrita (tabela TII + fora-gaveta).
2. README tabela hub alinhada.
3. `ModelSlot::from_name` aliases (`falcon_e`, `siglino`, `falcon_ocr`, `falcon_e_1b`…).
4. `fat_names_for(Vision)` inclui `SIGLINO.BIN`.
5. Comentário `mkfat32.py` espelha a política.

## Prova

- Host: `cargo test -p cortex --lib model_hub` (from_name / fat_names).
- Runtime: N/A (só contrato/docs + aliases).

## Honestidade

- SigLino / Falcon-OCR ainda **não** convertidos — legado `VISION.BIN` permanece.
- H1 = **fora** até motor SSM (não é “próximo rename”).
- Metal LOO mid-niche = residual s489.
