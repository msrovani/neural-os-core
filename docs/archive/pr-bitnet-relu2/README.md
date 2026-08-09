# PR BitNet: ativação relu² no 2B4T

**Bug:** `microsoft/bitnet-b1.58-2B-4T` declara `hidden_act: relu2`, mas o runtime
hardcoda `LLM_FFN_SILU` no FFN → logits errados (wrong-but-finite) em todos os
backends. Mesma classe do PR #586.

**Evidência (commit pinado do submodule `3rdparty/llama.cpp` = `390c3077`):**
- `src/models/bitnet.cpp:133` — `LLM_FFN_SILU` hardcoded no `build_ffn`.
- `src/llama-model.cpp:855` — mapa `LLM_FFN_OP_TYPES_FROM_STRING` sem `"relu2"`.
- `src/llama-graph.cpp:1690` — `LLM_FFN_RELU_SQR` (relu→sqr) **já implementado**;
  com gate `LLM_FFN_PAR` produz `relu²(gate)*up` = exatamente o `BitnetMLP` do HF.
- `llama_hparams::llm_ffn_op` + `LLM_KV_HIDDEN_ACT` já existem (padrão ModernBert).
- Conversores não gravam `<arch>.hidden_activation`.

## Status (2026-08-08)

- **Issue:** https://github.com/microsoft/BitNet/issues/602
- **PR conversores:** https://github.com/microsoft/BitNet/pull/603
- **PR C++ (upstream llama.cpp):** https://github.com/ggml-org/llama.cpp/pull/26751
  (adaptado ao master atual — a API mudou: `load_arch_hparams` + nested `graph`
  class; o padrão de leitura de `hidden_activation` espelha `modern-bert.cpp`)

## Arquivos

| Arquivo | Conteúdo |
|---|---|
| `fix-cpp.patch` | C++ — aplica em `isHuangXin/llama.cpp` (submodule): `src/models/bitnet.cpp` + `src/llama-model.cpp` |
| `fix-converters.patch` | Python — aplica em `microsoft/BitNet`: `utils/convert-hf-to-gguf-bitnet.py` + `utils/convert-ms-to-gguf-bitnet.py` |
| `issue.md` | Texto da issue (EN) para o microsoft/BitNet |

## Como submeter

1. **Issue:** abrir `issue.md` como issue no `microsoft/BitNet` (ou comentar na
   discussão #592 apontando para a issue).
2. **PR C++ (submodule):** fork de `isHuangXin/llama.cpp` → branch → aplicar
   `git apply fix-cpp.patch` → commit → PR. (O fix também se aplica ao
   `ggml-org/llama.cpp` upstream, onde o plumbing é idêntico.)
3. **PR conversores (microsoft/BitNet):** fork → branch → `git apply
   fix-converters.patch` → commit → PR. Se o PR C++ for aceito primeiro, incluir
   o bump do submodule no mesmo PR.

> **Nota:** os PRs #603 e #26751 já foram abertos via API (branches
> `fix/bitnet-relu2` no fork `msrovani`). O `fix-cpp.patch` em disco é a versão
> para o snapshot do submodule (`390c3077`); o PR #26751 usa a versão adaptada
> ao master do upstream.

## Verificação

```bash
# 1. converter o 2B4T com o conversor corrigido
python utils/convert-hf-to-gguf-bitnet.py microsoft/bitnet-b1.58-2B-4T out.gguf
# 2. conferir a metadata
python gguf-py/gguf/scripts/gguf_dump.py out.gguf | grep -i hidden
# 3. comparar logits antes/depois (com o fix o texto faz sentido; sem, é lixo)
```

GGUFs antigos (sem a key) continuam com SiLU — sem regressão.

## Origem

Descoberto durante a análise da v6 (SESSION_249): o nosso loader lê `act_type`
do header (relu2 para 2B4T); o upstream ignora o config e roda SiLU.