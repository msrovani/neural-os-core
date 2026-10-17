# SESSION_483 — Passo 2 do hub: Reranker alcançável + load-on-first-use (seam, não loop eager)

**Data:** 2026-10-17 · **Estado:** v1.9.99-s483 TEST · **Branch:** `main`
**Continua:** s482 (Passo 1, footprint por slot). Este é o **Passo 2** do plano do @oracle.

---

## 1. Decisão do HUMAN

Optei por **B: 2a + 2b completo com hook gated** quando ofereci (D) só-2a / (C) 2a+dead-weight-removal /
(B) 2a+2b completo. O 2b é load-on-first-use de verdade, com hook registrado, mesmo que hoje só
Pro/Active tenham consumidor para dispará-lo.

## 2. 2a — Reranker alcançável (+ gate)

`crates/neural-kernel/src/main.rs`:
- `pio_cap` do Reranker: **32MB → `slot_footprint_mb(slot.name())`**. Era inalcançável por
  construção: artefato de 165MB vs cap 32MB + `if sz > pio_cap { continue }` = skip sempre, com
  log `info` parecendo tentativa. `IMPLEMENTED` que se apresenta como wired.
- **Gate de headroom antes do `read_file`**: `heap_headroom_critical()` → recusa + `warn`.
  Necessário porque corrigir o cap torna a leitura de 165MB *possível*; sem gate é o padrão
  OOM → #PF → park (s415-420).

`crates/cortex/src/model_fit.rs`: braços `vision`/`siglip`/`vit`/`encoder` → 33MB,
`reranker`/`rerank`/`bge_reranker` → 165MB, `agent`/`qwen3b` → 752MB.

**Lição que quase me pegou:** o footprint **medido** (Passo 1) só existe **depois** do load. No
momento de *dimensionar* o load, `measured_slot_mb` é `None` e `slot_footprint_mb("reranker")`
caía em `_ => None` → meu primeiro edit dava `unwrap_or(0)` = cap 0 = **nunca carrega**, pior que
antes. O footprint tem duas metades: **medido** (pós-load, Passo 1) e **hint estático** (pré-load,
este passo). Faltava a segunda.

## 3. 2b — load-on-first-use (seam, não loop eager)

`crates/cortex/src/model_hub.rs`:
- `pub type SlotLoader = fn(ModelSlot) -> bool` + `static SLOT_LOADER: AtomicUsize` (0 = não
  registrado) + `register_slot_loader(f)` + `try_load_on_demand(slot)`.
- `ensure_slot_loaded(slot)` — **não pega nenhum lock**: o loader usa `HUB` e `TicketLock` não é
  reentrante (s438); carregar dentro do lock seria deadlock.
- `generate_from_slot()` chama `ensure_slot_loaded` **antes** do `match` (= antes de qualquer lock).
- `slot_real` virou `pub` (o bin precisa dela no adapter).
- Padrão bridge idêntico ao da TLS (SESSION_241): tipo na crate FE, `register_*` na FE, o boot
  registra com cast explícito, o consumidor chama a API da FE.

`crates/neural-kernel/src/main.rs`:
- **Removidas as 5 chamadas eager** `try_hub_slot_fat(...)` (Reranker/Vision/Pro/Learner/Agent).
- `register_slot_loader(hub_load_on_demand)` no lugar.
- `hub_load_on_demand(slot) -> bool`: adapter de 3 linhas. `try_hub_slot_fat` retorna `()` e tem
  vários `return;` — mudar a assinatura seria diff grande sem ganho; o bool sai do estado real
  (`slot_real`) depois da tentativa.

O boot deixa de fazer 5 leituras PIO por boot sem ninguém ler o resultado.

## 4. Correção minha: "sem consumidor" era meio errado

Eu tinha dito que Vision/Reranker/Learner/Agent não têm consumidor. **Metade errado:**
`generate_from_slot` existe **e é chamado** (`cortex.rs:5845`, path do Trinity/MoE). O que não
existe é consumidor de Vision/Reranker/Learner/Agent — para eles o hook simplesmente nunca dispara
(custo zero, que é o resultado correto). Active/Pro são os que gatilham.

## 5. Prova

- **Build** `cargo build --release`: **0 erros**. Imagem fresca.
- **Host test** `cargo test -p cortex --lib model_fit`: **6/6**.
- **Teste do s483 reescrito**: a versão s482 fixava `slot_footprint_mb("vision") == None` como
  *precondição* — ou seja, **fixava a mentira como expectativa**. O s483 afirma a verdade nova:
  **nenhum** slot do hub pode responder `None` (senão `slot_too_tight(None)==false` = "sempre
  cabe"). Verificado: falhou com `left: Some(33), right: None` antes da correção — o fix estava
  certo, a expectativa é que estava errada.
- **Runtime (falcon3-3b, 1 nó × 7G, 180 s)**: `T+8359` vivo, `[PANIC]` literal 0, `tick=32`.
  Não regrediu.
- **`cargo test -p cortex --lib` (suite completa)**: crash `0xc0000096 STATUS_PRIVILEGED_INSTRUCTION`
  — **pré-existente**, provado pelo método do repo: `git stash` → rodar → **crasha igual** →
  `stash pop`. (SESSION_418 documenta o mesmo crash em k_ai/hermes.)

## 6. 2a e 2b: runtime UNKNOWN (bloqueio honesto)

Sob QEMU, `qemu_dev_skip_models` (`main.rs`, `hv.is_sandbox()`) pula o bloco do hub inteiro.
Logo, neste boot:
- o log `loader registrado (load-on-first-use; 0 leituras PIO no boot)` **NÃO aparece**;
- `SLOT_LOADER` fica `0` → `try_load_on_demand` retorna `false` → `generate_from_slot(Pro)` cai no
  caminho antigo (HUB vazio → `None` → Active). **Fail-safe por default**, comportamento idêntico
  ao anterior no QEMU.

Isto é **by design e seguro**, mas significa que **2a e 2b são IMPLEMENTED, não VERIFIED**:
aceite em runtime exige HW real (ou o gate de sandbox liberado pelo dono — decisão que o @oracle
explicitamente marcou como do Maintainer). Registrei bloqueado em vez de declarar "provado".

## 7. Bug pré-existente (mantido do s482, sem regressão)

`#PF storm` sob falcon3 persiste com **IP migrante** — o sítio muda a cada boot, que é a assinatura
da classe wild-write/corrupção (s438/s415-417):
- s482: `ip=0xffffffff808f2204 cr2=0xfb8`
- s483: `ip=0xffffffff80912334 cr2=0x1fb8`
- baseline s479: `ip=0xffffffff80216bc0 cr2=0x40000007fff8` + `ip=0xffffffff80b341b6 cr2=0x3f4ccccd`

**Não** é do Passo 2 (o baseline já tinha 2 storms). **Não** é whack-a-mole: precisa de watchpoint
para capturar o *writer* (`tools/watch_corruption.ps1` / QEMU monitor). Simbolização por `llvm-nm`
no ELF da árvore continua falhando (faixa de endereços divergente) — não nomear símbolo sem extrair
o ELF de dentro da ESP (SESSION_446).

## 8. Pendentes

- **Aceite de runtime de 2a/2b**: HW real, ou liberação do gate de sandbox pelo dono.
- **Passo 3**: evict explícito de 1 slot (`Active ↔ Pro|Agent`) com `heap_headroom_low()`.
- **NeuralFS ainda eager**: o loop `try_hub_slot_neuralfs` (main.rs:5046) continua no boot — deixei
  fora do 2b porque é outromeio de transporte e removê-lo é escopo maior. Registrar.
- **#PF storm sob falcon3** (pré-existente, §7).
- **Passo 0 pendente**: `validate_niche_blob` + nomes canônicos no build (precisa de artefato
  Vision/Reranker para o footprint medido ter o que medir).

## 9. Lições

- **Footprint tem duas metades**: medido (pós-load) e hint estático (pré-load). Ter só a primeira
  deixa o dimensionamento do load caindo em `None` — e `None` no `slot_too_tight` significa
  "sempre cabe", a mentira mais silenciosa que existe.
- **Teste que fixa a mentira como precondição atrasa o fix.** O `assert_eq!(..., None)` do s482
  era um documento do bug, não uma especificação. Passou 6/6 enquanto a mentira valia.
- **"Sem consumidor" precisa ser medido, não inferido.** `generate_from_slot` existe e é chamado;
  eu confundi "sem `generate_from_slot` para Vision" com "sem consumidor". O grep de caller é a
  única prova.
- **Hook que nunca dispara é custo zero, não bug** — desde que o default seja seguro (seam não
  registrado = `false` = caminho antigo).
- **Crash de host se prova com stash, não com argumento** (0xc0000096 em cortex = pré-existente).
- **Loop eager é dívida que não aparece no QEMU**: sob sandbox ele nunca roda, então "remover" e
  "testar" são coisas diferentes — só HW real prova.