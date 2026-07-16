# docs/archive — Documentação histórica

**Propósito:** preservar sessões, planos de sprint e notas já concluídos ou supersedidos, sem poluir as fontes vivas (`docs/memory/`, `docs/architecture/`).

**Política:**
- Conteúdo **não é apagado** — só movido para cá.
- Cada arquivo arquivado deve ter banner `Histórico/Superseded` no topo (origem + fonte atual).
- Sessões **≥107** ficam em `docs/memory/`; sessões anteriores a 107 ficam em `sessions/`.
- ADRs **não** entram neste archive — permanecem em `docs/architecture/` (mesmo superseded).
- Ciclo documental: ver [`docs/GOVERNANCE.md`](../GOVERNANCE.md).

## Layout

| Pasta | Conteúdo |
|-------|----------|
| `sessions/` | `SESSION_*.md` anteriores à política ≥107 |
| `sprints/` | Planos de sprint concluídos (`sprint-plan-*`, `SPRINT-106*`) |
| `notes/` | Notas/opinião (ex.: Rodamap) |

## Banner template

```markdown
> **Histórico/Superseded** — Arquivado em YYYY-MM-DD.
> Origem: `docs/...`
> Fonte atual: `docs/memory/STATE.md` / ADR-XXXX / `docs/GOVERNANCE.md`
```
