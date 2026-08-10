# SESSION_255 — HW Expert v6 (ADR-0085 §3.2) + imagem HW real com BITNET2B v6 (2026-08-09)

**Escopo:** Criar o modelo HW Expert no formato canônico `.bitnet v6` (model_type=1)
e gerar a imagem USB para HW real (`target/usb_hw.img`) com **hwexpert v6** e
**BITNET2B v6**.
**Status:** ✅ Fechada — 1 commit — 0 erros — 24 testes cortex PASS + boot build OK.

---

## 1. Contexto

Pedido do dono: "gere a imagem para hw real, com o hwexpert v6 e o bitnet2b v6".
Auditoria inicial: o **BITNET2B v6** já existia (`target1/BITNET2B.v6`, 755MB,
ver=6 mt=0 LLM, 2B4T), mas o **hwexpert v6 não existia em lugar nenhum** — só o
v5 legado (`models/hw_expert/hw_expert_v4.bitnet`, 260KB, ver=5 mt=4), e o kernel
marcava o loader v6 hwexpert como "pendente (F1b)" (`model.rs:63-68`).

Decisão do dono (HITL): **converter v5→v6 + implementar loader F1b**.

---

## 2. Conversor v5→v6 (`tools/convert_hwexpert_v5_to_v6.py`)

Formato v5 (export do retrain_hw_expert_v4.py): header legado + body **com
prefixos** `u32 len + u32 scale` por tensor + rope 16 f32/layer.
Formato v6 (ADR-0085 §2/§3.2): num_params **u64**, `model_type=1`, reserved=0,
act_type/embed_type/feat no fim do header; body **sem prefixos** (rms = f32 puro,
tensor = packed + f32 scale).

**Decisão de fidelidade crítica — q_dim:** o modelo v5 foi treinado com
**q_dim=32** (atenção truncada) e o forward `predict_hw_v4` usa `model.q_dim`
para truncar. Teste `tools/check_hwexpert_qdim.py` provou que **q_dim=128 no
header muda as predições** (7/10 devices DIFF). O conversor preserva `q_dim=32`
e o loader v6 lê shapes fixos hwexpert `q/k/v/o=(h,h), g/u=(h,ff), d=(ff,h)`
— exatamente como o `load_hwexpert_v5`. NÃO colapsar q_dim para hidden.

Conversão: `python tools/convert_hwexpert_v5_to_v6.py models/hw_expert/hw_expert_v4.bitnet tools/target/hw_expert_v6.bitnet`
→ 265620 bytes (v5 266321 − rope/prefixos). Parity byte-exact em TODOS os
tensores + predições idênticas nos 10 devices canônicos
(`tools/check_hwexpert_v6_parity.py` PASS).

---

## 3. Loader v6 hwexpert no kernel (F1b)

- `crates/cortex/src/cortex.rs`: novo `load_hwexpert_v6()` — header v6 estrito
  (magic/version=6/u64 num_params/mt=1/reserved), feat bits 0-2, body sem
  prefixos, shapes fixos hwexpert, 5 heads (17,8,9,10,9), preserva q_dim.
- `crates/cortex/src/model.rs`: dispatch mt=1 → `load_hwexpert_v6` (substitui o
  fallback "pendente F1b" para o v5).
- `crates/neural-kernel/src/main.rs`: pontos de carga (QEMU-loader scan + FAT32
  fallback) tentam `load_hwexpert_v6` primeiro, fallback `load_hwexpert_v5`
  (backward compat com HWEXPRT4 v5 antigo).
- **Teste host** `hwexpert_v6_matches_v5_predictions`: carrega os DOIS arquivos
  reais (v5 e v6 convertido via include_bytes) e compara as 5 saídas × 10
  devices canônicos — **PASS**. 24 testes cortex, 0 falhas.
- `cargo check --release` (workspace): **0 erros**.

---

## 4. Imagem HW real

- `tools/mkfat32.py`: `HWEXPRT4.BIN` prefere `hw_expert_v6.bitnet` (depois v4).
  (Mudanças paralelas do dono no mesmo arquivo — `find_file` com `target1/`
  canônico + nomes `.v6` — já estavam no working tree e foram usadas: BITNET2B
  veio de `target1/BITNET2B.v6`.)
- `cargo build --release -p boot` → `target/uefi.img` (loader v6 embutido).
- `PACK_LLM=2b; python tools/build_image.py --hw --unified --size 6144` →
  `target/usb_hw.img` (6271 MB, ESP FAT + dados FAT32).
- Verificação na imagem (`tools/check_usb_image_models.py` + leitura do header
  no FAT32):
  - `HWEXPRT4.BIN` = 265620B, **ver=6 mt=1** h=128 L=6 q_dim=32 feat=0x03 ✅
  - `BITNET2B.BIN` = 755MB, **ver=6 mt=0** h=2560 L=30 vocab=128256 ✅

---

## 5. Lições

1. **`model.q_dim` do hwexpert é contrato de predição, não shape:** o forward
   trunca a atenção em q_dim. Converter "colapsando para hidden" (ADR §3.2
   literal) muda predições — preservar o q_dim treinado no header e ler shapes
   fixos hwexpert.
2. **v5→v6 hwexpert é conversão mecânica fiel:** body v5 com prefixos → v6 sem
   prefixos; bytes packed são idênticos; rope (16 f32/layer, descartado no
   forward) some no v6.
3. **Teste de loader com ARQUIVOS REAIS, não sintéticos:** o teste host usa
   include_bytes do v5 e do v6 convertido e compara predições — prova de que o
   loader lê o mesmo modelo, não apenas que parseia.
4. **`target1/` é o canônico de modelos v6** (SESSION_254+): find_file/mkfat32
   priorizam `target1/` e nomes `.v6`; `models/` é fallback.

---

## 6. Pendentes

- Boot QEMU com o hwexpert v6 (validação runtime completa — o boot HW real fica
  para quando o pendrive for gravado via Rufus DD).
- Retreino do HW Expert com a receita 1-bit (IDEA #489) — formato v6 já
  suportado, falta o treino host PyTorch.
