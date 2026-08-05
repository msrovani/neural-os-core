# SESSION_249 — ADR-0085 `.bitnet v6` Formato Canônico + ADR-0084 Fidelidade (2026-08-05)

**Escopo:** Implementação completa das ADRs 0084 (Engine BitNet — fidelidade + kernels CPU) e 0085 (Formato canônico `.bitnet v6` + registro K³CHJ). Fases F0–F6b + F1b + gate docs.

**Status:** Fases F0–F6b + F1b ✅ · 0 erros `cargo check --release --workspace` · 18 testes cortex PASS · 142+ testes workspace · nada commitado (pós-tarefa em andamento).

---

## 1. O que foi entregue

### F0 — Writer canônico + paridade byte-exact
- `tools/bitnet_writer.py` (novo, ~480 LOC numpy-only): `pack_ternary`, `write_header_v6`, `compute_feat`, `write_embed`, `write_rms`, `write_ternary`, `write_q6k` + encoder Q6_K real, `decode_q6k` (port Python), `--self-check` → `tools/golden_v6.bin`.
- `save_model_v6` (cortex.rs) — byte-exato com o writer Python.
- Teste `v6_writer_parity`: `save_model_v6` × `include_bytes!(golden_v6.bin)` — **PASS byte-exact**.

### F1 + F1b — 8 conversores → v6
- Críticos: `convert_bitnet.py` (2B4T: act_type=RELU2, embed Q6_K, feat computado), `train_hw_expert_v4.py` (model_type=HWEXPERT, sem prefixos), `train_router.py` (model_type=ROUTER, posicional).
- Restantes: `convert_gguf_to_bitnet.py` (feat só bit2, act_type do metadata, D3 tied), `convert_falcon3_bitnet.py` (2 normas sintéticas removidas, theta no EOF — bug real corrigido), `convert_safetensors_to_bitnet.py` (reescrito de container prefixado ilegível → body LLM real), `prepare_extra_models.py` (scales 1.0, rms_ffn_norm intermediate), `train_models_gpu.py` (**silu no forward de treino** ADR-0085 §10.3, prefixos mortos).

### F2 — Loader v6 estrito
- `load_model_v6` + `load_llm_v6` (cortex.rs): reserved==0 validado, feat bits 3-7 rejeitados, act_type/embed_type validados, tied⇒zero bytes de unembed (D3), theta só com bit2, `rms_ffn_norm` = intermediate_size exato (D2), fallback v3/v4 com WARN.

### F3/F4 — Kernels (ADR-0084)
- F1 decode: unpack branchless `(pair&1)-(pair>>1)` em `unpack_row_into` (era match por peso) + consts de tiling `ROW_BLOCK_SIZE/COL_BLOCK_SIZE/PARALLEL_SIZE`.
- F2 prefill: activation-parallel gated `m>=8` no `ternary_matmul` (reativa `avx2_bitwise_matmul` com guard de cauda).

### F5 — Fidelidade (M1–M4)
- M1: `act_type` no header → `ffn_act()` nos 4 forwards (relu2 p/ 2B4T, silu default); `relu2` em nn.rs.
- M2: eps RMSNorm 1e-6 → 1e-5 (2B4T); `rms_ffn_norm` canônico = intermediate_size (loader sem pad).
- M3: theta parametrizado no header (feat bit2), default 10000.
- M4: embed Q6_K — encoder Python + loader (bytes brutos, sem materializar 1.31GB) + `embed_lookup` row-wise + `unembed_logits` (matmul por super-bloco p/ tied).
- `bitnet_fwd_parity.py` fortalecido (fixer): aceita magic 0xBE11BE11, default 2B4T, gate logit-level ≤0.5% além do top-5.
- Teste `q6k_decode_matches_python`: decoder Rust × golden Python — **PASS**.

### F6 + F6b — Registro K³CHJ
- `cortex::model` (novo): `ModelKind` + `ModelView` (Llm/HwExpert) + `load_model_v6` dispatcher.
- `ModelHub::register_bytes(slot, data)` — ponto único de carga por bytes.
- main.rs: 4 sites LLM (QEMU-loader + ATA FAT + USB-MSC FAT) roteados via `load_model_v6` com fallback legado; hwexpert/router inalterados (loaders dedicados).
- Teste `v6_roundtrip_load`: save_model_v6 → load_model_v6 → comparação completa — **PASS** (valida o pipeline sem depender do 2B).

### Gate docs
- INDEX.md: ADRs 0084/0085 → status `fazendo` com resumo da implementação. TECNOLOGIAS.md linha 7.2 já refletia v6 canônico. IDEA_BANK #491 já registrada.

---

## 2. BUGS LATENTES encontrados e corrigidos

1. **`f16_to_f32` (gguf.rs) — CRÍTICO:** `sign = ((half>>15) as f32) * -1.0` → quando o bit de sinal é 0, `0.0 * -1.0 = -0.0`, e `-0.0 * mant * powf = -0.0`. **Todo f16 positivo decodificava como -0.0**, quebrando silenciosamente TODOS os dequants GGUF (Q4_0/Q5_0/Q6_K). Fix: `sign = if bit==1 {-1.0} else {1.0}`. Descoberto pelo teste Q6_K cross-check (d=-0).
2. **`num_params` do self-check Python** (F0): fórmula sobrecontava q/o (`7*hidden*q_dim` + extras). Corrigido para a soma real dos tensores; golden regenerado.
3. **LCG de paridade u32 truncava antes do `& 0x7FFFFFFF`**: Python (precisão arbitrária) ≠ Rust u32 wrapping. Fix: u64 no Rust. Sintoma: divergência no 1º f32 após o embed (offset 184) no teste de paridade.

---

## 3. Lições para AGENTS.md

- **`f16_to_f32`: nunca derivar o sinal com `(bit>>15 as f32) * -1.0`** — `0.0 * -1.0 = -0.0` e o produto inteiro vira -0.0. Usar `if bit==1 {-1.0} else {1.0}`. Gate: qualquer dequant f16 (Q4_0/Q5_0/Q6_K/Q8) com escala positiva deve produzir ≠0.
- **Paridade Python↔Rust de PRNG exige u64**: LCG `(x*1103515245+12345) & 0x7FFFFFFF` em Python usa inteiros arbitrários; em Rust u32 `wrapping_mul` trunca ANTES do mask → valores diferentes. Usar u64 e `as u32` na saída.
- **Q6_K encoder**: `d = block_max/(31*127)` e `scale_i = round(127*sub_max/block_max)` → `eff = d*scale_i ≈ sub_max/31`, reconstrução exata no ponto de máximo. Layout espelhado de `dequantize_q6_k_block` (gguf.rs): ql[128]+qh[64]+scales[16]+d(f16) por 256 pesos; element decode = half/lane/l/is.
- **include_bytes! de goldens**: `.gitignore` com `*.bin` engole os goldens usados por `include_bytes!` nos testes → clone fresco quebra o teste. Un-ignorar explicitamente (`!tools/golden_*.bin`).
- **v6: feat é inventário, não flag de arquitetura** — computado do que foi escrito (D5); tied⇒seção unembed NÃO existe (D3), loader nunca inventa zeros.

---

## 4. Pendentes (por design)

- **Boot QEMU v6 com 2B re-convertido**: bloqueado por download externo (~3GB safetensors do HuggingFace; `download_models.py` sem entrada). Desriscado pelo `v6_roundtrip_load` host PASS. Destravar: `huggingface-cli download microsoft/bitnet-b1.58-2B-4T --local-dir target/` → `python tools/convert_bitnet.py` → boot.
- **Fase 7 (W2A8 maddubs)**: gated por WHPX/HW real + gaps de geração (`soft_stride=3`, `MAX_SEQ=64`, 4-8 tokens) — ADR-0084 §3 F4.
- **Retreino TinyStories/RustCoder**: silu já no forward de treino; modelos antigos precisam retreino p/ casar com act_type=0 (ADR-0085 §10.3) — GPU long-running.

---

## 5. Evidência

- `cargo test -p cortex`: 18 testes PASS (parity byte-exact, Q6_K cross-check, round-trip v6).
- `cargo test --workspace --exclude neural-kernel --exclude boot`: 142+ testes, 0 falhas.
- `cargo check --release --workspace`: 0 erros (warnings policy-accepted).
- Syntax check Python: 8 conversores + writer + parity, todos PASS.
