# Dead tools — ferramentas de host arquivadas (missão §3)

Nada aqui é executado pelo build nem pelo runtime do OS. São scripts de host
(`tools/`) sem callers, one-shot já executados, ou destrutivos. Movidos com
`git mv` (não deletados) para limpar `tools/` sem perder a intenção.

## Por que estão aqui

| Ferramenta | Motivo do arquivamento |
|---|---|
| `forum_repair_ids.py` | **One-shot + destrutivo.** Reescrevia o log inteiro do fórum (mode `"w"`, sob `write_lock`) para reparar 7 linhas `FREEBU-0001` duplicadas — incidente histórico já corrigido. Aborta nos dados atuais (esperava 7 linhas, não encontra). Zero callers. |

**Recuperar:** `git mv docs/archive/dead-tools/<file>.py tools/<file>.py`.

## NÃO arquivadas (avaliadas e mantidas — decisão consciente)

| Ferramenta | Motivo de manter |
|---|---|
| `tools/forum_read.py` | **NÃO é redundante.** Usa cursor PRÓPRIO (`target/freebu_read.cursor`), separado do `forum.cursor` que `forum_post.py --poll` e `forum_watch.py` compartilham. Removê-lo forçaria a leitura manual a competir pelo mesmo cursor do watcher (pularia mensagens). A §3 chamou de "redundante"; a checagem do cursor refuta. |
| `tools/forum_post.py`, `forum_lock.py`, `forum_watch.py`, `test_forum_lock.py` | Infra viva do fórum OPCODE/1 (lock ao lado do log, unicidade de id, watchdog). |

> Nota: `tools/forum_freebu_intro.py` é local e está no `.gitignore` (não rastreado).
