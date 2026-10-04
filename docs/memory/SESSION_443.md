# SESSION_443 — Consumers de longa vida no TALC (2º consumer) + HISTOGRAMA de fragmentação (o dado real)

**Data:** 2026-10-03 · **Versão:** v1.9.99-s443 · **Branch:** `main`
**Crates:** `k_nano` (`TalcBuf` + histograma), `k_ai` (`ContextWindow`), `hermes` (telemetria)

## 1. O que já existia e o que não

A s439 roteou o **KV cache** para o TALC (`KvTalcPage`). O pedido desta sessão era
duplo: rotear os **outros** consumers pesados de longa vida e **estudar a fragmentação
com dados reais**. Auditoria do que ainda vivia no bump:

| consumer | onde estava | veredito |
|---|---|---|
| KV cache | `KvPageList` → `alloc_talc_routed` (s439) | já roteado |
| **`ContextWindow`** (`k_ai`) | `String`/`VecDeque` no **bump** | **vazamento monotônico** — achado desta sessão |
| experts MoE | arena Cortex | arena (não é o span do TALC) |
| pesos do modelo | `map_loader_region_ro` (FAT) | fora do heap |

## 2. O achado: `ContextWindow` não era "só" bump, era um vazamento

O bump allocator **nunca devolve memória** — o `dealloc` do híbrido é no-op. E
`ContextWindow` é exatamente o pior caso: **longa vida com churn**. `maybe_compact`
*remove* mensagens quando o orçamento estoura; no bump, cada mensagem descartada
deixava seus bytes retidos **para sempre**. Cada conversa adiciona e remove texto, e a
janela do bump (~2030MB) satura por conta de histórico de chat — invisível, porque
nenhum número aponta para "chat".

Isto reposiciona o roteamento: não é otimização, é **conserto de vazamento**.

## 3. `TalcBuf` — o tipo que devolve memória de verdade

`String`/`Vec` não servem: são atrelados ao allocator global (bump-first). Novo tipo em
`k_nano::allocator`:

- `with_capacity`/`from_str`/`push_str`/`as_str`/`shrink_to_fit`;
- **`Drop` devolve o chunk ao TALC** (free real);
- falha = `None`/`false` (**fail-closed**, nunca `oom()`), conteúdo intacto;
- `Send` (posse exclusiva), sem `Sync`.

**`grow` NÃO usa `realloc` — e esse é o detalhe load-bearing:** a lição s434b diz que,
para um chunk TALC-resident cujo realloc falha, `HybridAllocator::realloc` chama `oom()`,
ou seja **derruba o kernel**. Um buffer fail-closed não pode ter `oom()` no caminho, então
`grow` é alloc+copy+free, que devolve NULL honesto.

`ContextWindow` migrou: `role`/`content`/`system_prompt` são `TalcBuf`; `add()` passou a
retornar `bool` (recusado ≠ "guardei"); `talc_bytes()` expõe o quanto vive no TALC.

## 4. O dado real: histograma de fragmentação

Totais dizem **QUE** o TALC está fragmentado; não dizem **DE QUE JEITO** — e o jeito
decide a ação certa. `talc_walk_bins` passou a contar os gaps por faixa
(`TALC_HIST_BOUNDS`: `<8KB / 8-64KB / 64-256KB / 256KB-1MB / 1-16MB / >=16MB`), com
faixas escolhidas pelos tamanhos reais dos consumers (página de KV = 4096B, expert = KiB..MiB,
spill de 2MB). Zero alocação: só incrementos. O histograma sai no slog
(`ok talc u..M f..M lg..M g.. hist=a/b/c/d/e/f`), no snapshot JSON que o LLM lê, e num
campo de `HeapObserve`.

`tools/talc_frag_report.py` fecha o estudo: lê um log real de QEMU e responde
**"poeira" ou "poucos gaps grandes"?** —dust-dominant exige agir nos consumers pequenos;
fragmentação benigna significa que compactar é trabalho jogado fora. Testado ponta a ponta
num log sintético com a linha real do kernel (`EXIT=0`, CSV + veredito dust-dominant).

## 5. Validação

| checagem | resultado |
|---|---|
| `cargo test -p k-nano --lib -t1` | **276/276** (+11: 3 do histograma, 7 de `TalcBuf`, +1 do fuzz) |
| `cargo test -p k_ai --lib context_window -t1` | **5/5** |
| `cargo test -p hermes --lib -t1` | 309 passed, 1 failed (`permission_gate`, **pré-existente** da s440) |
| `cargo test -p cortex --lib -t1` | 126/126 |
| `cargo nk` | **0 erros** |
| `talc_frag_report.py` em log sintético | `EXIT=0`, relatório + CSV |
| `tools/test_talc_frag_report.py` | **3/3** — extrai o template do slog do **fonte do kernel**, formata a linha real e prova que o tool a entende (lição SESSION_411: teste de formato usa o byte do produtor, não cópia) |
| regra 419 | `hist=`, `TalcBuf` e `ok talc u` presentes no `kernel.elf` |

## 6. Falha pré-existente encontrada e provada

`cargo test -p k_ai --lib` **não completa**: aborta em `sgdb::bench::tests::d_series_100k`
com `STATUS_STACK_BUFFER_OVERRUN` (0xc0000409). Provado pré-existente do jeito que a lição
s418 manda: arquivo meu revertido para HEAD → **mesmo abort, mesmo exit code**; arquivo
restaurado byte-idêntico depois. Não é regressão s443 — mas a suíte completa de `k_ai`
continua não rodando, e isso é limitação, não sucesso.

## 7. Lições

1. **"Mover para o TALC" e "consertar vazamento" são o mesmo trabalho aqui.** O bump não
   tem free; qualquer consumer de longa vida com churn que fique nele é um vazamento
   silencioso. Antes de pedir otimização, pergunte quem está no bump e morre.
2. **Não reintroduza `realloc` num tipo fail-closed.** `oom()` é o comportamento correto
   do allocator global e o comportamento ERRADO de uma API que promete devolver `false`.
   A regra do s434b vale para quem escreve tipo novo, não só para quem conserta OOM.
3. **Fragmentação tem duas formas com ações opostas.** Poeira (muitos gaps pequenos) pede
   compactação dos consumers pequenos; poucos gaps grandes é benigno e compactar é
   desperdício. Por isso o histograma: sem ele, qualquer "ação anti-fragmentação" é tiro
   no escuro — inclusive a lane da s442, que escolhe pelo sintoma e julga pelo efeito medido.
4. **Um tipo de dado novo só existe quando sai do kernel.** O histograma entrou no
   snapshot, no slog e ganhou um leitor (`talc_frag_report.py`); dado que não tem quem o
   leia é decoração.

## 8. Pendências honestas

- **Sem QEMU nesta sessão.** Nenhum byte de TALC real foi medido; o histograma e o
  `TalcBuf` estão testados em host (sem TALC pronto, caem no híbrido) e ligados no build,
  mas a confirmação de que a janela de contexto está, de fato, no TALC em runtime
  (`[CTX] COMPACT` + queda de `hist[0]`) fica para o próximo lab.
- A queda do vazamento da janela **não foi quantificada**: o número honesto seria
  "bytes de bump retidos por N mensagens compactadas" antes/depois. É medição de lab.
- `reset_moe_cache`/`evict_kv` (s442) continuam sendo as ações da lane; o histograma agora
  diz a elas **qual** fragmentação estão vendo.

## 9. s444 — o histograma ganha o SEU consumidor: a lane anti-fragmentação

O §4 deste doc criou o histograma; a lane da s442 continuava decidindo só por
`free`/`largest`. Isso é um furo honesto: a lane pergunta "existe fragmentação?"
(regra da s435) e não "a fragmentação **é** deste tipo?". Duas consequências:

- Em fragmentação **benigna** (poucos gaps, todos grandes) a lane gastava um job
  LLM **e um HITL humano** para uma ação que não teria efeito — a pior versão da
  loop de feedback da s429-lab: trabalho que não devolve nada, e um clique pedido
  a alguém.
- Em fragmentação de **poeira** ela tratava como qualquer outra, sem dizer à IA que
  o que está em jogo é aloc médio falhando.

`FragKind` (`Dust` / `Benign` / `Mixed`, limiares 60% / 20% de gaps <8KB) fecha o
gap: benigno vira observe-only **antes** de chamar a IA; poeira segue para o LLM
com o histograma no snapshot; histograma ausente (log pré-s443) ⇒ `Mixed`, ou
seja, não afirma o que não mediu.

**Mutation 5/5** nos novos guards — e uma delas (M13, inverter o `>=`) reproduziu
uma corrupção real que tinha entrado no arquivo durante a própria bateria de
mutação da s442 e passado pelo `diff -q`. O jeito de blindar isso de forma durável: restaurar o
arquivo com **SHA-256** e reexecutar a suíte depois de qualquer bateria. Fixture de
teste que não distingue os limiares (M10 vivia com 67% de poeira, que é Dust tanto
com 60 quanto com 20) foi reforçada com casos de 60%/20%/30% exatos.
