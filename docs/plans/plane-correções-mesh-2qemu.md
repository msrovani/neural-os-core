# PLANO DE CORREÇÕES — Mesh 2 QEMU (6c/6GB) — Pós S454/S455

**Data:** 2026-10-05 · **Lane:** D5 (FREEBU) · **Base:** SESSION_454 + SESSION_455 + STATE.md
**Premissas basilares:** ADR-0088 (AIOS-first, HITL, auto-adaptação) + ADR-0113 (valor demonstrado/
complexidade, confiabilidade primeiro, HW real a qualquer custo) + AGENTS.md (zero hallucination,
busca ativa, registrar tudo).

---

## 0. Resumo executivo — o que os dois testes mostraram

Dois runs de 2×QEMU 6c/6GB com mesh P2P foram executados: S454 (antes do fix) e S455 (após fix
`-cpu Haswell`).

| Problema | S454 | S455 | Natureza |
|---|---|---|---|
| `-cpu max` mata boot (#GP PlatformPei) | ✅ PRESENTE | ✅ RESOLVIDO (Haswell) | Fix launcher |
| OOM → NULL+0x18 → #PF storm → park | ✅ T+6018 (ambos) | ❌ NÃO se repetiu | Classe: heap/TALCAR/alloc |
| matmul mesh >64KB drop silencioso | ✅ PRESENTE | ✅ PRESENTE (conhecido) | Wire format limit |
| gate depois do serialize (churn) | ✅ PRESENTE | ⚠️ HYPOTHESIS | Ordem 연산 |
| dual-truth peer (UI vs compute) | ✅ PRESENTE | ✅ PRESENTE | Incoerência de estado |
| parser monitor peers=0 falso-positivo | — | ✅ PRESENTE | Ferramenta |
| matmul barrier timeout boot | — | ✅ T+170/178 (não fatal) | WHPX 6c overhead |
| Hang no desligamento (sem log) | — | ✅ PRESENTE (último evento) | Shutdown path |

**Veredito honesto:** O sistema mesh **funciona** (S455: 27 min estáveis, peers=1, RX/TX, CRDT,
segurança OK, 0 PF/panic/OOM). Os problemas são: (1) um bug de launcher já resolvido; (2) uma
classe de falha de memória (OOM/park) que não se repetiu mas cuja causa raiz não foi isolada;
(3) limitações de wire/monitor/orqueamento que são conhecidas e documentadas.

---

## 1. PROBLEMAS DETECTADOS — classificação por severidade

### CRÍTICOS (bloqueiam confiabilidade/uso)

**C1 — OOM → #PF storm → park (S454, T+6018)**
- Sintoma: `OOM/TALC sem memoria Tier 1. size=2359307 agente=infer_worker` + `#PF storm park core`
- Causa raiz provável: growth de Vec no CellNetwork com heap esgotado (simbolização S454)
- Por que não se repetiu em S455: workload diferente (matmul que timeoutou não alocou 2.3MB)
- Risco: workload real pode reproduzir; o park fail-closed vira pause-spin sob WHPX (S454 P2)
- **Ação:** isolar causa raiz + gate de alocação + shutdown graceful

### MAJORES (degradam funcionalidade/medicão)

**M1 — Wire mesh limitado a 64KB (IDEA #654, S454 P3)**
- Causa: `FRAG_MAX_PARTS(64) × FRAG_MAX_CHUNK(1000) = 64000` com bitmask `[u8;8]`
- Sintoma: payloads reais 884KB-11.3MB são dropados silenciosamente
- Impacto: matmul mesh grande impossível; o slog culpa RAM (coincidência enganosa)
- **Ação:** expandir wire format ou documentar limit como constraint conhecido

**M2 — Shutdown sem log/hang (S455, desligamento)**
- Sintoma: logs paramam abruptamente sem evento de shutdown; QEMU congela
- Causa provável: caminho de shutdown não loga ou QEMU/guest congela antes de logar
- Impacto: impossível diagnosticar desligamentos; viola observabilidade (ADR-0113 §4)
- **Ação:** instrumentar caminho de shutdown com log obrigatório + graceful shutdown

**M3 — Parser monitor peers=0 falso-positivo (S455, IDEA #656)**
- Causa: parser pega último `peers=N` do log; canal FL emite `peers=0`
- Impacto: métrica de saúde do monitor é enganosa (mas o sistema está saudável)
- **Ação:** corrigir parser para priorizar MESH_HEALTH/P2P

### MENORES (comportamentais/otimização)

**m1 — Matmul barrier timeout no boot (S455, IDEA #657)**
- Não fatal; sistema continuou; não se repetiu em 27 min
- HYPOTHESIS: timeout 5s curto para WHPX 6c
- **Ação:** investigar se é self-test não-critico ou ajustar timeout

**m2 — Ordem gate-serialize (S454 P4)**
- HYPOTHESIS: churn de alocação antes do gate esgota heap
- **Ação:** validar hipótese + reordenar se confirmado

**m3 — Dual-truth peer (S454 P5)**
- UI diz peers=1, compute diz sem peer
- **Ação:** unificar fonte de verdade ou documentar a incoerência

---

## 2. PLANO DE CORREÇÕES — prioridade por ADR-0113

### Prioridade 1: Estabilidade do substrato (ADR-0113 §5, F1)

#### P1.1 — Isolamento do OOM/TALC (crítico)

**Problema:** OOM em T+6018 (S454) não se reproduziu em S455, mas a causa raiz não foi isolada.
Sem isolamento, é impossível garantir que não vai acontecer novamente com workload diferente.

**Plano:**
1. **Falsificador (ADR-0113 §8):** rodar S455 novamente com `mesh desligado` e verificar se
   OOM some. Se sumir → o mesh é a variável. Se persistir → é algo do boot/workload.
2. **Instrumentação do caminho OOM:** garantir que todo `alloc fail` loga:
   - agente que tentou alocar
   - tamanho pedido
   - headroom disponível no momento
   - fase cognitiva (observe/plan/act/verify/remember)
3. **Gate de alocação (ADR-0113 §6):** antes de alocar grande, verificar headroom; se crítico,
   recusar com log claro (não alocar e crashar).
4. **Shutdown graceful no park:** se um core vai parkar por OOM, tentar:
   - logar o evento com contexto completo
   - notificar os outros nós do mesh
   - tentar gracefully shutdown antes de parkar

**Critério de aceite:** OOM reproduzível com falsificador claro + gate que previne ou loga
com contexto completo + shutdown que não deixa core em loop infinito.

**Estado:** HYPOTHESIS (não isolado) → precisa falsificador + instrumentação.

---

#### P1.2 — Shutdown observável (crítico — M2)

**Problema:** Desligamento não loga; QEMU/guest congela sem registrar evento.

**Plano:**
1. **Identificar o caminho de shutdown:** onde está o código que responde ao desligamento?
   - Se é via botão do Jarbas (UI): traçar o caminho do clique → intent → shutdown
   - Se é via botão do GTK do QEMU: é outside do guest (não logável pelo kernel)
2. **Instrumentar shutdown com log obrigatório:** antes de qualquer ação de desligamento,
   logar: `SHUTDOWN_INITIATED reason=<...> agent=<...>`
3. **Graceful shutdown:** se possível, antes de desligar:
   - notificar mesh peers (`shutdown notice`)
   - flushar buffers críticos (bootlog, BOOT.LOG)
   - esperar operações em curso concluírem (bounded)
4. **Se shutdown for externo (QEMU kill):** aceitar que não há log do kernel; garantir que
   o BOOT.LOG do próximo boot registra que o boot anterior foi interrompido.

**Critério de aceite:** todo desligamento iniciado pelo sistema loga o evento + tenta notificar
peers + flusha BOOT.LOG. Desligamento externo resulta em boot seguinte com registro de interrupção.

**Estado:** DESCONHECIDO (caminho de shutdown não identificado ainda).

---

#### P1.3 — Wire mesh >64KB (maior — M1, IDEA #654)

**Problema:** Matmul mesh acima de 64KB é dropado silenciosamente. Limita compute distribuído.

**Plano:**
1. **Documentar como constraint conhecido** (urgent, baixo esforço): se não há previsão de usar
   matmul >64KB no mesh, documentar o limite e a razão no STATE/SESSION.
2. **Se há previsão de uso:** expandir o wire format:
   - Opção A: aumentar bitmask para `[u8;16]` (128 fragmentos × 1000 = 128KB)
   - Opção B: fragmentação em múltiplas mensagens com reassembly por ID
   - Opção C: limitar matmul mesh a 64KB e usar outro canal para >64KB
3. **Testar:** depois da mudança, validar com payload real >64KB (ex.: 1MB).

**Criterio de aceite:** payload >64KB é entregue corretamente (não dropado silenciosamente)
ou o limite está documentado como constraint aceito.

**Estado:** IDEIA #654 registrada; não implementada.

---

### Prioridade 2: Observabilidade (ADR-0113 §4)

#### P2.1 — Parser do monitor (maior — M3, IDEA #656)

**Plano:**
1. Corrigir `mesh_watch_30min.py` para priorizar `peers=N` do canal P2P/MESH_HEALTH:
   - Opção A: pegar o `peers=N` que vem de `[P2P]` ou `MESH_HEALTH`
   - Opção B: ignorar o canal `[nk] [FL]` na contagem de peers
2. Adicionar teste: simular log com MESH_HEALTH peers=1 e FL peers=0; verificar que o parser
   reporta 1.
3. Regenerar CSV/summary com parser correto paraValidar.

**Criterio de aceite:** parser reporta peers=1 quando MESH_HEALTH diz 1, mesmo com FL dizendo 0.

**Estado:** BUG IDENTIFICADO; não corrigido.

---

#### P2.2 — Monitor de 30 min mais robusto

**Problema:** O monitor terminou cedo ~3x durante S455; o CSV ficou com dados misturados de duas
execuções; o summary não foi regenerado.

**Plano:**
1. **Limpar CSV antes de cada run** (ou usar arquivo por execução com timestamp).
2. **Garantir que o monitor roda até o fim:** verificar se o término antecipado é por:
   - timeout do Python
   - exceção não tratada
   - kill externo
3. **Adicionar coluna de versão do parser** no CSV para rastrear mudanças.
4. **Validar após cada run:** confirmar que o summary foi gerado e reflete a execução corrente.

**Criterio de aceite:** run limpa → CSV apenas da execução atual → summary gerado e coerente.

**Estado:** PROBLEMAS OPERACIONAIS; não corrigidos.

---

### Prioridade 3: Confiabilidade do orqueamento (ADR-0113 §5)

#### P3.1 — Ordem gate-serialize (menor — m2, S454 P4)

**Problema:** HYPOTHESIS de que churn de alocação antes do gate esgota heap.

**Plano:**
1. **Validar hipótese:** adicionar log antes/depois do serialize e do gate; medir alocação
   real no caminho.
2. **Se confirmado:** reordenar: gate ANTES de alocar (verificar se cabe antes de copiar).
3. **Se não confirmado:** descartar hipótese e buscar outra causa.

**Criterio de aceite:** Hipótese validada ou refutada com evidência; se confirmada, ordem corrigida.

**Estado:** HYPOTHESIS; não validada.

---

#### P3.2 — Dual-truth peer (menor — m3, S454 P5)

**Problema:** UI (MESH_HEALTH) diz peers=1; compute (`online_nodes()`) diz sem peer.

**Plano:**
1. **Identificar as duas fontes:** onde cada uma lê o estado de peers?
2. **Se for incoerência real:** unificar a fonte (uma fonte de verdade).
3. **Se for visão diferente de módulos diferentes:** documentar que é esperado (cada módulo
   vê o que vê) e garantir que não causa decisão errada.

**Criterio de aceite:** ou unificado, ou documentado como incoerência aceita com justificativa.

**Estado:** OBSERVED; não investigado.

---

### Prioridade 4: Performance/melhoria (ADR-0113 §6, últimos)

#### P4.1 — Matmul barrier timeout (menor — m1, IDEA #657)

**Plano:**
1. **Identificar se é self-test não-critico:** se o matmul que timeoutou é parte do boot self-test
   e não é essencial, pode ser skipado ou dado mais timeout.
2. **Se é essencial:** investigar se timeout 5s é adequado para WHPX 6c; ajustar se necessário.
3. **Testar:** rodar S455 novamente e verificar se timeout se repete.

**Criterio de aceite:** se é self-test não-critico → skipado/documentado; se é essencial → timeout
adequado para WHPX 6c.

**Estado:** HYPOTHESIS; não investigado.

---

## 3. PLANO DE VALIDAÇÃO — como provar que as correções funcionam

### V1 — Reprodução controlada do OOM (S454)

**Objetivo:** reproduzir o OOM de T+6018 em condições controladas.

**Plano:**
1. Rodar S454 novamente (com `-cpu Haswell` fixo) e verificar se OOM se repete.
2. Se repete: instrumentar caminho OOM e isolar causa.
3. Se não repete: variar parâmetros (carga de mesh, modelos carregados) até reproduzir.

**Criterio:** OOM reproduzido com condições documentadas.

---

### V2 — Shutdown graceful documentado

**Objetivo:** provar que desligamento loga evento.

**Plano:**
1. Identificar caminho de shutdown no código.
2. Adicionar log obrigatório no início do shutdown.
3. Rodar teste: clicar no botão de desligamento → verificar log do evento.
4. Se não há caminho de shutdown no kernel: documentar que desligamento é externo (QEMU)
   e que o próximo boot registra interrupção.

**Criterio:** shutdown logado ou documentado como externo.

---

### V3 — Wire mesh >64KB testado

**Objetivo:** provar que payload >64KB funciona (ou limite documentado).

**Plano:**
1. Se expansão do wire: implementar + testar com payload 1MB real.
2. Se documento de limite: adicionar documentação clara no STATE/SESSION.

**Criterio:** payload >64KB entregue ou limite documentado.

---

## 4. ORDENAÇÃO RECOMENDADA

| Ordem | Ação | Porque primeiro |
|---|---|---|
| 1 | P2.1 — corrigir parser do monitor | Baixo esforço, imediato, melhora observabilidade |
| 2 | P1.2 — identificar caminho de shutdown | Crítico para diagnóstico; baixo esforço se for apenas identificar |
| 3 | P1.1 — falsificador do OOM | Necessário antes de investir em correção; pode revelar causa raiz |
| 4 | P1.3 — documentar limite wire 64KB ou expandir | Depende da previsão de uso; documentar é rápido |
| 5 | P2.2 — robustez do monitor | Melhora processo de teste; baixo esforço |
| 6 | P3.1 — validar hipótese gate-serialize | Depende de instrumentação do OOM |
| 7 | P3.2 — dual-truth peer | Baixo risco; pode ser documentado como incoerência |
| 8 | P4.1 — matmul barrier timeout | Não fatal; investigar se repete em futuro run |

---

## 5. RISCOS E INCERTEZAS

- **Causa do OOM não isolada:** pode ser interação complexa entre mesh, alocação, modelo carregado.
  Ainstrumentação é essencial antes de qualquer correção.
- **Shutdown sem caminho no kernel:** se o botão de desligamento for do GTK do QEMU (fora do guest),
  não há nada a fazer no kernel — apenas documentar.
- **Wire mesh >64KB:** expansão do wire format é mudança de protocolo; pode quebrar compatibilidade
  com nós existentes. Avaliar impacto antes de implementar.
- **WHPX vs metal:** todos os testes são em WHPX; comportamento no metal pode ser diferente.
  Honestidade: resultados QEMU não são resultados metal (ADR-0113).

---

## 6. LIVROS A BDA

- **ADR-0088:** premissa máxima AIOS-first — tudo começa com análise da premissa
- **ADR-0113:** missão otimização sistêmica — confiabilidade primeiro, valor demonstrado/
  complexidade, HW real a qualquer custo
- **SESSION_454:** detalhes do primeiro run (OOM, wire limit, gate, dual-truth)
- **SESSION_455:** detalhes do segundo run (estável, parser bug, matmul timeout, shutdown hang)
- **IDEA #654:** wire limit 64KB
- **IDEA #656:** parser bug peers=0
- **IDEA #657:** matmul barrier timeout
- **GOVERNANCE.md:** ciclo IDEA → ADR → sprint → TODO → implementação → SESSION → check

---

## 7. PRÓXIMOS PASSOS IMEDIATOS

1. **Ler o código do shutdown** para identificar se há caminho no kernel (P1.2).
2. **Corrigir parser do monitor** (P2.1) — baixo esforço, imediato.
3. **Limpar CSV do monitor** e rodar S455 novamente por 30 min com parser corrigido (P2.2).
4. **Documentar limite wire 64KB** no STATE (P1.3 documentação).
5. **Aguardar próximo run** para ver se matmul barrier timeout se repete (P4.1 observação).

---

**Honestidade:** Este plano é baseado em evidências dos logs S454/S455 + documentação do projeto.
As correções são hipóteses para a maioria dos problemas (exceto o parser do monitor, que é um bug
identificado). Nenhuma correção será implementada sem falsificador ou validação prévia, conforme
ADR-0113.
