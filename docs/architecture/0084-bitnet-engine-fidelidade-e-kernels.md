# ADR-0084: Engine BitNet — Fidelidade Arquitetural, Kernels CPU e Receita de Treino 1-bit

**Data:** 2026-08-04
**Status:** Proposed
**Lifecycle:** `por_fazer`
**Fonte:** Estudo externo (microsoft/BitNet + bitnet.cpp, arXiv 2504.12285 2B4T, arXiv 2511.21910 Platinum, Deveraux-Parker/nanoGPT_1GPU_SPEEDRUN, hestia2026/Hestia) cruzado com auditoria interna do stack `cortex`
**Ideias:** relaciona #126–156 (cortex), #375–377 (GGUF), #479–490 (CPU-first); novas a registrar no IDEA_BANK
**Substitui:** — (complementa ADR-0011, ADR-0012, ADR-0019, ADR-0061)

---

## 1. Contexto

O engine BitNet do Neural-OS (`cortex`: ternário 2-bit, `PackedTernaryTensor`, kernels AVX2/AVX512/SSE,
MoE Trinity, GGUF loader, conversão de modelos 2B4T/Falcon3/LLaMA-8B) foi auditado contra o estado
da arte externo em quatro frentes: (a) repositório oficial microsoft/BitNet (hoje = bitnet.cpp),
(b) o paper do modelo que já convertemos (BitNet b1.58 2B4T), (c) otimização de inferência CPU do
bitnet.cpp (`src/README.md`), (d) dois repos de treino (nanoGPT speedrun, Hestia QAT) e um paper de
hardware (Platinum).

Resultado do cruzamento:

1. **Mismatches arquiteturais ativos** entre nosso forward e o modelo 2B4T real (silu vs ReLU²,
   SubNorms ausentes, RoPE theta, embedding ternário) — o forward otimizado computa uma função
   diferente da treinada.
2. **Otimizações de kernel conhecidas e com números** (unpack branchless, acumulador em
   registrador, activation-parallel, tiling) que se aplicam 1:1 ao nosso packing, sem mudar formato.
3. **Receita de treino 1-bit** consolidada (STE/latent/SubLN/ReLU² do paper + tanh-scaling/LR
   separados do speedrun + QAT por expectativa suave do Hestia) para o próximo treino de modelos
   próprios.
4. **Material descartado** com justificativa (ASIC Platinum, CUDA/FP8, truques de arquitetura fp).

Esta ADR registra o diagnóstico, a ordem de implementação acordada e as decisões de política.

---

## 2. Diagnóstico — Fidelidade Arquitetural (2B4T vs nosso forward)

Config real do 2B4T (config.json + safetensors stats + modeling_bitnet.py): hidden 2560, 30 layers,
20 heads / 5 KV heads (GQA, head_dim 128), FFN 6912 **GLU com ReLU²**, RoPE theta **500000**,
RMSNorm eps 1e-5, **sem bias**, vocab 128256 (LLaMA-3 BPE), **tie embeddings**, embed/lm_head em
**BF16** (328.8M valores, zero pesos ternários fora dos linears), **sem scales salvos** (per-tensor,
derivado na conversão).

| # | Item | Oficial | Nosso estado | Impacto | Prioridade |
|---|---|---|---|---|---|
| M1 | Ativação FFN | `down(ffn_sub_norm(relu2(gate(x))·up(x)))` | `silu(gate)·up` (cortex.rs:901, nn.rs) | Forward computa função **diferente da treinada** | 🔴 corrigir |
| M2 | SubNorms | 4 RMSNorms/layer (attn_sub_norm, ffn_sub_norm inclusos) | 2–3 norms; `rms_inner_attn`/`ffn_layernorm` opcionais no formato — **alinhamento a verificar** | Escala/saída diverge | 🔴 corrigir |
| M3 | RoPE theta | 500000 | default 10000 (provavelmente hardcoded; `layer_features bit2` só liga/desliga RoPE) | Frequências ~50× fora | 🔴 parametrizar no header |
| M4 | Embedding | BF16 tied | ternário packed (conversor) | **I2_S (ternário) em embedding = N/A — modelo quebra** (bitnet.cpp src/README); Q6_K ≈ grátis (17.149 vs 17.109 PPL) | 🔴 converter p/ Q6_K |
| M5 | Scale | per-tensor derivado, não salvo | 1 f32/tensor vestigial | **OK** — absorvido pelas RMSNorms seguintes (RMSNorm é invariante a escala) | — |

**Política derivada (P1):** corrigir a fidelidade (M1–M4) **antes** de otimizar velocidade —
otimizar a função errada é custo duplo. A correção é numérica, não de formato de pesos: nada de
retreino (ver §6).

---

## 3. Decisões — Ordem de Implementação (acordada)

### Fase 1 — Kernel decode (m=1): unpack branchless + acumulador em registrador

**Evidência (bitnet.cpp src/README, EPYC 7V13 1 thread):** p/ [1,2048]×[2048,2048] **weight-parallel
vence**: 0.058ms vs 0.075 sem paralelo (1.29×); activation-parallel até piora (0.076). Nosso
decode já é weight-parallel estruturalmente (1 ativação, varre N em blocos de 8) — falta:

- **Unpack sem branch**: portar `(pair&1)-(pair>>1)` do AVX-512 (bitnet_avx512.rs:34) para o path
  AVX2 (hoje `match` por peso, bitnet_avx2.rs:139-142).
- **Acumulador em registrador**: hoje reload/store f32 por t (bitnet_avx2.rs:186-188); manter
  acumulador YMM por bloco de N e dar flush no fim.

**Alvo:** `crates/cortex/src/bitnet_avx2.rs` (~40-80 LOC). Risco baixo (tail n%8 existe).

### Fase 2 — Kernel prefill (m≥8): activation-parallel (t→unpack 1×→i→j)

**Evidência:** p/ m≥32 activation-parallel vence ~2× (128×2048×2048: 10.82→5.81ms; 512: 43.26→23.34).

- Nosso `avx2_ternary_matmul_impl` (bitnet_avx2.rs:162-200) tem ordem i→t→j com `unpack_row_into`
  por (i,t) → m×k desempacotamentos no prefill. Inverter para t→i→j: unpack da linha t **uma vez**,
  FMA com m ativações.
- **Blueprint já existe em casa**: `avx2_bitwise_matmul` (L96-122) carrega 4 ativações f32 + LUT
  byte→4 pesos; foi desativado por **bug OOB** de store quando `n%4!=0` (SESSION_162) — reativar com
  guard de cauda + memoização do unpack por t + tratamento dos 2 layouts de packing (row-boundary vs
  flat vocab 32002).
- **Dispatch por m**: `dispatch_ternary` (compute.rs:59) não distingue m; selecionar
  `m==1 → weight-parallel` / `m≥8 → activation-parallel` no kernel (sem tocar o dispatch).

**Alvo:** `bitnet_avx2.rs`, `compute.rs`, `tensor.rs`. ~80-150 LOC. Risco médio (bug histórico é
nesta área — verificação via `bitnet_fwd_parity.py`).

### Fase 3 — Embedding Q6_K + correções de fidelidade (M1–M4)

- **M4 (embed)**: `tools/convert_bitnet.py` — embed BF16→**Q6_K** (não ternário); loader
  (`cortex.rs` embed path) com dequant Q6_K (dequant já existe em `gguf.rs` K-quants); **bump de
  versão do formato `.bitnet`**; custo **+190MB RAM** no slot 2B (82→270MB; heap cap 2GB OK).
  Sem retreino — re-conversão offline.
- **M1 (relu2)**: `relu2(x)=max(x,0)²` — só mul, sem FPU. no_std-safe.
- **M2 (SubNorms)**: alinhar as 4 RMSNorms/layer do 2B4T ao forward; validar contra o formato
  (`rms_inner_attn`, `ffn_layernorm` já existem no header).
- **M3 (theta)**: parametrizar RoPE theta no header `.bitnet` (default 10000 para modelos próprios;
  500000 para 2B4T).

**Verificação:** `tools/bitnet_fwd_parity.py` (paridade host vs kernel) + `cargo check --release`
+ boot QEMU com 2B.

### Fase 4 — Kernel I2_S/maddubs oficial (W2A8) — GATED, depois

O kernel oficial (ggml-bitnet-mad.cpp): unpack shift+mask (sem branch), `_mm256_maddubs_epi16`
(u8×i8→i16, 32 MACs/instrução), acumulação i32, scale f32/linha + si per-token no epílogo com
desconto do viés {0,1,2,3}→{-1,0,1}. Ganho ~2-4× sobre nosso path f32-FMA; é o que dá os 29ms
TPOT/0.028J do 2B4T.

**Não fazer agora**, porque:
1. Exige pipeline de ativação int8 (redução absmax por matmul) — mudança numérica e estrutural.
2. 2B4T é nativa a8 (sem degradação), mas Falcon3/LLaMA-8B RTN-convertidos degradam — possível
   fine-tune para recuperar.
3. **Ganho nulo sob TCG** (dev default sem AVX2) — só rende em WHPX/HW real.
4. Os limitadores reais de qualidade estão fora do matmul: `soft_stride=3`, `MAX_SEQ=64`,
   geração 4-8 tokens.

**Gate de abertura:** (a) Fases 1–3 completas e verificadas; (b) gaps de geração/contexto
endereçados; (c) execução primária em WHPX/HW real. ~150-300 LOC + plumbagem de scales.

### Fase 5 — Tiling configurável (a qualquer momento)

`ROW_BLOCK_SIZE=4`, `COL_BLOCK_SIZE=128`, `PARALLEL_SIZE=4` como consts de tuning por HW
(faixas: p∈[2,4,8], row∈[2..32], col∈[32..1024]). Na prática: `COL_BLOCK` para cache; `ROW_BLOCK`
para o weight-parallel do decode.

---

## 4. Decisões — Receita de Treino 1-bit (aplicável ao PRÓXIMO treino, sem retreinar nada existente)

Evidências: 2B4T paper (STE + latent weights + absmean/absmax + SubLN + ReLU², SFT com loss SUM,
DPO β=0.1, LR 2-stage com cooldown, "treino nativo 1-bit > PTQ de modelos 4-8× maiores" — Tabela 3);
nanoGPT_1GPU_SPEEDRUN (GPT-2 124M val 3.25 em ~90min/1×4090); Hestia (QAT por expectativa suave).

| Item | Origem | Porta para | Prio |
|---|---|---|---|
| **Tanh logit scaling** `30*tanh(x/30)` — estabiliza precisão mista | speedrun | `tools/train_*.py` + `BitNetTrainer` | 🔴 alta |
| **LR constante + cooldown linear, zero warmup** (alinha com 2-stage 2B4T) | speedrun + 2B4T | schedules host + `BitNetTrainer` | 🔴 alta |
| **LRs separados por tipo de param** (embed ~10× alto, head baixo, betas (0.8,0.95)) | speedrun | treinos host | 🔴 alta |
| **QAT por expectativa softmax** sobre codebook {-1,0,1} com pressure/anneal (`compress_ratio=0.2`, `anneal_ratio=0.8`, cosine→0) — ataca o **dead-zone do STE** em modelos <3B | Hestia | `tools/train_*.py` (host); no_std só versão lerp sem exp | 🔴 alta |
| Muon + Newton-Schulz 5 (convergência ~2×; 5 matmuls/step — experimento em 128×64) | speedrun | host | 🟡 média |
| Grad accumulation `grad /= micro_batches`; curriculum de janela 256→1792 | speedrun | host (GTX 4GB) | 🟡 média |
| Q/K RMSNorm antes do RoPE (se ausente) | speedrun | `cortex/src/nn.rs` | 🟡 média |

Escopo: **host (PyTorch)** para HW Expert v6 / router v2 / novos modelos; `BitNetTrainer` on-device
recebe apenas o que porta barato (cooldown LR, lerp pressure). Nada disso muda modelos em produção.

---

## 5. Descartado (com justificativa)

| Item | Fonte | Motivo |
|---|---|---|
| **Platinum ASIC** (2511.21910) | arXiv | Hardware ASIC (cs.AR) p/ mpGEMM LUT — sem fabricação no projeto; a única ideia útil (LUT/shuffle_epi8) já está coberta pelo T-MAC nas Fases 1-2 |
| CUDA kernels W1.58A8, FP8/torchao, TF32, torch.compile, flex_attention, CUDA streams/pinned memory | BitNet/speedrun | Exigem CUDA/GPU/compile — fora do runtime no_std |
| MQA, U-Net skips, value embeddings, smear | speedrun | Truques de arquitetura fp de 124M; não se aplicam a ~1M ternário |
| Hutch++ / Hessiana offline | Hestia | HVP/autograd — inviável no device e caro no host p/ 1M params; proxy barato (variância/‖W‖₁) como alternativa se preciso |
| PhaseQuantizer, bitwidth intN assimétrico | Hestia | Especulativo; só se formos além de ternário |
| BitTorrent/merkle (de estudos anteriores) | ADR-0081 | Reconfirmado como fora de escopo |

---

## 6. Decisões de Política

1. **Fidelidade primeiro, velocidade depois** — M1–M4 antes das Fases 1-2; otimizar a função errada
   é custo duplo.
2. **Nenhum retreino de modelos existentes** — correções são numéricas (fidelity) ou de conversão
   offline (Q6_K, bump de versão do `.bitnet`). Receita de treino aplica-se ao próximo modelo.
3. **W2A8 gated** — só com WHPX/HW real primário + gaps de geração resolvidos (§3 Fase 4).
4. **Embedding fora do ternário** — I2_S em embeddings é falha conhecida (N/A); embed vai para Q6_K.
5. **Treino nativo 1-bit > PTQ** (2B4T Tabela 3) — direção para modelos próprios, reforçada por
   speedrun (eficiência de treino) e Hestia (QAT suave).
6. **Honestidade** — sem mudança de formato que quebre arquivos legados sem bump de versão; manter
   `bitnet_fwd_parity.py` como gate de paridade para todo item numérico.

---

## 7. Custos

| Fase | Esforço | LOC | Risco | Custo runtime |
|---|---|---|---|---|
| 1. Decode | ~1 sessão | 40-80 | baixo | −1.29× no decode; 0 RAM |
| 2. Prefill | ~1 sessão | 80-150 | médio (bug OOB histórico) | −2× p/ m≥8; gate por m p/ não regredir decode |
| 3. Fidelity + Q6_K | 1-2 sessões | ~100-160 + conversor | baixo-médio (bump formato) | **+190MB** RAM slot 2B; arquivo 605→~800MB |
| 4. W2A8 | 2+ sessões | 150-300 + scales | médio (numérica) | −2-4× (só WHPX/HW real) |
| 5. Tiling | <1 sessão | ~20 | baixo | tuning |

Total Fases 1-3: ~1-2 dias de trabalho, **zero custo financeiro, zero retreino**.

---

## 8. Riscos

- **Fase 2 regressão de decode** se o gate por m falhar (activation-parallel é pior em m=1) — mitigado
  por seleção `m==1 → weight-parallel` + parity.
- **Bump de formato `.bitnet`** — arquivos legados devem continuar carregáveis (leitor com fallback
  de versão) ou ser re-convertidos; mitigado por bump de versão + re-conversão script.
- **Q6_K embed** aumenta RAM do slot 2B — dentro do heap cap 2GB; verificar com `-NoDisk` TCG boot.
- **Fidelity M2** — alinhar 4 RMSNorms/layer pode expor divergências de escala acumuladas; parity é
  o gate.

---

## 9. Checklist de Aceite

- [ ] M1: relu2 no forward (nn.rs/cortex.rs) com parity 2B4T
- [ ] M2: SubNorms alinhadas ao 2B4T com parity
- [ ] M3: theta parametrizado no header `.bitnet` (default 10000, 2B4T=500000)
- [ ] M4: embed Q6_K no conversor + loader + bump de versão; re-conversão do 2B
- [ ] Fase 1: decode branchless + acumulador em registrador (parity + boot)
- [ ] Fase 2: activation-parallel gated por m (parity + boot; sem regressão decode)
- [ ] Fase 5: consts de tiling
- [ ] `cargo check --release` 0 erros a cada fase
- [ ] Boot QEMU 2B (`-NoDisk` TCG + WHPX) com geração viva
- [ ] INDEX.md, IDEA_BANK e TECNOLOGIAS.md atualizados

---

## 10. Referências

- [microsoft/BitNet](https://github.com/microsoft/BitNet) — `src/ggml-bitnet-mad.cpp`, `include/gemm-config.h`, `src/README.md`
- [arXiv 2504.12285](https://arxiv.org/abs/2504.12285) — BitNet b1.58 2B4T Technical Report
- [microsoft/bitnet-b1.58-2B-4T](https://huggingface.co/microsoft/bitnet-b1.58-2B-4T) — config.json + safetensors stats
- [arXiv 2511.21910](https://arxiv.org/abs/2511.21910) — Platinum (ASIC; descartado)
- [Deveraux-Parker/nanoGPT_1GPU_SPEEDRUN](https://github.com/Deveraux-Parker/nanoGPT_1GPU_SPEEDRUN) — `train_gpt2_4090_90min_3_25loss.py`
- [hestia2026/Hestia](https://github.com/hestia2026/Hestia) — `thermo_quantizer.py`, `thermo_scheduler.py`
- Relacionadas: ADR-0011 (BitLinear), ADR-0012 (packing), ADR-0019 (cortex), ADR-0061 (CPU-first), ADR-0057 (dispatch), ADR-0083 (gap IA); SESSION_162 (bug bitwise OOB)

---

## 11. Revisão (2026-08-04) — validação independente pré-implementação

Revisão em duas frentes: (a) **interna** — verificação claim-a-claim do diagnóstico contra o código real
(`cortex.rs`, `nn.rs`, `bitnet_avx2/avx512.rs`, `compute.rs`, `gguf.rs`, `convert_bitnet.py`,
`tools/bitnet_fwd_parity.py`); (b) **externa** — conferência das evidências contra fontes primárias
(config.json + modeling_bitnet.py do 2B4T, `microsoft/BitNet` src/README.md e ggml-bitnet-mad.cpp,
arXiv 2504.12285, nanoGPT_1GPU_SPEEDRUN, Hestia).

### 11.1 Veredito externo — verificado (14/17 grupos exatos, 0 refutados)

- ✅ Arquitetura 2B4T (M1–M4 §2): hidden 2560 / 30 camadas / 20 heads / 5 KV / FFN 6912 GLU **ReLU²** /
  theta 500000 / vocab 128256 LLaMA-3 / tied / BF16 embed / sem scales salvos — tudo confere.
- ✅ bitnet.cpp: 1.29× weight-parallel (0.058 vs 0.075 EPYC 7V13), activation-parallel 1.85–2.0×,
  I2_S embed = N/A, Q6_K 17.149 vs 17.109 PPL, 29ms/0.028J (este rotulado "Estimated" no paper).
- ✅ Receita de treino: tanh 30×, cooldown, betas (0.8,0.95), embed LR ≈12×, Muon+NS-5; Hestia
  `compress_ratio=0.2`/`anneal_ratio=0.8` — números exatos.
- ⚠️ **Correção 1:** "4–8× maiores" (§4 Tabela 3) → os baselines PTQ são **3.5–4×** (Falcon3-**7B**,
  Llama3-**8B** vs 2B). Rephrase.
- ⚠️ **Correção 2:** citação `modeling_bitnet.py` (§2) — o arquivo não existe mais no repo HF; o código
  canônico vive em `huggingface/transformers/models/bitnet/`.
- ⚠️ "Acumulação i32" (Fase 4) é simplificação: o loop interno é i16 com widening i32 a cada 32 blocos.

### 11.2 Veredito interno — kernel e diagnósticos confirmados; M2 refutado

- ✅ M1 (silu em `cortex.rs:901` e nos 4 forwards; `nn.rs:51-53`), M3 (conversor nunca escreve θ/bit2;
  loader default 10000), F1 (`unpack_row_into` match por peso 139-142; reload/store por t 186-188),
  F2 (`avx2_bitwise_matmul` L75-127 desativado, zero callers; bug OOB `n%4!=0` confirmado com SESSION_162;
  `dispatch_ternary` compute.rs:59 nunca lê m; 2 layouts de packing existem), F4 (`bitnet_avx512.rs:34-40`
  `(pair&1)-(pair>>1)`), justificativas do gate F4 (`soft_stride=3`, `MAX_SEQ=64`, 4-8 tokens) — **confirmados**.
- ❌ **M2 diagnóstico incorreto**: o forward **já aplica 4 RMSNorms/camada + final** (`rms_attn` 754,
  `rms_inner_attn` 888, `rms_ffn` 894, `rms_ffn_norm` 916, `rms_final` 928) e o conversor exporta as 4
  (`convert_bitnet.py:177-180`). O trabalho real de M2 é:
  1. **eps**: forward 1e-6 (`cortex.rs:708`) vs 2B4T **1e-5**;
  2. **Truncamento `rms_ffn_norm`**: loader lê só `hidden` (2560) f32 e faz pad para 6912
     (`cortex.rs:1853-1863`), mas o conversor escreve 6912 → **últimos 4352 pesos descartados
     silenciosamente**;
  3. **Caminho GGUF usa normas identidade** (`gguf.rs:712-715`) — M1-M3 no path `.bitnet` não alcançam
     Falcon3/LLaMA-8B.
- ⚠️ M5 "scale vestigial" só vale para o path HW Expert v5 (`read_prefixed_ternary`); o loader principal
  lê e **aplica** scale f32 (`read_ternary_tensor_with_scale`, `mul_scalar` 758-762/890/896-898/918).
  Conclusão (RMSNorm absorve) segue válida para o 2B4T (conversor não grava scales).

### 11.3 Findings bloqueadores — Fase 3 NÃO está pronta como escrita

1. 🔴 **Mismatch de layout conversor↔loader (baseline quebrado)**: `convert_bitnet.py` escreve v4 **sem
   scale f32 por tensor** (`:166, 198-204`), mas `load_model` v4 **consome scale incondicionalmente**
   (`read_ternary_tensor_with_scale`) e lê `rms_ffn_norm` com 2560 vs 6912 gravados. O arquivo 2B4T atual
   pode **nem parsear limpo**. **Fase 3 deve começar por auditoria/reconciliação de layout**
   (existe `tools/_probe_bitnet2b.py` para diagnóstico) **antes de qualquer trabalho Q6_K**.
2. 🔴 **Gate de paridade fraco demais para M1-M3**: `tools/bitnet_fwd_parity.py` (existe, 231 LOC — não é
   pré-requisito faltante) (a) só aceita magic `B1TM`/`B1` antigo (`bitnet_header` L136-141) — falha em
   v4/v5 `0xBE11BE11`; (b) usa modelo **850M**, não o 2B4T; (c) gate é overlap **top-5** sem threshold de
   logits — **não distingue silu de relu²**. Fortalecer (métrica de logits + modelo 2B4T + magic novo)
   senão a verificação M1-M3 é decorativa.
3. 🟡 **Armadilha Q6_K**: `dequantize_q6_k` bulk materializa **1.31GB f32** (vocab 128256) → estoura o cap
   de 2GB com o arquivo ~800MB residente. O cálculo +190MB (82→269MB, verificado correto) **só vale com
   dequant row-wise dentro do `embed_lookup`** (`cortex.rs:696-704` lê 1 linha/token). Especificar
   explicitamente; "dequant já existe em gguf.rs" pode induzir ao caminho bulk.
4. 🟡 **Contradição de ordem**: §2 P1 ("fidelidade antes de velocidade") vs §3 (F1/F2 velocidade antes de
   F3) vs checklist §9 (M1-M4 primeiro). Tecnicamente **sem impacto**: F1/F2 são bit-exactos
   (função preservada — a ativação vive fora do matmul, `cortex.rs:901`), então velocidade-primeiro não
   viola P1. Falta 1 frase na ADR reconciliando.
5. 🟡 **`soft_stride=3`** (hidden≥2048, `cortex.rs:740-750`) pula ~⅔ das camadas — a **maior divergência de
   fidelidade** de todas; está só como rationale de gate do F4, deveria ser item de fidelidade explícito
   (ou declarado intencional/budget).
6. 🟡 **`model.rope_theta` hardcoded** 10000 (`cortex.rs:1950`) enquanto cos/sin honram θ do EOF
   (`:1931-1939`) — armadilha para quem ler o campo.

### 11.4 Impacto e status

- **F1 (decode branchless) e F5 (tiling): independentes dos findings — executáveis já.**
- **F2:** factível, mas o gate por m e a reativação do bitwise exigem a memória de layout
  (2 layouts de packing) — sem bloqueio, risco médio mantido.
- **F3:** **bloqueada** até (1) auditoria de layout conversor↔loader, (2) parity gate fortalecido,
  (3) M2 reescopado (eps + truncamento + GGUF identity), (4) dequant Q6_K row-wise especificado.
- Lifecycle permanece `por_fazer` (Proposed) até Fases 1-3 concluídas e verificadas (§9).
