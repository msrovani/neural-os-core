# Governança documental

Este documento define o ciclo canônico que mantém intenção, decisão, execução e evidência sincronizadas.

## Ciclo obrigatório

```text
IDEA_BANK → ADR temática → plano de sprint → TODO → implementação + STATE
          → SESSION → check final (IDEA + ADR + STATE)
```

1. **IDEA_BANK:** registrar toda ideia com status, destino e evidência conhecida.
2. **ADR temática:** agrupar ideias relacionadas que impliquem decisão arquitetural, stack, anel ou contrato público.
3. **Plano de sprint:** detalhar escopo, ordem, critérios de aceite e riscos. Planos concluídos vão para `docs/archive/sprints/`.
   - **Planos Cursor** (implementação): ao fechar, registrar seção **“Planos Cursor implementados”** na ADR temática + linha na tabela `INDEX.md` → “Planos Cursor → ADR”. Não versionar `.cursor/plans` no git.
4. **TODO:** transformar o plano aprovado em checklist executável, com referências a IDEA/ADR.
5. **Implementação + STATE:** implementar e atualizar `docs/memory/STATE.md` somente com a verdade operacional atual.
6. **SESSION:** registrar decisões, evidências, comandos de verificação, limites e handoff.
7. **Check final:** sincronizar status da IDEA, lifecycle no `docs/architecture/INDEX.md`, STATE, TODO e evidência da SESSION.

## Regra A — ADR por tema

Não existe relação obrigatória `1 ideia = 1 ADR`.

- Decisão arquitetural, anel, stack ou contrato público: exige ADR própria ou ADR temática existente.
- Features correlatas: agrupam-se na ADR do tema e apontam para ela.
- Fix ou polish pontual: pode seguir por `TODO + SESSION`, com `ADR = — (fix pontual)`.
- Ideia descartada ou fundida: recebe justificativa no IDEA_BANK; ADR só é necessária se uma decisão anterior for substituída.

Uma ADR não deve ser criada apenas para preencher uma lacuna de tabela. Primeiro deve existir uma decisão arquitetural real.

## Campos cruzados mínimos

### IDEA_BANK

```text
ID | ideia | status | ADR (ou — justificado) | sprint | evidência
```

### ADR e INDEX

```text
ID/arquivo | Status canônico | lifecycle | ideias cobertas
substitui | substituída por | sprint | evidência
```

O status no corpo da ADR é `Proposed | Accepted | Rejected | Superseded`. O lifecycle operacional fica no `architecture/INDEX.md`.

### TODO

```text
item | IDEA/ADR | critério de aceite | status
```

### STATE

Somente versão vigente, pista ativa, fatos verificados, residuals e próximo passo. História pertence a SESSION, CHANGELOG ou archive.

### SESSION

```text
objetivo | mudanças | evidência | limites | IDEA/ADR tocadas | próximo
```

## Lifecycle de ADR

Os valores válidos são `por_fazer`, `fazendo`, `completa`, `modernizada`, `substituida`, `obsoleta`, `pesquisa`, `conflito_id` e `plano_sprint`. Definições e inventário canônico ficam em `docs/architecture/INDEX.md`.

## Regras de preservação

- Não renumerar ADR conflitante sem migração aprovada; documentar o conflito no INDEX.
- Não apagar história; usar banner de superseded/obsoleto e preservar o corpo.
- Não editar documentos arquivados para simular o estado atual.
- Não declarar release apenas porque critérios parciais foram atendidos; conferir o gate vigente no STATE e na ADR.

## Checklist de encerramento

- [ ] IDEA atualizada com ADR temática ou `—` justificado.
- [ ] ADR e lifecycle coerentes com a implementação.
- [ ] TODO concluído ou residual explicitamente replanejado.
- [ ] STATE contém apenas a verdade operacional atual.
- [ ] SESSION contém evidência e limitações.
- [ ] Links apontam para caminhos existentes.
- [ ] Verificação técnica adequada ao risco foi executada e registrada.
