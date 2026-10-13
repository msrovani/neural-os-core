# ADR-0115: Falcon v7 ideal — formato, conteúdo e gate (síntese 0114+0116)

- **Status:** Proposed (especificação; nada implementado; cada item novo
  exige gate medido antes de entrar em arquivo ou kernel)
- **Lifecycle:** `por_fazer`
- **Origem:** análise das ADRs 0114 (padrões) e 0116 (vereditos E01–E38)
  contra os `.v6` testados no host (suite/paridade/bench, out/2026).
- **Fonte:** este documento; implementação futura em `tools/bitnet_writer.py`,
  `tools/convert_falcon3_to_v6.py`, `tools/chat_v6.py`, `crates/cortex`.

## 1. Análise das ADRs frente ao v6 testado

### 1.1 ADR-0114 (padrões TensorFold) vs v6
- affine4/lane-tiling → pattern p/ kernels CPU (medir fit, SESSION_412);
  affine4 como FORMATO de pesos, rejeitado (v6 canônico). Sem impacto v7.
- EXL3 in-engine → doutrina de loader já nossa (IDs oficiais, header
  autodescritivo). Sem impacto v7.
- Decode exato (resumed==fresh) → vira CONTRATO de validação v7 (§3),
  não formato.
- Living Weights → vira SEÇÃO DE PROVENANCE no v7 (§2.3): versionar
  evolução exige saber que fonte/ferramenta gerou cada arquivo — dor real
  (arqueologia dos FALCON3_BASE/7B). Único item 0114 com impacto em formato.
- DMA-race/hygiene/zero-dep → runtime existente. Sem impacto v7.

### 1.2 ADR-0116 (vereditos) vs v6
Adotado e provado (entra no v7 como base): unpack estridado + transpose
sempre + scales recíprocas + gate de exatidão (conversor); unembed Q6K
(feat `0x08`); normas/embed/unembed fiéis; template TII identificado;
sampling com seed; gramática como direção; suite/paridade/bench como gate.
Em aberto com protocolo (candidatos, NÃO comprometidos): escalas por
linha (E30: MSE + suite + custo), packing 5-in-8 (benchmark decode),
seção multimodal (projetor treinado + VQA), BF16-embed, Medusa com heads
treinadas, grammar-draft prototype (accept rate).
Rejeitado (fora do v7): tudo GPU/CUDA, PTQ-de-denso, binário, fuzzy-forward,
atenção ternária, MoTE-artefatos, speculative sem accept, 27B fantasmas,
EXL3/affine4 como pesos, dual-scale (condicional não disparada).
Números duros: 1B suite 1/3 (Q1✓); bench v6 3–4x vs transformers-CPU;
paridade cos 1,0; Q6K==denso caractere a caractere; entropias 5,7–9,5.

## 2. Especificação v7 ideal

Regra de versão: campo `version` 6→7; leitores ACEITAM 6 e 7 (arquivos
atuais seguem válidos); escritor emite 7. Golden tests nos arquivos v6
permanecem byte-idênticos. Nenhum bit novo entra em arquivo sem passar
pelo gate do §3.

### 2.1 Mantido do v6-feat8 (provado, sem tocar)
Pesos ternários packed 2-bit (convenção kernel) + scales f32; Q6K embed e
unembed; RMS f32; theta f32 + RoPE full-dim; tie; medusa slots (zeros até
heads treinadas); act SILU; header autodescritivo; forward ternário +
RoPE + GQA + SwiGLU + greedy/seeded.

### 2.2 NOVO no v7 (só o medido/provado)
1. **Provenance** (de 0114): seção pós-header — repo fonte (str),
   revisão (40B ASCII), conversor+versão (str), git hash (40B), convenção
   de scales u8 (0=absmean, 1=recíproca-nativa), convenção de unpack u8
   (0=kernel-consecutivo, 1=estridado-HF), tokenizer id u8. ~120 B.
   Motivo: nunca mais arqueologia (caso FALCON3_BASE/7B).
2. **Tokenizer embutido**: `tok_type=1` + payload BPB1 existente
   (~3 MB p/ 131k). Motivo: fim da caça ao BPE.BIN (confusão 32k/131k
   medida); artefato autocontido p/ USB. Fallback: BPE externo quando
   ausente.
3. **Unembed Q6K** (de 0116-feat8): carregado para o formato (era feat;
   vira seção padrão com flag; leitores v6-feat8 continuam válidos).

### 2.3 CONDICIONAL (porta com gate, não promessa)
- Escalas por linha no unembed (feat `0x10`, hidden f32): SÓ após o
  protocolo E30 (MSE + delta na suite com Q2 de canário + custo).
  Demais tensores: nem protocolo aberto (sem denso-fonte).
- Seção multimodal (projetora + convenção visual): SÓ após projetor
  treinado + probe VQA + custo CPU.
- Restante da lista 1.2: fora até prova.

### 2.4 Conteúdo por tamanho (só nativo-ternário, regra E08)
| Modelo | Fonte | L/h/q/kv | inter | vocab | Arquivo v7 (alvo) |
|---|---|---|---|---|---|
| 1B | tiiuae/Falcon3-1B-Instruct-1.58bit | 18/2048/8/4 | 8192 | 131072 | FALCON3_1B.V7 (~700 MB) |
| 3B | tiiuae/Falcon3-3B-Instruct-1.58bit | 22/3072/12/4 | 9216 | 131072 | ~1,7 GB |
| 7B | tiiuae/Falcon3-7B-Instruct-1.58bit | 28/3072/12/4 | 23040 | 131080 | ~2,9 GB |
| 10B | tiiuae/Falcon3-10B-Instruct-1.58bit | 40/3072/12/4 | 23040 | 131072 | ~3,6 GB |
Uniformidades úteis: head_dim 256 e k_dim 1024 em todos; q_dim==hidden;
theta 1000042; tie False. Candidato leve extra: BitNet-b1.58-2B-4T (E16).

## 3. Gate e rollout
Gate por arquivo (o rig 0116): triagem 0-buracos + paridade unpack MATCH +
entropia + suite + bench + bytes exatos + provenance preenchida. Contrato
de decode determinístico (de 0114): seed reproduzível + resumed==fresh
(harness já; kernel adota disciplina de seed).
Rollout: (0) congela esta spec PROPOSED; (1) reader/writer no host
(bitnet_writer + chat_v6); (2) 1B ponta a ponta no rig; (3) reader no
kernel (`cargo check`, host tests, boot QEMU com a imagem nova);
(4) 3B/7B/10B mesma esteira; (5) gates condicionais; (6) lifecycle →
`completa` + SESSION + n-sgdb. NÃO-GOALS: §1.2-rejeitados (lista acima).

## 4. Sync n-sgdb
`scope=project/neural-os-core`, entities `adr-0115` + `mom/decision`.
