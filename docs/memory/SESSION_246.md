# SESSION_246 — Auditoria Técnica 7.x: Gap da Camada de IA + Correções (ADR-0083)

## Problema
A auditoria técnica (seção 7) concluiu que a infraestrutura de inferência é real
(transformer ternário, GQA, RoPE, KV cache, GGUF, BPE), mas a **inteligência** que
ela deveria servir não existia — componentes "neurais" eram ruído com aparência de
modelo:

- **7.2** — Roteador MoE "treinável" = LCG seed=42 (embedding uniforme + pesos
  ternários aleatórios determinísticos); log anunciava "Router MoE loaded".
- **7.3** — Treinamento on-device = regressão linear escalar de 64 pesos, sem
  retropropagação; `train_task` com inputs=targets=1.0 (nada a aprender).
- **7.4** — Memória semântica = pseudo-hash FNV-1a por default (BGE ausente).
- **7.5** — Saudação demo = pool de tokens fixo + bias posicional/bigram até 8.0
  (efetivamente CANNED).

> "Hoje, se o senhor removesse toda a camada neural, o sistema se comportaria quase
> igual — porque quem decide são as palavras-chave."

## Correções aplicadas

### 1. Roteador MoE carregável de arquivo (7.2) — `cortex/src/trinity.rs`
- `load_router_from_file()`: lê `.bitnet` v3+ com tensores nomeados `router_embed`
  (VOCAB×HIDDEN f32) e `router_weight` (HIDDEN×N_EXP i8 ternário, N_EXP 1..=8).
- **Formato canônico (tools/train_router.py export_bitnet):** header 20B
  (magic/ver/vocab/hidden/layers) + model_type(16) + ntensors(4) @36; tensores com
  name 64B + n_orig + n_quant; **avanço pos = n_orig*4 + n_quant** (o export pado o
  weight para caber); leitura do weight = n_orig bytes i8.
- `init_router_weights()`: consome statics `ROUTER_EMBED`/`ROUTER_WEIGHT` se
  carregados; senão LCG (fallback explícito).
- Boot (`neural-kernel/src/main.rs`): tenta `ROUTER.BITNET` via NVMe→AHCI→ATA→USB-MSC.
- Lookup 8.3 OK: `encode_83("ROUTER.BITNET")` → "ROUTER.BIT" casa com a entrada FAT
  (mkfat32 trunca o nome para 8.3).

### 2. Log honesto no router (ADR-0083 §5.1) — `load_router(embed, weight, trained)`
- `trained=true`: "Router MoE loaded (trained): ..."
- `trained=false`: warn "Router MoE weights: DETERMINISTIC FALLBACK (LCG seed=42,
  UNTRAINED)" — nunca anunciar "loaded/trained" para ruído (fecha auditoria 7.2).

### 3. Backprop real (7.3) — `k_ai/src/cognitive.rs` (ADR-0083 §5.2)
- `TransformerTrainer` substitui o esqueleto: `train_forward` (attention full
  causal, todas as camadas, salva ativações), `backward` (gradientes analíticos:
  CE→unembed→rms_final→camadas[FFN grouped, rms, GQA attention, RoPE, q/k/v/o]→embed),
  `update_weights` (STE ternário via `ternary_update` + rms contínuo).
- `self_test()`: modelo pequeno sintético (hidden=16, 1 layer, vocab=16) — **CE loss
  diminui**: 2.7018 → 1.7487 (20 steps) — **PASS no boot QEMU real (T+815)**.
- `Tensor` ganhou `#[derive(Clone)]` (cortex/src/tensor.rs) — necessário para o cache.

### 4. Saudação sem pool canado (7.5) — `cortex/src/cortex.rs` + `bpe.rs`
- Removidos `argmax_row_greeting_only`, `GREETING_BIAS_IDS`, `greeting_candidate_ids`,
  `greeting_step_candidates`, `greeting_position_bias`, `greeting_bigram_bias`.
- Saudação usa `argmax_row_hf_vocab` (argmax real). `is_greeting` só controla
  max_gen (8) e early-exit `text_is_greetingish` (UX legítimo).
- Constrained decode de clima MANTIDO (saída estruturada ≠ linguagem livre).

### 5. Router treinado (7.2/ADR-0083 §5.3) — `tools/train_router.py`
- Dataset rotulado / 7 experts (inclui keywords reais do classify_keywords);
  embedding 99×64 f32 + matriz ternária 64×7 com STE; gate ≥80% → exporta
  `ROUTER.BITNET` (93.5% acc, round-trip OK). Exporta para `tools/target/`
  (gitignored; `mkfat32.find_file` cobre o path).

### 6. Assets opcionais (ADR-0083 §5.5) — `tools/mkfat32.py` + `mkexfat.py`
- `("ROUTER.BITNET", find_file("ROUTER.BITNET"))` — BGE.BIN já era opcional.

## Erros pré-existentes revelados pelo cargo clean (lição AGENTS.md)
`cargo clean -p neural-kernel -p k-nano -p cortex -p k_ai` revelou erros que o cache
incremental escondia (em `crates/k_nano/src/xhci/`):
- `mod.rs:747` — `% ISOC_SLOTS as u16` (type mismatch) → `((... ) % ISOC_SLOTS) as u16`.
- `bringup.rs:1471` — match de tipos incompatíveis (`Some(s) => s, None => None`).
- `bringup.rs:1440/1450` — use-after-move do ring UVC; `configure_uvc_endpoint`
  agora retorna `Option<()>` (ring vive só no static `ISOC_UVC`; o poll usa
  `ISOC_UVC.lock()`).

## Verificação
- `cargo check --release` — 0 erros (warnings conhecidos pré-existentes).
- Boot QEMU (TCG, sem disco): self_test PASS CE 2.7018→1.7487; boot completo até o
  ATA scan (o #PF final = bug conhecido ATA PIO sob TCG, SESSION_243, não desta sessão).
- Boot QEMU (TCG, com disco): FAT32 monta, TLSPINS lido; ATA PIO lento sob TCG
  (BGE.BIN 141MB + LLAMA8B 2GB no caminho) — validação end-to-end do ROUTER.BITNET
  via FAT pendente de boot completo (HW real ou CI Linux).

## Follow-ups
- [ ] Validar "Router MoE loaded (trained)" no boot completo (CI Linux QEMU ou HW real).
- [ ] Replay buffer MoE (`r3::update_with_replay`) conectado ao treino do router
  (ADR-0083 §5.4).
- [ ] `tools/train_router.py` com holdout real (ADR-0083 §5.3 gate >80% já no script).

## Lições
1. **Log honesto é requisito**: componente neural deve reportar origem real
   (`trained` | `deterministic_fallback` | `pseudo`) — padrão "formato + seed + log
   otimista" é o anti-padrão (auditoria 7.2).
2. **cargo clean revela erros reais** que o cache incremental esconde — rodar após
   mudanças estruturais (AGENTS.md já avisava; confirmado de novo).
3. **Constrained decode é legítimo para saída estruturada** (clima/JSON), não para
   linguagem livre (saudação).
4. **FAT 8.3 trunca nomes longos** — "ROUTER.BITNET" → "ROUTER.BIT" no diretório;
   o lookup `encode_83` normaliza, então read_file("ROUTER.BITNET") funciona.
