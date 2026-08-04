# ADR-0085: `.bitnet v6` — Formato Canônico + Registro de Modelos K³CHJ

**Data:** 2026-08-04
**Status:** Proposed
**Lifecycle:** `por_fazer`
**Fonte:** Requisito do dono (padronizar codificação/escrita/leitura/inferência/propagação K³CHJ) +
auditoria tripla ADR × Python × Rust (ver ADR-0084 §11 e comparação 3-vias) + design do oracle
**Ideias:** relaciona #479–490 (ADR-0084); nova #491 a registrar no IDEA_BANK
**Substitui:** — (formaliza a auditoria de layout exigida pela ADR-0084 §11.3.1; complementa ADR-0019, ADR-0012, ADR-0011, ADR-0084)

---

## 1. Contexto

Requisito do dono: **o sistema todo DEVE padronizar** como (1) codifica modelos, (2) escreve o arquivo
`.bitnet` inteiro, (3) lê os modelos que cria, (4) extrai inferência/inteligência deles, (5) processa os
dados por todo o K³CHJ (k_nano → k_hal → cortex/k_ai → hermes → jarbas).

Fato base da auditoria: o **único par byte-exato** hoje é Rust `save_model`↔`load_model`
(cortex.rs:2216-2301 ↔ 1664-1939, round-trip cortex.rs:2311). **Nenhum conversor Python produz arquivo
alinhado**:

| Bug | Evidência |
|---|---|
| convert_bitnet v4 **sem scales**; loader lê f32 incondicional | cortex.rs:1774, 1867-1873 |
| convert_gguf v5 escreve **2 normas**, feat=0x07; loader lê 4 | cortex.rs:1847-1866 |
| convert_falcon3 escreve **4 normas**, feat=0x04; loader lê 0 | convert_falcon3:256-268 |
| `rms_ffn_norm` escrito `intermediate`, lido `hidden` (pad) | cortex.rs:1853-1863 |
| tied escreve **zero-unembed** que mata o theta do EOF | gguf:1060-1062, falcon3:341-345, cortex.rs:1905-1917 |
| Containers length-prefixed que nada lê | convert_safetensors:82-88, train_models_gpu:99-101 |
| num_params u32 vs u64 entre dialetos v5 | cortex.rs:1670-1674 vs 250 |
| Nenhum campo de **ativação** no formato (M1) | todos |
| Embed **sempre ternário** (M4); sem tag de tipo | todos os conversores |

O 2B4T convertido pelo convert_bitnet "carrega" só por acaso (v4 força has_basic_rms; falhas de bounds
caem em valores sintetizados L1894-1917; região do QEMU-loader é zero-padded) — **o forward 2B como
distribuído computa lixo** (consistente com SESSION_162 e `tools/_probe_bitnet2b.py`).

Esta ADR elimina as ambiguidades **por construção**: um formato autodescritivo (v6), um writer canônico,
um loader estrito com fallback legado, e um registro único de modelos para todo o K³CHJ.

---

## 2. Header v6 (offsets fixos)

Magic **permanece `0xBE11BE11`** (scanner QEMU-loader main.rs:2093-2140 e carregadores legados dependem).
`version u16 = 6`.

### Preamble comum (17 bytes) + bloco por `model_type`

| Offset | Tamanho | Campo | Semântica |
|---|---|---|---|
| 0 | 4 | magic | `0xBE11BE11` |
| 4 | 2 | version | `6` (u16 LE) |
| 6 | 8 | num_params | **u64 — único significado em todo o sistema**: soma de todos os elementos de todos os tensores (embed, norms, pesos, heads, router). Informativo (pre-grow do heap, telemetria) — o loader **nunca** infere layout a partir dele. |
| 14 | 1 | model_type | `0=llm`, `1=hwexpert`, `2=router`, `3..255=reservado` |
| 15 | 3 | reserved | zeros (marcador de writer canônico; loader rejeita ≠0 — pega bytes corrompidos cedo) |

### Bloco transformer (model_type 0 e 1) — a partir do offset 18

| Offset | Tamanho | Campo | Semântica |
|---|---|---|---|
| 18 | 2 | hidden u16 | |
| 20 | 2 | num_layers u16 | |
| 22 | 2 | num_heads u16 | |
| 24 | 4 | vocab_size u32 | |
| 28 | 2 | max_seq u16 | |
| 30 | 2 | intermediate_size u16 | FFN hidden (canônico = 6912 p/ 2B4T) |
| 32 | 2 | num_kv_heads u16 | GQA |
| 34 | 2 | q_dim u16 | |
| 36 | 4 | num_medusa u32 | |
| 40 | 4 | tie_flag | `b"TIED"` ou 4×0x00 |
| 44 | 1 | tok_type | 0=none, 1=BPE |
| 45 | 4 | tok_len u32 | |
| 49 | tok_len | tokenizer_data | |
| 49+T | 1 | **act_type** | `0=silu`, `1=relu2`, `2..255=reservado`. Novo campo M1 — o forward escolhe a ativação por arquivo (2B4T=1; modelos pequenos treinados silu=0), sem mudança global. |
| 50+T | 1 | **embed_type** | `0=ternary-packed`, `1=Q6_K`, `2=BF16`, `3..255=reservado`. Novo campo M4. |
| 51+T | 1 | **feat** | Semântica re-definida, **sempre computada do que foi escrito**: `bit0=rms_inner_attn presente`, `bit1=rms_ffn_norm presente`, `bit2=theta f32 no EOF`. Bits 3-7 = 0 (loader rejeita ≠0 — é inventário real do arquivo, não flag de arquitetura). |
| 52+T | — | body | abaixo |

Decisão de posicionamento: `act_type`/`embed_type`/`feat` depois do tokenizer; feat como último byte do
header (mesma posição relativa do v4, L1745) — o bloco é autodescritivo, o leitor v6 nunca adivinha.

---

## 3. Body por `model_type`

### 3.1 LLM (model_type=0) — sequência, tamanhos derivados

```
embed:  [embed_type==0] packed (hidden*vocab+3)/4
        [embed_type==1] blocos Q6_K: ceil(hidden*vocab/256) × 210B  (layout GGUF: ql[128]+qh[64]+scales[16]+d f32 — reusa gguf.rs:511-568)
        [embed_type==2] bf16: hidden*vocab*2 bytes (row-major (hidden,vocab))
        + f32 scale  (SEMPRE, mesmo embed_type≠0)
por layer × num_layers:
  rms_attn        f32[hidden]
  rms_ffn         f32[hidden]
  se feat&1:  rms_inner_attn  f32[hidden]
  se feat&2:  rms_ffn_norm    f32[intermediate_size]      ← CANÔNICO (intermediate, não hidden)
  7 × (packed(rows*cols+3)/4 + f32 scale), nesta ordem e shapes:
    q (hidden, q_dim), k (hidden, k_dim), v (hidden, k_dim), o (q_dim, hidden),
    gate (hidden, ffn_group), up (hidden, ffn_group), down (intermediate_size, down_out)
    com k_dim = kv_heads*(q_dim/heads); ffn_group = intermediate_size*q_dim/hidden; down_out = q_dim
rms_final f32[hidden]
se !tied: unembed (packed + f32 scale)        ← tied ⇒ NADA escrito
para _ in 0..num_medusa: (packed + f32 scale)
se feat&4: theta f32
```

### 3.2 HWExpert (model_type=1)

Mesmo bloco transformer **com `q_dim==hidden`, `num_kv_heads==num_heads`, `intermediate_size=ff`** — as
fórmulas colapsam para q/k/v/o=(h,h), g/u=(h,ff), d=(ff,h) (mesmo que `load_hwexpert_v5` já lê,
cortex.rs:316-322, só que sem prefixos). Tail: `rms_final f32[hidden]` + 5 heads `(packed + f32 scale)`
shapes (h,17),(h,8),(h,9),(h,10),(h,9) — substitui unembed/medusa. `act_type` presente porém não usado
(hwexpert não tem ativação FFN no forward).

### 3.3 Router (model_type=2) — bloco próprio a partir do offset 17

| Offset | Tamanho | Campo |
|---|---|---|
| 17 | 4 | vocab u32 |
| 21 | 2 | hidden u16 |
| 23 | 2 | n_experts u16 |
| 25 | — | embed f32[vocab×hidden] row-major |
| 25+ | — | weight i8[hidden×n_experts] raw (não-ternário — sem scale) |

Posicional, sem names/tags. Router legado (version u32=3, train_router.py:574) continua com
`load_router_from_file` (trinity.rs:541); v6 é o novo caminho. **Colisão impossível**: versões legadas
são u16 3..5 (u32=3 no router, mas offset 4 lido como u16=3 < 6), v6 = 6.

---

## 4. Decisões (com racional)

| # | Decisão | Racional |
|---|---|---|
| D1 | **Scales: SEMPRE presentes, SEM flag `has_scales`.** Todo tensor quantizado é seguido de f32 scale; produtor escreve 1.0 quando não há escala significativa; loader lê+aplica incondicionalmente. | (a) a flag permitiria produtores quebrados atuais "funcionarem" sem escala — o objetivo é quebrar esse caminho; (b) scale NÃO é vestigial: o/down caem na residual (cortex.rs:891, 919-923) e unembed nos logits — nenhuma RMSNorm renormaliza; (c) uma regra = zero ramos no loader. Custo: 4B×8/layer ≈ 960B/layer no 2B4T — desprezível. |
| D2 | **`rms_ffn_norm` canônico = `intermediate_size`.** Loader lê exatamente intermediate (corrige L1855); writer Rust escreve intermediate (corrige L2257, hoje clampado a hidden); remover o pad (L1856-1863) no v6. | ADR-0019 L213 já especifica `[f32; intermediate_size]`; 2B4T grava 6912. O pad escondia o desalinhamento — v6 lê exato ou falha. |
| D3 | **Tied ⇒ seção unembed NÃO existe** (zero bytes). Loader: `tie_flag==b"TIED"` ⇒ pula unembed e lê theta em seguida. | Corrige a perda de theta em gguf (L1060-1062) e falcon3 (L341-345), que escreviam zero-unembed nunca consumido. |
| D4 | **`num_params` u64 em todo v6** (LLM, hwexpert, router). hwexpert migra para v6 (morre o u32 do dialeto v5, cortex.rs:250). | Um significado, um campo, zero branch por tipo. |
| D5 | **`feat` computado do escrito** (helper `compute_feat(has_inner, has_ffn, has_theta)` no writer) — nunca hardcoded. Loader v6: bit diz presente e leitura falha → `None` + log (estrito). | Mata a classe de bug convert_gguf (feat=0x07 com 2 normas) e falcon3 (feat=0x04 com 4 normas). |
| D6 | **`embed_type` no header, dequant row-wise**: Q6_K armazenado (hidden,vocab) row-major; `embed_lookup` (cortex.rs:696-704) decodifica por lookup — cada token lê 2560 blocos de 210B (~537KB scan/token no 2B, ≈ms em TCG; sem cache — `// ponytail: decode-per-lookup, cache por token se perf exigir`). | Bulk dequant f32 = 1.31GB (vocab 128256) estoura o cap 2GB (cortex.rs:1704). Alternativa (layout (vocab,hidden) + flag transposta) rejeitada: transfere o problema para o matmul do unembed tied. |
| D7 | **Unificação: LLM + hwexpert + router migram para v6** (mesmo magic/version/num_params, `model_type` desambigua o body). **STT, Piper, BGE, ViT, wakeword, register-predictor FICAM** como exceções documentadas (magic preservado p/ scanner; containers e loaders próprios embutidos nos consumidores jarbas/k_ai — round-trip OK hoje, não compartilham o pipeline transformer). | hwexpert é literalmente um TransformerModel com 5 heads (cortex.rs:244-351); router já tem tag `model_type` (train_router.py:575). Forçar STT/Piper (CTC LSTM, VITS) no container transformer agrega zero valor e quebra round-trips que funcionam. |
| D8 | **Escrita canônica única**: `tools/bitnet_writer.py` (numpy puro, sem torch) + Rust `save_model` como referência; **paridade por arquivo golden**. | Hoje cada conversor hand-rola bytes (6 layouts divergentes). Um writer = uma verdade; teste de paridade = gate automático. |
| D9 | **v6 estrito; v3/v4 legados bug-compatíveis com WARN.** v6: leitura fora de bounds ou `feat` inconsistente → `None` + log. v3/v4: parse atual preservado (heuristics L1792-1819, fallbacks L1894-1917) + WARN no boot "layout legado suspeito; re-converter p/ v6" + detecção barata de escala ausente (`off` final vs `data.len()` fora de ±2%). | Fail-loud total em v3/v4 quebraria o ladder 850M (que só "funciona" via fallbacks — SESSION_162) no mesmo commit que introduz v6; o tool de migração resolve. Fail-loud pleno chega quando o arquivo for re-convertido. |
| D10 | **Migração = re-conversão, não cirurgia de bytes.** `tools/migrate_bitnet_v6.py` re-roda o conversor certo (safetensors/GGUF) com flags v6. Arquivos sem fonte → marcados inutilizáveis (fail loud). | Scales ausentes e comprimento ambíguo de `rms_ffn_norm` são irrecuperáveis do byte — a única "migração" correta é regenerar. 2B4T: re-conversão via convert_bitnet.py v6. |

---

## 5. Writer canônico + paridade

### `tools/bitnet_writer.py` (novo, ~150 LOC, numpy-only)

API pública (funções puras, sem I/O de modelo):

```python
pack_ternary(flat_int8) -> bytes                       # mover pack_kn_fast (convert_bitnet:65-78) p/ cá
write_header_v6(f, *, model_type, num_params, hidden, layers, heads, vocab, max_seq,
                intermediate, kv_heads, q_dim, medusa, tie, tok, act_type, embed_type, feat)
compute_feat(has_inner, has_ffn, has_theta) -> int     # D5
write_embed(f, vals_f32_or_int8, embed_type, scale)    # ternary | Q6_K | BF16, sempre + scale f32
write_rms(f, vec_f32)                                   # f32 exato (sem clamp)
write_ternary(f, q_int8, scale)                         # packed + f32 scale
write_q6k(f, vals_f32, rows, cols)                      # blocos 210B (espelha gguf.rs:512-568)
--self-check                                            # grava tools/golden_v6.bin a partir de spec sintética determinística
```

### Paridade Python↔Rust (gate de F0)

`cortex` ganha `#[cfg(test)] fn v6_writer_parity()`: constrói o MESMO modelo sintético determinístico do
self-check Python (LCG seed fixa, espelhando cortex.rs:2311+), gera bytes com `save_model_v6`, e compara
**byte a byte** com `include_bytes!("../../tools/golden_v6.bin")`. Igualdade = packing, header, ordem,
scales, feat — tudo pinado. Roda no host via `cargo test -p cortex` (gate host já funciona: SESSION_247).

### Diffs por script Python (F1)

| Script | Mudanças exatas |
|---|---|
| convert_bitnet.py | usa writer; feat=compute_feat(True,True,True) (4 normas, theta, scales 1.0 — absmean per-tensor derivado como hoje L81-86); rms_ffn_norm **intermediate**; act_type=1; embed_type=1 (Q6_K, D6) ou 0 (decisão do dono §10); version=6; num_params u64 |
| convert_gguf_to_bitnet.py | usa writer; **para de escrever rms_inner/ffn_norm** (feat=compute_feat(False,False,theta) → só bit2); act_type do metadata `arch.activation` (silu/relu2); tied ⇒ sem zero-unembed (D3); theta SEMPRE no EOF (bit2); scales RTN como hoje (L740-742) |
| convert_falcon3_bitnet.py | usa writer; **para de escrever as 2 normas sintéticas** (L266-268) — feat=0b100; act_type=0; theta no EOF (hoje ausente — bug real); scales mantidos |
| prepare_extra_models.py | usa writer; escreve scales (1.0); rms_ffn_norm intermediate; feat computado; act_type=0; embed ternary ou Q6_K |
| convert_safetensors_to_bitnet.py | **reescreve para body LLM real** (hoje é container prefixado ilegível, L82-88) — vira wrapper do writer com mapeamento por camada |
| train_models_gpu.py (BitNetLM.export_bitnet) | usa writer (mata prefixos L99-101); feat computado; act_type=0. ⚠️ **adicionar silu ao forward de treino** (L152, `g*u` → silu(g)·u) + retreino TinyStories/RustCoder p/ o export casar com act_type=0 (decisão do dono §10.3) |
| train_hw_expert_v4.py / retrain_hw_expert_v4.py | export v6 model_type=1, raw packed + f32 scale (mata prefixos u32 len+u32 scale L254-256), num_params u64 |
| train_router.py | export v6 model_type=2 (embed f32 + weight i8 posicional) |
| validate_hw_expert_v4.py / _class.py, sim_load_model_hwexpert.py | lêem body hwexpert v6 (mesmos shapes, sem prefixos) |
| bitnet_fwd_parity.py | header parser v6 (magic+version u16, L132-156 hoje espera `B1TM`), default → 2B4T, métrica logit-level (≤0.5% rel top-16) além do overlap top-5 |

---

## 6. Loader + legado

`load_model_v6(data) -> Option<ModelView>` em novo `cortex::model` (ou cortex.rs):

1. magic ≠ BE11BE11 → None. `version` u16: `==6` → v6 estrito; `3..=5` → caminho legado (`load_model`
   atual + WARN; hwexpert v5 → `load_hwexpert_v5`; router v3 → loader trinity); `>6` → None + log.
2. v6: parse preamble → dispatch por `model_type` → bloco específico. Estrito: `reserved==0` (offset
   15-17); leituras sempre bounds-checked (None em falha); `feat` decide presença/consumo exato das
   normas; `tie_flag` decide existência do unembed (D3); theta lido só com bit2; `act_type`/`embed_type`
   validados (≥2/≥3 → None).
3. Remover no v6: pad de `rms_ffn_norm` (L1856-1863), fallback `rms_final` (L1894-1898 — v6 exige),
   zero-detect de unembed (L1907-1910 — v6 nunca escreve zero-unembed), heurística rem/need (L1792-1819,
   só <v4).

`tools/migrate_bitnet_v6.py` (~80 LOC): detecta o conversor de origem (header v4/v5 + shapes) e re-roda
com flags v6; sem fonte → FAIL loud. 2B4T = re-conversão. Arquivos v4 gerados pelo writer Rust atual são
auto-consistentes e dispensam migração.

---

## 7. Registro + propagação K³CHJ

`cortex::model` (novo, ~120 LOC) — **registro mínimo, não framework**:

```rust
pub enum ModelView { Llm(TransformerModel), HwExpert(HwExpertV4Model), Router(RouterModel) }
pub fn load_model_v6(data: &[u8]) -> Option<ModelView>;
impl ModelView {
    pub fn kind(&self) -> ModelKind;                       // Llm | HwExpert | Router
    pub fn as_llm(&self) -> Option<&TransformerModel>;
    pub fn as_hwexpert(&self) -> Option<&HwExpertV4Model>;
    pub fn as_router(&self) -> Option<&RouterModel>;
}
```

`ModelHub` existente (model_hub.rs:82-96) ganha `register_bytes(slot, data) -> bool` =
`load_model_v6` + armazenamento por kind (Llm → `Box<dyn Model>` no slot; HwExpert/Router → statics
legados ou slots tipados). main.rs consolida os ~12 sites `load_model` + 2 `load_hwexpert_v5` + 4
`load_router_from_file` (main.rs:2760-3589) em um único ponto.

**Fluxo de chamada (requisito 5):**

```
FAT32 (MODEL.BIN, mkfat32/populate_fat32) | QEMU-loader (main.rs:2727-2836, magic scan inalterado)
  → bytes → cortex::model::load_model_v6  (fronteira de formato = cortex)
  → ModelView:
      Llm      → forward_with_kv / generate_text (cortex.rs:712, 2914); experts Trinity MoE = slots Llm
      HwExpert → predict → build_card (k_ai; set_hwexpert_v4_model já existe)
      Router   → trinity::route (substitui load_router_from_file; embed f32 + weight i8)
  → hermes: skill/router agents via generate_from_slot (model_hub.rs:112) / EventBus — sem tocar formato
  → jarbas: consome texto/embeddings via hermes — sem contato com .bitnet
k_nano/k_hal: transporte somente (scan de magic + DMA) — inalterados
```

---

## 8. Fases + gates

| Fase | Escopo | Arquivos | Esforço | Gate |
|---|---|---|---|---|
| F0 | Spec + writer + paridade | tools/bitnet_writer.py, cortex.rs (save_model_v6), test v6_writer_parity, golden_v6.bin | 1 sessão | `cargo test -p cortex` (parity byte-exact) + `cargo check --release` 0 erros |
| F1 | Migração de conversores | os 8 scripts + validate_hw_expert_v4*, mkfat32 (nomes inalterados) | 1-2 sessões | cada arquivo novo carrega em host (validate/python) + boot QEMU 2B re-convertido |
| F2 | Loader v6 + legado + migrate | cortex.rs (load_model_v6), migrate_bitnet_v6.py | 1 sessão | boot v6 (`-NoDisk` TCG + WHPX); v4 legado → WARN não-fatal |
| F3 | Fidelidade por tags (ADR-0084 M1-M4) | cortex.rs:901 (act_type nos 4 forwards), embed_lookup Q6_K (L696-704), theta (já existe L1931), rms_ffn_norm intermediate (L1855), bitnet_fwd_parity.py | 1-2 sessões | parity 2B4T PASS (logit-level) + cargo check + boot |
| F4 | Registro K³CHJ | cortex::model, model_hub.rs, main.rs (consolidação de call sites) | 1 sessão | boot 8 fases + smoke de agentes + `tools/check_duplication.py` |

Ordem consistente com ADR-0084 §11: **layout audit primeiro** — esta ADR É a auditoria; F0-F2 precedem
qualquer trabalho de kernel (ADR-0084 F1/F2).

---

## 9. Fora de escopo

STT/Piper/BGE/ViT/wakeword/register-predictor: containers próprios, magic preservado, loaders locais
(D7). k_nano/k_hal: transporte. `slot_from_bitnet_bytes` (model_hub.rs:267): heurística por tamanho
inalterada. Nomes FAT 8.3 inalterados.

---

## 10. Decisões do dono (resolvidas 2026-08-04)

1. **embed_type do 2B4T na F1** → **Q6_K já na re-conversão** (rec aceita). embed_type=1 na F1; F3
   fica só com o dequant row-wise no `embed_lookup` e a ativação do forward.
2. **Legado v3/v4** → **WARN-only** (rec aceita). Parse legado preservado + WARN de boot +
   `migrate_bitnet_v6.py` na F1; fail-loud pleno só após re-conversão.
3. **train_models_gpu (BitNetLM)** → **adicionar silu ao forward de treino** (L152, `g*u` → silu aplicado
   antes de `*up`) para o modelo exportado casar com act_type=0 — **exige retreino** do TinyStories/
   RustCoder na F1 (não é mais aceitar toy).
4. **act_type extensível** → **reservar `2=GELU-tanh`** (rec aceita; custo zero, um byte).

---

## 11. Riscos

1. Strictness v6 pode expor que o ladder 850M atual carregava lixo — mitigado por WARN + re-conversão
   na F1.
2. Dequant Q6_K row-wise ~537KB/token em TCG — correto primeiro, cache só se perf exigir (D6).
3. hwexpert/validate movem em lockstep na F1 (mesmo formato).
4. Heap 2B: Q6_K +190MB OK (82→270MB), mas embed_type=2 BF16 = 656MB no 2B — documentar que BF16 é
   para modelos pequenos.
5. `model_save_roundtrip_self_test` (cortex.rs:2311) e o golden compartilham o spec — manter ambos
   derivados do mesmo generator.

---

## 12. Checklist de Aceite

- [ ] F0: `tools/bitnet_writer.py` + `save_model_v6` + `v6_writer_parity` (byte-exact) + golden
- [ ] F1: 8 conversores + validators emitindo v6; 2B re-convertido carrega e gera
- [ ] F2: `load_model_v6` estrito + fallback legado com WARN + `migrate_bitnet_v6.py`
- [ ] F3: act_type/embed_type/theta/ffn_norm intermediate aplicados; parity 2B4T logit-level PASS
- [ ] F4: `cortex::model` + ModelHub `register_bytes` + call sites consolidados no main.rs
- [ ] `cargo check --release` 0 erros a cada fase
- [ ] Boot QEMU v6 (`-NoDisk` TCG + WHPX) 8 fases + geração viva
- [ ] INDEX.md, IDEA_BANK e TECNOLOGIAS.md atualizados

---

## 13. Referências

- ADR-0084 §11 (revisão; auditoria de layout exigida em §11.3.1)
- ADR-0019 (formato v3/v4; L213 `rms_ffn_norm [f32; intermediate_size]`)
- ADR-0012 (packing 2-bit), ADR-0011 (BitLinear)
- `cortex.rs:1664-1939` (loader), `cortex.rs:2216-2301` (writer), `cortex.rs:2311` (round-trip)
- `gguf.rs:511-568` (dequant Q6_K), `cortex.rs:696-704` (embed_lookup)
- SESSION_162 (bug OOB bitwise; coerência fraca), SESSION_247 (gate host)
- Comparação 3-vias ADR × Python × Rust (2026-08-04)
