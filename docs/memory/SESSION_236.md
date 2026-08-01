# SESSION_236 — Codemap: index completo do repositório (66 mapas + atlas raiz)

**Data:** 2026-08-01
**Objetivo:** Indexar a base inteira com o skill `codemap` — gerar hierarquia de `codemap.md` por crate/submódulo + atlas raiz, para agentes e humanos navegarem o código sem re-leitura total.

## Resumo

| Etapa | O que foi feito | Resultado |
|---|---|---|
| Init | `codemap.mjs init` com includes `crates/**/*.rs` + `tools/**/*.py` | 738 arquivos selecionados, 67 diretórios, `.slim/codemap.json` (estado de change-detection) |
| Lanes | 8 fixers paralelos (1 por crate/tree, escopos de escrita disjuntos) | 66 `codemap.md` escritos a partir do código real (símbolos verificados por grep) |
| Atlas | Orquestrador montou `codemap.md` raiz (responsabilidade, entry points, tabela 13 linhas, anéis, comandos de refresh) | Root Atlas criado |
| AGENTS.md | Seção `## Repository Map` adicionada (idempotente) | linha 414 |

## Lições / descobertas do index (drifts docs vs código real)

1. **`probe_uefi_framebuffer` não existe mais** — removido na migração Limine (SESSION_232). Atual é `display::fb::probe_raw_framebuffer` (chamado de `limine_boot.rs:155`); bpp/stride vêm de `resolve_bytes_per_pixel` (3\|4, fallback 4). ADR-0045 note sobre probe está stale.
2. **jarbas/audio agora é fonte única** — `neural-kernel/src/audio/mod.rs` é só `pub use jarbas_crate::audio::*;` (refactor E4 emagrecer). A nota "truth=neural-kernel/src/audio" do AGENTS.md/ADR-0045 está stale — a verdade é jarbas.
3. **neural-kernel `fs/`, `vfs/`, `neural_fs/` locais = espelhos legados NÃO compilados** — o crate compilado usa `pub use hermes_crate::{fs,vfs,neural_fs}`. Documentado honestamente nos codemaps (não inventar relação de compile falsa).
4. **Drift AGENTS.md:** `tools/update_tecnologias.py` não existe; `migrate_k2chj.py` arquivado em `docs/archive/migration/`; "bios.img + bootloader 0.11 kernel_main" stale — boot crate produz só `uefi.img` via Limine (SESSION_232).
5. **`MpmcQueue`** (não `MpmcChannel`) — correção pós-write do fixer k_nano.
6. **PowerShell não expande `~`** para node — usar caminho completo do script: `C:\Users\msrov\.config\opencode\skills\codemap\scripts\codemap.mjs`.

## Verificação

- `cargo check --release -p neural-kernel`: **0 erros** (29 warnings conhecidos).
- Zero placeholders, zero codemap.md vazio (check `<100 bytes` = 0).
- `.slim/codemap.json` snapshot: 739 files.

## Next

- Incremental: após edições, `codemap.mjs changes --root ./` → atualizar só folders afetados → `codemap.mjs update`.
- Arrumar drifts acima (probe_uefi_framebuffer ref, AGENTS.md TECNOLOGIAS script) quando tocar nesses arquivos.
