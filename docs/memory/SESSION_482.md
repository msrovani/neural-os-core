# SESSION_482 — Passo 1 do hub: footprint medido por slot (+ harness de #PF que parou de mentir)

**Data:** 2026-10-17 · **Estado:** v1.9.99-s482 TEST · **Branch:** `main`
**Continua:** s481 (resolve/informa + tabela de nichos). Este é o **Passo 1** do plano que
@oracle desenhou para o hub (honesty → alcance → evict).

---

## 1. O que mudou (Passo 1 — footprint por slot)

`crates/cortex/src/model_hub.rs`:
- `static SLOT_FOOTPRINT: [AtomicU64; 8]` — bytes **medidos** por slot, 0 = sem blob.
- `note_footprint(slot, bytes)` — chamado no topo de `register_bytes` (cobre os 4 caminhos:
  GGUF / LLM-v6 / HWExpert-v6 / redirect). No redirect HwExpert→Active grava **nos dois**,
  senão o footprint de Active fica stale.
- `measured_slot_mb(slot) -> Option<u64>` — medido em MB, `None` = nunca inferido.

`crates/cortex/src/model_fit.rs`:
- `slot_footprint_mb()` agora consulta o medido **antes** do header global:
  medido > header global > hint estático.

O que isso mata (medido, não deduzido): antes `vision`/`reranker`/`agent` não tinham braço no
`match` → `_ => None` (linha 200) → `slot_too_tight(None) == false` → **"sempre cabe"**; e
active/pro/learner respondiam pelo **mesmo** `loaded_model_header` (do *último* carregado).

## 2. Prova

- **Host test** `model_fit::ram_policy_tests::slot_footprint_e_por_slot_nao_header_global`:
  antes `slot_footprint_mb("vision") == None`; agora vision=33, generator_pro=1900, e o par
  deixa de ser indistinguível. 6/6 em `cargo test -p cortex --lib model_fit`.
- **Build** `cargo build --release`: 0 erros (só os warnings conhecidos).
- **Runtime (falcon3-3b, 1 nó × 7G, 200 s)**: `model 1012764KB` (tamanho correto), `LLM LOADED`,
  `[PANIC]` literal ausente, `T+10165`, `tick=32` (era 1 no s481). **Não regrediu.**

**Runtime do footprint em si: BLOQUEADO (UNKNOWN).** O falsificador do @oracle ("carregar Vision
e ver `generator_pro` parar de reportar 33MB") exige um blob Vision/Reranker — **não existe no
repo** (só `.bin` de dep-graph/query-cache). Depende do Passo 2 (build dos nomes canônicos).
Taxonomia ADR-0113: a **lógica** está VERIFIED por host test; a **propriedade** runtime é UNKNOWN.

## 3. Achado colateral: harness de #PF mentia de dois jeitos

`tools/run-mesh-model.ps1` acusava `ALGUM NÓ COM #PF` num boot saudável. Duas causas, ambas
medidas no log:
- **5 das 12 ocorrências de `#PF` são TEXTO** (`patch_idt: ... #UD/#GP/#PF hook`, `avoid #PF`),
  não faults. Fault real exige `[EXC] #PF` (7 no log).
- **`panic` sem âncora** casava `panic=true` do registro cross-boot do `k_ai` SELF lendo o
  ramlog do boot **anterior** (mesma classe do s477).

Corrigido para usar o marcador que o kernel **já emite** (`#PF storm` / `RECOVER-HALT`) e o
literal `[PANIC]`. Validei contra 3 logs: falcon3-atual → FALHA(storm), falcon3-baseline →
FALHA(storm), BGE → SAUDÁVEL.

## 4. Bug PRÉ-EXISTENTE aberto: #PF storm sob falcon3

O boot do falcon3 tem `#PF storm` real (`streak=3+ park core`), **não** causado pelo Passo 1:

| | storms | IP | cr2 |
|---|---|---|---|
| baseline s479 (pré-Passo1) | **2** | `0xffffffff80216bc0` | `0x40000007fff8` (heap TALC) |
| | | `0xffffffff80b341b6` | `0x3f4ccccd` (= `0.8f`, spill de `AffectSnapshot::NEUTRAL`) |
| atual (com-Passo1) | 1 | `0xffffffff808f2204` | `0xfb8` |

O IP muda a cada boot e o baseline tem **mais** — assinatura da classe **wild-write / corrupção**
(o sítio do crash migra; s438/s415-417). **Não é whack-a-mole**: precisa de watchpoint
(`tools/watch_corruption.ps1` / QEMU monitor) para capturar o *writer*. Item aberto, não tocado aqui.
Nota: simbolização por `llvm-nm` no ELF da árvore **falhou** (nenhum símbolo abaixo do IP) —
os endereços de runtime e o ELF não estão na mesma faixa (SESSION_446: não atribuir crime a
outro build; extrair o ELF de dentro da ESP antes de nomear símbolo).

## 5. Plano (do @oracle, com os 3 achados verificados)

- **AirLLM NÃO é estratégia de troca.** `gguf_streaming.rs:322` = *"generate STUB — attn/FFN não
  wired — refuse inventar tokens"*; `:86` diz `net=0 callers`. É um `bool` de *fit*
  (`needs_airllm`), não um scheduler. E layer streaming sobre ATA PIO = I/O por token.
  Residência (Active sempre + 5 pequenos ~644MB sob demanda) + **1 evict explícito**
  Active ↔ (Pro|Agent) é a troca real.
- **Reranker inalcançável:** `main.rs` dá `ModelSlot::Reranker => 32MB` de `pio_cap` contra
  artefato de 165MB, e `if sz > pio_cap { continue }` → skip sempre, mas o log sai `info`.
  `IMPLEMENTED` que se apresenta como wired. (Passo 2.)
- **Boot é eager** (`main.rs:5032-5036` tenta Reranker→Vision→Pro→Learner→Agent em todo boot).
  Passo 2 troca por load-on-first-use.
- **Nome canônico (s481) fica; `role` no header v7 não** — slot-id é chave interna, nome é chave
  de descoberta. O que faltava era registro de footprint em runtime (este Passo 1).

Ordem: 1 honesty (feito) → 2 alcance+demanda → 3 evict com gate de headroom.

## 6. Pendentes

- Passo 2: `pio_cap` do Reranker + load-on-first-use (falsificador: boot cold com `RERANKER.V7`
  presente ainda mostra `reranker=1` → o eager não foi revertido).
- Passo 3: evict explícito de 1 slot com `heap_headroom_low()`.
- Runtime do footprint (Vision presente) depende do build dos nomes canônicos.
- **#PF storm sob falcon3** (item pré-existente, §4).

## 7. Lições

- **Contar occurrence de um símbolo no log ≠ contar o evento.** `#PF` aparecia 12×, sendo 5
  texto de log. O fault tem forma (`[EXC] #PF`); use a forma, não o nome solto.
- **Harness que já tem o marcador não deve adivinhar o limiar.** O kernel emite `#PF storm` /
  `RECOVER-HALT`; o harness reimplementava uma heurística (`PF > 0`) e mentiu.
- **Baseline antes de culpar o diff:** o único log do falcon3 era do run novo (o harness
  sobrescreve). O baseline veio de `boot_mesh_c_FALCON3.txt` (mesmo modelo, matriz s479) — e
  mostrou **mais** storms. Sem isso eu teria的时间里 aberto um bug de corrupção no Passo 1.
- **Falsificador que não roda é UNKNOWN, não "provado".** O do Passo 1 exige um artefato Vision
  que não existe; registrei como bloqueado em vez de declarar o footprint "demonstrado".