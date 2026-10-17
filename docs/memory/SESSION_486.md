# SESSION_486 — Passo 5-6 do hub: poda verificada (2 mortos apagados, 2 premissas refutadas)

**Data:** 2026-10-17
**Continua:** s485 (Passo 3, evict explícito). Este é o **Passo 5-6** do plano s481
(podar `fat_names_for`, apagar `slot_from_bitnet_bytes` + `load_models_multi`).

## 1. A regra que governou o passo

O plano s481 dizia "sem callers" para os dois alvos de deleção e "peso morto" para o
MICROPY.WASM. A lição da PRÓPRIA s481 ("grep que exclui um diretório produz conclusões
falsas") exigiu re-verificação no repo INTEIRO antes de qualquer deleção — e ela mudou
o quadro: **2 premissas caíram, 2 deleções se confirmaram**.

## 2. Verificação (grep repo inteiro: crates/ tools/ docs/, .rs .py .md)

| Alvo | Veredito | Evidência |
|---|---|---|
| `load_models_multi` (cortex.rs:5962) | **MORTO** | único match em código = a própria definição |
| `slot_from_bitnet_bytes` (model_hub.rs:546) | **MORTO transitivo** | único caller em código = `load_models_multi` (5970/5982) |
| `ModelSlot::from_name` | **VIVO** | model_fit.rs:175 (`slot_footprint_mb` — path do cap do Reranker s483 + `maybe_fit`) → fica |
| MICROPY.WASM "peso morto" (s484) | **PREMISSA REFUTADA** | módulo hermes é VIVO: `try_init_at_boot()` em main.rs:2755 (boot path, k33 "micropython"); lê VFS `/micropython/micropython.wasm` com fallback MVP embutido; `eval` recusa honesta `micropython_mvp_stub` |
| Podar `fat_names_for` | **PREMISSA MOOTA** | morreu com o cancelamento dos renames (s484: truncamento simétrico, nomes longos CASAM); nomes = outputs reais do mkfat32 (VISION/LLAMA8B/RUSTCDR3/RERANKER/LEARNER/AGENT/MICRO) + política s458 (PRO.v6/FALCON7B guardados p/ adiante) |

## 3. Feito

1. **Apagado `slot_from_bitnet_bytes`** (model_hub.rs, ~32 LOC) — heurística
   params→slot / tamanho→slot. Sem callers após a 2ª deleção.
2. **Apagado `load_models_multi`** (cortex.rs, ~34 LOC) — loader multi-blob do boot
   que o load-on-first-use (s483 2b) tornou órfão. Era o único caller dos dois acima
   (e usava `from_name`, que sobrevive por model_fit).
3. **MICROPY.WASM — nada a podar, registro corrigido**: o arquivo NEM EXISTE em
   `target1/`/`models/` → a linha condicional do mkfat32.py (`find_file("MICROPY.WASM")`)
   já é no-op hoje (empacota nada, custo zero). O módulo hermes fica (front viva,
   lição s449: "sem consumidor é estado temporal"). A perna FAT→VFS nunca foi conectada
   (leitor lê path VFS `/micropython/micropython.wasm`, não o nome FAT da raiz; sem
   mount `/micropython` no bin) — anotado como fato, não corrigido (YAGNI até a frente
   precisar de bytecode real).
4. **`fat_names_for` — no-op honesto**: podar os nomes era o Passo 5 DEPOIS do Passo 1
   (nomes canônicos gravados pelo build). O Passo 1 foi cancelado na s484 (renames
   inúteis), então a premissa da poda morreu junto. As listas são o contrato de nomes
   que o build produz/produzirá — podar quebraria a política s458.

## 4. Prova

- `cargo check --release` → **exit=0** (0 erros).
- `cargo test -p cortex --lib model_hub` → **1/1** (evict s485 intacto).
- `cargo test -p cortex --lib model_fit` → **6/6** (path `from_name`/`slot_footprint_mb` intacto).
- Deleção líquida: ~66 LOC de código morto compilado, zero mudança de comportamento.

## 5. Lições

- **"Sem callers" de plano antigo é hipótese, não fato** — o s481 escreveu "sem callers"
  para funções que TINHAM caller entre si (`load_models_multi` → `slot_from_bitnet_bytes`);
  a cadeia de morteza é transitiva e precisa ser tracada até a raiz antes de apagar.
- **Peso morto declarado por grep de nome pode ser módulo vivo** — "MICROPY.WASM zero
  leitores" (s484) confundiu o ARQUIVO FAT com o MÓDULO: o módulo é boot-wired; o arquivo
  nem existe. Grep de nome de arquivo ≠ grep de caminho de leitura.
- **Poda condicionada a passo cancelado é no-op** — o Passo 5 (podar fat_names_for)
  dependia do Passo 1 (nomes canônicos); cancelado o Passo 1 na s484, a poda perdeu o
  motivo. Planos em cadeia: quando um elo cai, re-avaliar os elos seguintes.

## 6. Pendentes

- Aceite de runtime do hub (validador s484 + evict s485) — HW real ou gate liberado.
- `#PF storm` sob falcon3 (pré-existente, IP migrante — watchpoint).
- Direção B do evict (recarregar Active sob demanda) — só com caminho runtime.
- Frente MICROPY (bytecode real): se um dia precisar, conectar FAT→VFS ou embutir.
