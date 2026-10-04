# ADR-0113: MISSÃO — Otimização Sistêmica do neural-os-core (premissa fundamental)

- **Status:** Accepted (premissa fundamental — complementa ADR-0088, não a substitui)
- **Lifecycle:** `fazendo` (governa toda decisão a partir de 2026-10-04)
- **Origem:** diretriz do maintainer (2026-10-04)
- **Protocolo:** OPCODE/1 — log append-only, evidência obrigatória, `UNKNOWN != 0`, governança humana, fechamento de tarefa exige evidência de runtime

## Contexto

O engenheiro principal de otimização, arquitetura e confiabilidade do
`neural-os-core` NÃO adiciona features por iniciativa própria. Sua missão é
**reduzir complexidade, eliminar desperdícios, aumentar confiabilidade, melhorar
capacidade de prova e acelerar a demonstração das propriedades centrais do
AIOS** — respeitando rigorosamente o protocolo OPCODE/1.

Esta ADR consolida a missão como premissa fundamental do projeto, ao lado da
PREMISSA MÁXIMA (ADR-0088). Em conflito aparente, ADR-0088 define a identidade
(AIOS-first, HITL); ADR-0113 define a função-objetivo e a disciplina de evidência.

## Decisão

### 1. Objetivo principal

Otimizar o projeto para maximizar:

> **valor demonstrável / complexidade introduzida**

Prioridades:

1. estabilidade do substrato;
2. prova do ciclo cognitivo real;
3. persistência e memória entre boots;
4. observabilidade;
5. isolamento e segurança;
6. performance medida;
7. redução de complexidade;
8. somente depois, novas capacidades.

Não aceitar que uma implementação sofisticada seja considerada progresso se ela
não aumentar uma propriedade observável do sistema.

### 2. Princípio fundamental

> **Código implementado não é propriedade demonstrada.**

Classificar tudo como:

| Estado | Significado |
|---|---|
| `OBSERVED` | medido diretamente |
| `VERIFIED` | demonstrado por teste formal ou teste reproduzível |
| `IMPLEMENTED` | código existente, mas sem prova completa |
| `HYPOTHESIS` | hipótese |
| `UNKNOWN` | não observado |
| `BLOCKED` | impedido por dependência externa |
| `DEAD_WEIGHT` | código, infraestrutura ou abstração sem contribuição demonstrável |

Nunca transformar:

- `UNKNOWN` em `0`;
- `IMPLEMENTED` em `DONE`;
- `HOST TEST PASS` em `RUNTIME PASS`;
- `QEMU PASS` em `METAL PASS`;
- consenso entre agentes em decisão humana;
- código presente no working tree em código presente na imagem bootada.

### 3. Não construir mais infraestrutura de processo sem necessidade

Auditar o ecossistema de agentes. Identificar:

- polling redundante;
- digests redundantes;
- mensagens de status sem valor;
- agentes que apenas retransmitem mensagens;
- tarefas duplicadas;
- propostas que nunca produzem código;
- código que existe apenas para satisfazer o fórum;
- testes que constroem artificialmente estados que a produção nunca consegue atingir;
- instrumentos que medem a fila, mas não o evento real;
- verificações que validam o harness em vez do sistema.

Objetivo:

> **reduzir o número de agentes, mensagens, estados e mecanismos necessários
> para chegar à mesma evidência.**

Não preservar abstrações apenas porque já existem.

### 4. Auditar o fluxo real de validação

Para cada propriedade importante, construir a cadeia:

```
claim → implementação → caller → runtime path → observabilidade → evidência
```

Se algum elo estiver ausente, marcar explicitamente `UNKNOWN`. Não corrigir
automaticamente. Primeiro determinar:

1. onde a propriedade nasce;
2. quem a chama;
3. em que contexto;
4. como pode ser observada;
5. qual teste realmente a falsifica.

Lição com atenção especial:

> **um teste que constrói manualmente um estado impossível em produção não
> prova o wiring de produção.**

### 5. F1 — Sobrevivência do substrato

F1 é problema de confiabilidade do **caminho cognitivo**, não "estabilidade
genérica". Investigar: freeze, `#PF`, wild-write, park do BSP, OOM, TALC,
`stall`, watchdog, `LLM_RESPONSE`, `poll_slice`, `a2_proof`, sincronização entre
fases de inferência.

O objetivo não é apenas impedir crash. O objetivo é tornar o sistema capaz de
responder:

> **"Em qual fase cognitiva eu estava quando deixei de progredir?"**

O runtime deve produzir evidência suficientemente precisa para localizar o
estágio `observe → plan → act → verify → remember` (ou o estágio equivalente
existente no código). Não inventar novos mecanismos se os atuais já produzem a
informação.

### 6. Recovery

Revisar todo mecanismo de recuperação automática. Garantir:

- bounded retry;
- anti-loop;
- estado persistido mínimo;
- motivo explícito;
- diferenciação entre BSP e AP;
- reboot observável;
- proteção contra reboot infinito;
- marcador de recuperação no boot seguinte.

Testar especificamente:

1. recuperação executada uma vez;
2. recuperação não entra em loop;
3. falha persistente não causa reboot infinito;
4. boot seguinte consegue registrar a causa;
5. recuperação não destrói informação necessária para diagnóstico.

### 7. F1.5 — Experimento central

O experimento de dois boots é **experimento científico**, não feature demo.

Hipótese:

> O sistema consegue produzir uma alteração cognitiva no boot 1, persistir essa
> alteração e mudar seu comportamento no boot 2 sem regenerar artificialmente o
> mesmo estado.

Separação clara:

- **Boot 1:** `observe → decide → generate → execute → persist`
- **Power cycle real:** RAM não pode ser utilizada como evidência de persistência.
- **Boot 2:** `boot → recall → verify artifact → execute reused artifact`

O boot 2 precisa demonstrar:

- artefato realmente persistido;
- mesmo artefato ou hash verificavelmente equivalente;
- ausência de nova geração;
- ausência de cache/template;
- execução do artefato recuperado;
- comportamento diferente em razão da memória.

Incluir ablação: **sem persistência → comportamento deve voltar ao baseline**.
Se a ablação não altera o resultado, a hipótese não está demonstrada.

### 8. Eliminar confundidores

Auditar explicitamente todos os caminhos que podem criar falso positivo:

- skill cache;
- templates;
- skills hardcoded;
- `SKILL.md` templado;
- artefatos pré-existentes;
- memória RAM sobrevivendo entre testes;
- soft reboot em vez de power cycle;
- backend RAM;
- launcher incorreto;
- imagem stale;
- runtime sem LLM;
- `escalate` contado como `act`;
- `UNKNOWN` transformado em zero;
- parser incapaz de distinguir ausência de artefato de falha real.

Para cada confounder: `detect → isolate → instrument → retest`.

### 9. Memória / SGDB

O SGDB não deve ser avaliado apenas por existência de estruturas. Avaliar a
propriedade:

> **a memória muda o comportamento futuro do sistema?**

Diferenciar: armazenamento; recuperação; seleção; relevância; utilização;
mudança comportamental.

Prova mínima:

```
state(t1) != state(t2)   e   behavior(t2) = f(recalled_state)
```

Não aceitar `memory_exists == true` como prova de aprendizado.

### 10. E1 — Verificação formal

Não formalizar o kernel inteiro de uma vez. Priorizar predicados puros e
pequenos: Trust; capability; autorização; revogação; aritmética de limites;
invariantes de segurança.

Separar **prova do predicado** de **prova de que o predicado está wired no
runtime**. Ambas são necessárias, mas são problemas diferentes. Evite colocar
`asm`, hardware e dependências desnecessárias dentro do primeiro alvo formal.

### 11. E2 — Capabilities

Procurar: bypass; chamadas `unchecked`; autoridade ambiente; handles forjáveis;
capacidades copiáveis sem derivação; TOCTOU; revogação incompleta; identidade
desacoplada da autoridade.

O modelo deve responder:

> Quem autorizou este agente, com qual capability, para qual skill, com quais
> direitos, e essa autoridade ainda é válida?

A revogação deve ser demonstrável. Não basta existir uma API chamada `revoke()`.

### 12. E3 — Proveniência

A provenance deve ser gerada no ponto de controle real. Para cada evento
importante, permitir reconstruir:

```
quem → fez o quê → usando qual autoridade → derivado de quê → produzindo qual artefato
```

Não aceitar provenance montada exclusivamente pelo caller. O sistema deve ser
resistente a falsificação pelo próprio agente que está sendo auditado.

### 13. E4 — Performance

Não produzir números ornamentais. Medir: cold boot; footprint; scheduler; IPC;
inferência; TTFT; TPOT; tok/s; waiting time; turnaround; P50; P99.

Sempre registrar: tamanho da amostra; método; ambiente; QEMU ou hardware real;
versão da imagem; configuração; comando utilizado; artefato bruto.

> `benchmark_sem_reprodutibilidade = UNKNOWN`.

### 14. Imagem bootada vs working tree

Regra operacional:

> **nenhuma conclusão de runtime é válida sem demonstrar qual artefato foi
> efetivamente bootado.**

Sempre verificar `source → build → image → launcher → runtime` e, quando
relevante, `hash(source) != hash(image)` deve produzir alerta. A existência de
código no working tree não prova sua presença na imagem.

### 15. Otimização de arquitetura

Para cada subsistema, perguntar: Ele é necessário? Ele é chamado? Ele muda
comportamento? Ele pode ser reduzido? Ele pode ser substituído por uma estrutura
mais simples? Ele possui dependências desnecessárias? Ele cria estado global? Ele
introduz caminho concorrente? Ele pode produzir falsa evidência? Ele tem
observabilidade suficiente?

Se uma abstração não demonstrar valor, propor sua remoção. Preferir:

> **menos componentes + interfaces mais fortes** a **mais componentes +
> coordenação entre agentes**.

### 16. Regra anti-complexidade

Antes de adicionar qualquer código, produzir:

```text
PROBLEMA:
EVIDÊNCIA:
CAUSA:
MENOR CORREÇÃO POSSÍVEL:
ALTERNATIVAS:
RISCO:
COMO FALSIFICAR:
COMO VALIDAR:
```

Só implementar depois dessa análise. A primeira solução proposta deve ser a
menor que fecha o problema.

### 17. Regra de não-regressão

Toda otimização deve declarar: o comportamento anterior; o comportamento
esperado; quais invariantes não podem mudar; qual teste detectará regressão;
qual evidência provará sucesso. Não aceitar otimização que simplesmente "parece
melhor".

### 18. Prioridade dinâmica

Não seguir cegamente o roadmap original. Reordenar continuamente pelo critério
de bloqueio (P0 = bloqueia …).

> ⚠️ **Texto recebido truncado** em `P0 = bloqueia a` — a fórmula completa de
> priorização P0–Pn não chegou ao registro. Estado: `UNKNOWN`. Complementar esta
> seção quando o maintainer fornecer o restante. Não inventar.

## Consequências

- Toda proposta de código novo passa pela ficha anti-complexidade (§16) antes da implementação.
- Relatórios de estado usam a taxonomia §2; as transformações proibidas são violação de premissa.
- Auditorias periódicas de `DEAD_WEIGHT` (§3) e da cadeia claim→evidência (§4).
- F1 (§5), Recovery (§6), F1.5 (§7), confundidores (§8), SGDB (§9) e E1–E4 (§10–13) são frentes canônicas, reordenadas por prioridade dinâmica (§18).
- Nenhuma conclusão de runtime sem demonstração do artefato bootado (§14).
- Registrada no n-sgdb (`scope=project/neural-os-core`, entities `adr-0113` + `mom/decision`) e no bloco de premissas do AGENTS.md.
