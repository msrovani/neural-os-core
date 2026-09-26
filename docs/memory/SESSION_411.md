# SESSION_411 — A2 proof: 1 token real (lost wakeup do AP) + mesh node_id + BPE Falcon3 + posture

**Motivo:** fechar o "marco zero" do plano de cognição — (1) FALCON3-3B gerar ≥1 token real num run QEMU; (2) mesh de 2 nós convergir; (3) o token sair legível (BPE). Terminou numa caçada de hang silencioso do prefill (lost wakeup no idle dos APs) e em três bugs de honestidade.

## Mesh 2 nós — colisão de node_id (causa-raiz + fix)
- **Sintoma:** `tools/run-qemu-p2p-mesh.ps1` subia 2 nós (socket netdev) mas ambos logavam `node_id=15`, `peers=0`, `role=Master nodes=1` — não convergiam.
- **Causa-raiz:** o script colocava o flag netmode em ~`0x100100000` (calculado de `modelEndAddr`), mas `detect_qemu_net_mode` (`hermes/net.rs`) só lê CANDIDATES baixas (`0x0200_0000`/`0x1640_0000`) e exige `addr < ram_end`; com guest ≤4G o scan alto nem roda. Flag ignorada → ambos ficavam com o IP default do slirp `10.0.2.15` → `node_id()` (`k_nano/net/mesh.rs`, deriva de `ip[3]` senão `mac[5]`) = **15 nas duas** → `add_or_update_node` deduplica por node_id → 1 nó.
- **Fix:** `$netmodeAddr = 0x16400000` (= `NETMODE_LOADER_PHYS`).
- **Validação (2×2G/2c, `-NoDisk`, gtk):** A `netmode=STATIC 10.0.3.2 node_id=2 Master`; B `10.0.3.3 node_id=3 Worker→Memory`; `peers=1` nos dois; TOFU settled; `role-assign` estável; SkillSync 17 skills Master→Worker (`mesh_g3_probe` aplicada); MKTP 16; CRDT `peer node=3 v=34`.

## a2_proof — hang do prefill (diagnóstico + causa-raiz + fix)
- **Sintoma:** 3 runs travavam em `prefill_slice slow id=1 layer=1/22` e ficavam 8+ min sem layer 2; a prova (`a2_proof submit id=2`) nunca era claimada.
- **Hipótese inicial (@oracle):** barrier SMP wedged (`wait_barrier` sem deadline). Implementado deadline de 60 s + RAII no latch — **não resolveu**.
- **Instrumentação decisiva** (enter/exit por slice + matmul enter/exit + barrier wait + diag do `poll_slice` já na 1ª chamada):
  ```
  prefill_step enter layer=0/22 proof=0
  matmul enter m=2 k=3072 n=3072 aps=3
  matmul exit ok=true us=355885          <- SMP OK (barrier NAO era a causa)
  prefill_step exit layer=1 applied=1 us=4241359
  ```
  e **nenhum** `prefill_step enter layer=1` depois → o AP parou de fatiar.
- **Causa-raiz:** **lost wakeup**. O slice de ~4,2 s atrasa a UI → `present_overdue()` liga `UI_YIELD_INFER` → `try_infer_poll_slice()` devolve false → `ap_idle_loop` cai no `hlt`. Quando o display recupera e o flag limpa, **ninguém envia IPI** (sem submit novo — o job já está ACTIVE) → o AP dorme para sempre e o job ativo nunca retoma.
- **Fix (`k_nano/src/smp/ap_work.rs`):** o AP só entra em `hlt`/`mwait` quando `!ui_yield_infer()` **e** um 2º `try_infer_poll_slice()` confirma ociosidade (fecha o TOCTOU); sob gate transitório faz espera bounded (`sleep_us(1000)`) + `continue` — retoma sozinho quando o flag limpa. Log 1×/episódio `ap idle gated by ui_yield (bounded retry)`.
- **Bônus fail-closed:** deadline de 60 s no barrier SMP (fallback single-core honesto, nunca hang silencioso) + RAII no `SLICE_BUSY` (nenhum early-return deixa o latch preso).

## a2_proof — ACEITE (marco zero)
`BOOT.LOG` em disco (LBA 2048), run solo 6G/4c WHPX com `FALCON3.BIN`:
```
a2_proof claimed_by=ap id=2
prefill_done id=2 tokens=2 layers=22 slices=44 us=69042824
a2_proof decode_one id=2 step=1 tok=12 decode_us=61 piece_len=1
a2_proof done id=2 total_us=69137148 prefill_us=69042824 decode_us=1737 toks=1 out_len=1
```
1 token real gerado por forward completo (22/22 layers, 69,0 s de prefill). Sem BPE o token cai no fallback char99 (`tok=12` → `)`).

## BPE Falcon3 (Q3) — token legível
- O tokenizer HF **já estava no repo**: `target1/falcon3/tokenizer.json` (BPE, vocab **131072**, 128810 merges, `Ġ`=espaço; especiais `<|startoftext|>`=10 / `<|endoftext|>`=11).
- **Gap real era no gerador:** `tools/export_bpe_bin.py` só conhecia Llama-128k (`<|begin_of_text|>`) e SP32-32k. Fix: fallback `_first_id` para os especiais Falcon3 + preview ASCII-safe (console Windows cp1252 quebrava com `UnicodeEncodeError` ao imprimir `Ġ`).
- `python tools/export_bpe_bin.py target1/falcon3/tokenizer.json target/bpe_vocab.bin` → BPB1 1422KB, `vocab_n=131072 bos=10 eos=11 merges=0`.
- Loader: `-device loader,file=target/bpe_vocab.bin,addr=0x150000000` (acima do FALCON3.BIN @0x100000000+989MB, dentro do scan `[0x100000000..0x180000000)`).
- **Resultado:** `BPB1 found @0x150000000` + `BPB1 LOADED vocab_n=131072 bos=10 eos=11` + `a2_proof setup id=2 bpe=1 prompt_len=6` + `a2_proof decode_one id=2 tok=18665 piece_len=5` + `[JARBAS] JARBAS:  quad`. **`tok=18665 → " quad"`** (subword real) vs `)` sem BPE.
- Nota: com BPE o prompt "oi" tokeniza em 6 tokens (vs 2) → prefill 170 s (vs 69 s).

## posture FAIL (HUD) — amostra mínima
- `hub_posture_sev` (`cortex/src/decision.rs`) julgava por razão (`esc*2 > total`) **sem amostra mínima**: 1 escalate (`total=1`) declarava FAIL (log `posture FAIL a0 x0 e1 L0` no boot). Contadores são cumulativos (sem reset), então recuperava só quando `auto_ok` acumulava.
- Fix: `POSTURE_MIN_SAMPLES = 4` + lógica extraída pura `posture_from_counts(auto, abs, esc)` (testável sem tocar statics globais); `total < 4 → n/a`. Alinha com a doutrina `n/a ≠ 0` e espelha o `n >= 8` de `trusted`.

## Lições (para AGENTS.md)
- **Lost wakeup por gate transitório:** quem cede por flag transitório (`ui_yield`) e dorme em `hlt`/`mwait` depende de um wake que pode nunca chegar (job já ACTIVE ⇒ sem submit ⇒ sem IPI). Regra: re-checar bounded antes de dormir.
- **Serial sob contenção SMP perde linhas inteiras** (trunca/entrelaça): a evidência canônica é o `BOOT.LOG` em disco, não o serial.
- **Instrumentação enter/exit por unidade de trabalho** localiza hang em 1 run; a hipótese do oracle (barrier) foi refutada por medição (`matmul exit ok=true`).
- **Id derivado de config externa:** testar unicidade no boot — flag não-lida colapsa todos os nós no default (`node_id=15` = IP slirp).
- **Formato de export é contrato:** BPB1 precisa dos especiais corretos (`<|startoftext|>`/`<|endoftext|>`) e `vocab_n == model.vocab`.

## Verificação
- `cargo check --release` → **0 erros**; `cargo build --release` OK (imagem regenerada).
- `cargo test -p cortex --lib -- --test-threads=1` → **91/91**; `-p k-nano --lib` → **210/210**; `-p hermes --lib -- --test-threads=1` → 229; `-p jarbas` → 107; `-p skill-registry` → 11.
- Boots: mesh 2 nós (convergência), solo 6G `a2_proof done toks=1`, solo 6G+BPE (token legível `quad`).
- Commits: `320f621e` (A2 + skill model-born + D-accel/stream), `4d3a2e3d` (lost wakeup + deadline barrier + netmode mesh), `166ca058` (export_bpe_bin Falcon3), `f6a121b8` (posture amostra mínima).
