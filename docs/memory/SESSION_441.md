# SESSION_441 — Fuzz host de `talc_walk_bins` contra gap-lists corrompidas (2 guards de memória)

**Data:** 2026-10-03
**Branch:** main | **Base:** s440
**Pedido:** adicionar testes de fuzz em host para `talc_walk_bins` com gap-lists corrompidas
(loop, node fora do span, size gigante), garantindo `partial=1` **sem pender**.

---

## 1. Motivação

`talc_walk_bins` (`crates/k_nano/src/allocator.rs`, s435) é o **único** lugar do kernel que
lê metadado do TALC **sem confiar nele** — todo o resto do allocator aceita o que o talc
escreveu. Ele:

- lê `bins` (offset 16 do `Talc`) e segue `next`/`size` de cada gap-node;
- alimenta a linha `talc` do HUB HEALTH e os vereditos do hub_triage;
- roda a 2 Hz no HUD e 1/min no triage, sobre um span que é **demand-paged**.

Se um `next` corrompido apontar para um endereço fora do span, a leitura de `size`
(atual `(node+16).read_volatile()`) pode sair do claim → **#PF** — exatamente a classe de
`#PF` rotativo que a s438 passou duas sessões caçando (`vals 0x6/0x13c/0x7ee00001/0x42d`).
Até aqui essa classe era **não testável**: no QEMU a corruptão é rara e o sintoma é um
freeze genérico; no host ela é reproduzível em microssegundos.

## 2. O que o fuzz encontrou (2 guards de memória, ambos load-bearing)

O código original validava `node < base || node >= acme` — insuficiente em dois casos:

| # | Corrupção | Comportamento original | Correção |
|---|---|---|---|
| G1 | `node` perto de `acme` (ex. `acme-8`) | `size` é lido em `node+16` → **fora do span** (página não mapeada → `#PF`) | exigir `node + TALC_GAP_MIN_FOOTPRINT (24B) <= acme` |
| G2 | `next` desalinhado (ex. `base+4`) | `read_volatile` de `*const usize` desalinhado = **UB** (o check de debug do Rust aborta o processo) | exigir `node % align_of::<usize>() == 0` |

`TALC_GAP_MIN_FOOTPRINT = 24` = `MIN_CHUNK_SIZE` do talc 4.4.3 (`LlistNode` 16B + `SIZE` 8B),
lido do fonte. `free += size` → `free = free.saturating_add(size)` (ver §5, honestidade).

## 3. O arnês (`mod talc_walk_fuzz`, 13 testes)

Scratch = `[span | redzone]` numa caixa `Box<[u64]>` só. O **redzone fica atrás de `acme`
com `size = 32`** (valor que o walk aceitaria): se o `size` de um node que cruza `acme` for
lido, o número muda e o teste acusa — **detecta leitura fora do span sem página de guarda**,
portátil no host. `fake_talc()` monta um `Talc<ErrOnOom>` real e grava `bins` no offset 16
(o mesmo `read_volatile` da produção).

| Teste | Classe | Resultado exigido |
|---|---|---|
| `walk_sane_chain_sums_free_exactly` | baseline são (3 gaps, 2 bins) | `partial=0`, free exato |
| `walk_self_loop_aborts_at_cap_without_hanging` | `next = si mesmo` | `partial=1`, `gaps = CAP+1`, < 2s |
| `walk_two_node_cycle_aborts_at_cap` | a→b→a | `partial=1`, `gaps = CAP+1` |
| `walk_all_bins_looping_still_bounded_by_global_cap` | 128 bins em self-loop | CAP é **global** (não 128×) |
| `walk_node_outside_span_is_partial` | `base-8`, `acme`, `usize::MAX` | `partial=1`, sem tocar fora da caixa |
| `walk_giant_size_is_partial_and_never_wraps_free` | `size = usize::MAX` / saturante / 0 | `partial=1`, gap válido anterior conta |
| `walk_free_saturates_instead_of_wrapping` | 4000 gaps **individualmente** válidos, domínios sobrepostos | free = soma cheia, `used` satura em 0 |
| `walk_misaligned_node_is_partial` | `next = base+4` | `partial=1` (**G2**) |
| `walk_header_crossing_acme_never_reads_past_span` | `node = acme-8` (size cai no redzone) | `free=0` (**G1**) + caso-limite `acme-24` **passa** (sem over-reject) |
| `walk_null_bins_and_empty_span_are_partial` | nunca claimado / span vazio | `partial=1` (nunca "0 livre" como fato) |
| `walk_over_real_talc_is_never_partial` | **`Talc` real** (`claim`/`malloc`/`free` de verdade) | `partial=0` — os guards não rejeitam o allocator de produção |
| `fuzz_corrupted_gap_lists_never_hang_and_stay_consistent` | 400 casos, xorshift determinístico (`0xDEECE66D0BADF00D`) | invariantes + budget de 2s |
| `fuzz_single_node_random_pointer_never_pends` | 2000 casos, `next` aleatório (classe do s438) | nunca pendura, `gaps ≤ CAP+1` |

Invariantes comuns (`assert_invariants`): `largest_free ≤ free_bytes`; `gaps ≤ CAP+1`;
`partial ∈ {0,1}`; `partial=1 ⇒ used=0` (ou `= span` nos 2 ramos degenerados);
`partial=0 ⇒ used` fecha com `span − free`.

## 4. Prova de que os testes não são decorativos (mutation testing)

Cada guard foi removido e a suíte **acusou**:

| Mutação | Sinal |
|---|---|
| **M1** sem footprint + sem align | `unsafe precondition(s) violated: read_volatile requires alignment` → **STATUS_STACK_BUFFER_OVERRUN** (abort) |
| **M2** só sem footprint | **STATUS_ACCESS_VIOLATION** (leitura fora da caixa) |
| **M3** CAP 4096 → 100M | `walk_self_loop` **FAILED** (loop pendurado) |
| **M4** `saturating_add` → `+=` | **passou** (ver §5 — guard inalcançável hoje) |
| **M5** sem bounds-check de span | **STATUS_ACCESS_VIOLATION** |

## 5. Honestidade do guard `saturating_add`

Com `size ≤ span` (garantido pelo bound) e `gaps ≤ 4097`, o teto real de `free` é
`CAP·span` ≈ 28 TB no claim de 6.9 GB — o **wrap de u64 exigiria um span de ~4.5 PB**,
fora do budget do kernel. Ou seja: M4 passou porque o guard é **inalcançável**, não inútil.
Mantido como cinto-e-suspensório (se o bound afrouxar, o número publicado continua sendo
o maior possível em vez de mentir um valor pequeno) e **documentado no código como tal** —
regra 419: nada marcado como seguro sem o porque.

## 6. Validação

- **k-nano 265/265** `-t1` (249 + 13 fuzz + 3 novos do `walk_over_real_talc`) — 0 falhas.
- **cortex 126/126** `-t1` (allocator é a base do KV routing s439 — nada regrediu).
- `cargo nk` **0 erros** (37.8s).
- **hermes 287/288** — `permission_gate::tests::test_risk_level_classify` falha **também
  com meu `allocator.rs` revertido no HEAD** (order-dependence de statics, pré-existente,
  passa isolado). **Não é desta sessão.**

## 7. Lições

- **Fuzz de leitura de metadado é o teste que o QEMU não dá**: a classe `#PF` por pointer
  corrompido precisa de *fault em processo separado* para ser visível; no host o teste
  **`STATUS_ACCESS_VIOLATION` é o sinal** — e o check de debug do Rust (`unsafe
  precondition violated`) transforma UB de alinhamento em abort determinístico.
- **Guard de `read_volatile` precisa de DOIS bounds**: o ponteiro estar dentro do span **e**
  o objeto lido inteiro caber nele (`node+24 ≤ acme`). O segundo é o que faltava — `node`
  válido no span pode ter `size` fora dele.
- **Teste de integração com o allocator real é o contrapeso do fuzz**: os 12 testes de
  corrupção provam que os guards pegam lixo; o 13º prova que eles **não rejeitam** o que o
  talc real escreve (`claim`/`malloc`/`free` de verdade, `partial=0`). Sem ele, "passar" podia
  significar "telemetria sempre parcial" no kernel.
- **Mutation testing é o que separa teste útil de teste decorativo**: 5 mutações, 4
  acusadas com sinal concreto (abort / access violation / FAILED). A 5ª expôs um guard
  legitimately inalcançável — e isso é um **achado**, não uma falha do teste.
- **Redzone com valor plausível** (`size = 32`, que o walk aceitaria) detecta leitura fora
  do span **sem página de guarda**: se o byte lido mudar o resultado, o teste acusa. Alternativa
  portátil a `mmap(PROT_NONE)` no host Windows.
- **Falha de teste em crate que não toquei = suspect antes de álibi** (lição s418 aplicada
  ao contrário: `git show HEAD:arquivo > arquivo`, roda, compara — prova em 2 comandos).

## 8. Pendências

- Rodar o arnês em **loop com seeds variadas** (`--ignored` / env seed) como no Nightly,
  fora do gate de CI (hoje: 2400 casos em 30ms, roda todo commit).
- s436b (bughunt do stall pós-`a2_proof`) segue **🔴** no TODO — este fuzz cobre a classe
  `#PF` por metadado corrompido, **não** o stall determinístico T+23.3k (que não reproduziu
  em 3 boots seguidos).