# SESSION_442 — Anti-fragmentação do TALC: IA-observa → IA-age (vocabulário fechado + HITL + efeito medido)

**Data:** 2026-10-03 · **Versão:** v1.9.99-s442 · **Branch:** `main` · **Crate:** `hermes` (+ seam em `cortex`)

## 1. O elo que faltava

A s435 deu à IA o **número** do TALC (`free`/`largest`/`gaps`/`partial`) e a s433 deu o
**canal LLM**. O que não existia era o meio do ciclo:

- Fragmentação era `Observe` (decisão certa pela lição s429-lab — escalar sintoma vira
  loop de feedback), mas **não existia caminho para virar ação**;
- A "ação" que o LLM produzia (`{"title":..,"action":..}`) era **texto livre** — rótulo
  de toast. Ela nunca chegava a nenhum executor. A IA observava e *descrevia*.

Ou seja: o sistema sabia o número, sabia pedir opinião, e mesmo assim **nunca agia**.
`s442` fecha esse elo sem abrir mão das duas lições que o proibiam:
**IA decide** (ADR-0088) e **humano aprova** (HITL).

## 2. Vocabulário FECHADO (`AntiFragCmd`)

O LLM **escolhe uma chave** de um catálogo; ele nunca escreve o comando. Texto livre é
apenas o rótulo humano do toast. Cada variante tem seam real e efeito verificável:

| cmd | seam real | efeito |
|---|---|---|
| `evict_kv` (args `recent`/`heavy`) | `cortex::cortex::kv_h2o_evict_global` (**novo s442**) | H2O no KV global: mantém recent+heavy, devolve o resto ao TALC |
| `drop_kv` | `cortex::cortex::kv_cache_reset` | descarta o KV global inteiro (páginas TALC voltam) |
| `reset_moe_cache` | `cortex::global_arena::reset_moe_cache` | libera experts MoE da arena |
| `no_action` | — | declínio honesto ("não há ação segura agora") |

**Propriedade de segurança (com teste e mutation própria):** não existe caminho de código
que transforme texto do modelo em comando. `"action":"..."` ou `"cmd":"rm_rf"` caem em
`Fallback` e contam contador — não executam nada.

**Honestidade do executor:** cada `apply()` devolve o que aconteceu e loga o porquê.
`evict_kv` com o KV global vazio **ou em voo** (`global_kv_cache_take` pegou a posse) é
`no-op honesto` com slog `warn` — a lane nunca rouba o cache de um prefill/decode.

## 3. HITL: nada executa sem humano

O comando escolhido abre um pedido `Escalate` no `ApprovalGate` que já existia
(`/approve <id>` / `/deny <id>` no CLI, overlay Jarbas). Cadeia:

```
fragmentação (free>=256MB && largest*4<free)
   └─ LLM escolhe cmd ── HITL Escalate ── /approve ── apply(seam real)
                                                    └─ verify(before vs after) ── log do efeito
```

- `/deny` → nada executa (`Step::Denied`).
- Sem resposta em 10 min (`ANTIFRAG_HITL_TTL_TICKS`) → pedido **esquecido**: aprovar
  depois age sobre um snapshot velho, que é pior que não agir.
- `ApprovalGate` é adquirido com `try_lock` **com limite de 64 tentativas** — nunca
  `lock()` atravessando publicação no bus (é exatamente como o s438 morreu).
- A amostra do TALC é colhida **antes** de agir: sem isso a "antes" já é o "depois" e a
  medição vira teatro.

## 4. O ciclo fecha com número, não com promessa

`verify(before, after)` julga o efeito: só é `Improved` se o **maior gap** cresceu (free
subindo com `largest` parado é ruído de amostra); `Worse` se o **uso** subiu; `NoChange`
caso contrário — e amostra `partial=1` não afirma nada.

Antes disso, um **delay de 3 s** (`ANTIFRAG_MEASURE_DELAY_TICKS`): `talc_refresh_usage`
só corre a 2 Hz, então o pump seguinte leria **a mesma amostra** que colhemos antes de
agir — e o veredito "sem efeito" seria um falso negativo fabricado. Pior: o cooldown de
5 min impediria uma segunda medição boa. Medir cedo é mentir com passo curto. Depois de agir (com ou sem
efeito) a lane **cala 5 min** (`ANTIFRAG_QUIET_TICKS`): hipótese errada não vira
metralhadora de `drop_kv`.

## 5. Gates que mantêm a lane honesta

`decide()` só age com: TALC com claim, **amostra não-parcial**, fragmentado pela **mesma
régua da s435** (`is_fragmented`, uma só verdade), modelo carregado e headroom ok. Sem
modelo ou sem headroom: fragmentação **segue observe-only** — a lane anti-fragmentação é
IA+HITL, não heurística automática.

## 6. Mutation testing 8/8

| mutação | teste que pegou |
|---|---|
| texto livre vira `Cmd(DropKv)` | `texto_livre_do_llm_nao_vira_cmd` |
| `deny` executa mesmo assim | `denied_nao_executa` |
| TTL do HITL removido | `hitl_expirado_e_ignorado` |
| cooldown pós-efeito removido | `pos_efeito_entra_em_cooldown_e_verifica_uma_vez` |
| `largest` parado vira "melhora" | `verify_nao_chama_de_melhora_de_largest_parado` |
| clamp dos args removido | `args_fora_da_faixa_sao_clampados` |
| amostra parcial decide assim mesmo | `amostra_parcial_nunca_decide` |
| delay de medicao removido (mede a amostra com ela mesma) | `medir_antes_do_delay_queimaria_a_unica_verificacao` |

## 7. Validação

- `cargo test -p hermes --lib anti_frag -- --test-threads=1` → **21/21** (10 de decisão/estado + 11 de parsing/segurança)
- `cargo test -p hermes --lib -- --test-threads=1` → **309 passed, 1 failed** — a falha é a
  **pré-existente** de `permission_gate::tests::test_risk_level_classify` (order-dependence
  de statics, registrada na s440); isolada passa.
- `cargo test -p cortex --lib -- --test-threads=1` → **126/126**
- `touch crates/neural-kernel/src/main.rs && cargo nk` → **0 erros** (41.9 s)
- Evidência de link (regra 419): `fragmentacao detectada`, `evict_kv` e `hub.antifrag`
  presentes em `target/x86_64-unknown-none/release/neural-kernel`.

## 8. Lições

1. **"A IA propõe texto" não é ciclo fechado.** O gargalo da autonomia nesta sessão não era a
  telemetria nem o canal — era o **meio**: nenhum texto de LLM tinha onde virar efeito.
   Ao elencar "quem executa, com que autoridade, sobre qual dado", o furo apareceu.
2. **Vocabulário fechado é o que torna HITL aplicável.** Um gate de aprovação sobre texto
   livre é teatro (o humano aprova uma string que o kernel executa por tabela ou não
   executa). Enum + catálogo no prompt = o humano aprova o **mesmo** comando que roda.
3. **Medir o efeito fecha o loop de verdade.** Sem `verify`, "IA age" é um unknown de
   runtime; com ele, a hipótese fica refutável (`nochange`/`worse` é um resultado legítimo).
4. **Julgar antes da amostra nova é fabricar resultado.** Todo sensor com cache tem um
   período; medir dentro dele compara o valor com ele mesmo. O delay de 3 s não é
   folga — é parte da definição de "medição honesta".
5. **Amostra degradada não é informação, é ruído** — `partial=1` (s441) é justamente o
   caso em que a lane precisa **não** decidir.
6. **`try_lock` com limite, sempre que o lock atravessa publicação** — herança direta do
   freeze do s438 (TicketLock não-reentrante + IRQ).

## 9. Pendências honestas

- **Sem validação em QEMU desta sessão**: a lane precisa de um boot com TALC de fato
  fragmentado (contexto longo) para ver `fragmentacao detectada` → `HITL #` → `/approve`
  → `efeito:` no log. Código e testes existem; a prova de runtime é a próxima.
- `reset_moe_cache` atua na **arena**, não no span do TALC — entra no catálogo como
  "libera memória de longa vida", mas seu efeito na fragmentação do TALC é indireto
  (por isso a escolha de cmd é da IA e o efeito é medido, não presumido).
- `evict_kv` realoca as páginas restantes (dequantiza/requantiza): o ganho é líquido em
  contexto longo e **nulo** quando `recent+heavy ≈ len` — razão de o efeito ser medido.