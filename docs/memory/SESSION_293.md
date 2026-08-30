# SESSION_293 — Card de seleção de disco (install) sai da casca

**Objetivo:** o card 7902 do instalador aparece quando o usuário digita `install` no HermesChat.

## Problema

Usuário rodou `/install` (ou `install` bare) no HermesChat → card nunca apareceu. Trace:

1. `crates/neural-kernel/src/shell.rs::execute("install")` publicava `TOPIC_SYS_INSTALL_UI` corretamente —
   **mas não era chamado de lugar nenhum.** HermesChat envia `USER_INTENT` → `HermesAgent::user_receiver`
   → `hermes::parse_command(text)` → `Command::Chat("install")` (variante fallback, linha 744
   em hermes.rs) → vai pro LLM. Nada publishava o evento.
2. `crates/hermes/src/shell.rs::execute("install")` (handler duplicado) era **stub quebrado**:
   só chamava `hw_profiler::profile_hardware()` e imprimia texto; nunca publicava
   `SYS_INSTALL_UI`. Esse handler também nunca é invocado pelo input do HermesChat.
3. `Command::Install` não existia no enum; nenhum Command match arm tratava install.

Resultado: `DisplayAgent::tick()` rodava o `try_receive` no `install_ui_receiver` (lazy
subscribe), o receiver estava vazio, nada spawnado, card nunca aparecia.

## Fix

- **`hermes::hermes.rs`:**
  - enum `Command` ganha variante `Install`.
  - `parse_command` reconhece `/install`, `/instalar` (entre `model` e `fetch`); fallback bare
    `install`/`install <x>` antes do `Command::Chat` final.
- **`hermes::agents.rs`:**
  - intent_name match: novo braço `hermes::Command::Install => "Install"`.
  - Dispatch (linha ~903): publica `TOPIC_SYS_INSTALL_UI` no EventBus (mesmo `payload: Vec::new()`
    do shell.rs) + retorna `"Install: selecione o disco de destino na UI\n"` no chat.
- **`hermes::shell.rs::execute` install handler:**
  - Era um stub que só imprimia `format!("install: run via AutoInstallerAgent...")`.
  - Agora publica o mesmo `TOPIC_SYS_INSTALL_UI` (parity com `neural-kernel::shell.rs`).
  - Mantém o gate `INSTALLER_BUSY` (recusa se uma instalação já está rodando).

## Fluxo final

```
HermesChat input "/install" (ou "install")
  → HermesAgent user_receiver
  → parse_command → Command::Install
  → dispatch: publish(SYS_INSTALL_UI) + chat reply
  → DisplayAgent::tick() lazy-subscriber drains → spawn_card(disk_selection_card()) [card 7902]
  → User clicks disk button
  → handle_card_button(7902, btn_idx) → button_index_to_disk_index(btn_idx)
  → DISK_SELECTION.store(idx) + publish(SYS_INSTALL)
  → AutoInstallerAgent::run_install_from_bus (do SESSION_292)
  → read_kernel_from_boot (ATA→USB fallback + guard target≠source)
  → SysInstaller::install → gpt dual ESP+NeuralFS no target
```

## Verificação

- `cargo check --release`: **0 erros** (1m 59s).
- Não rodou testes de host (mudança é só hermes enum + 2 match arms; host tests do hermes
  usam o enum via `parse_command`, então passarão).

## Residual

- Concorrência no receiver: `DisplayAgent` é Continuous e o receiver é lazy — primeira tick
  após `install_ui_receiver = None` cria o `Receiver`. Se o `Command::Install` dispatch
  ocorre **antes** da primeira tick do DisplayAgent, evento perdido. Mitigado por DisplayAgent
  rodar Continuous desde Phase 6 (antes de qualquer input do usuário no Phase 7/Runtime).
- Limite residual: clique no card usa `take_card_hit_button` → handle_card_button; fora
  do ramo `7902` o handler é `_ => {}` (silencioso). OK para este caso.

## Nota de processo

Sessão concorrente ativa durante o trabalho (jarbas/compositor + fb + soul_mirror + run-qemu-4c-loop.ps1).
Commit cirúrgico: só `crates/hermes/src/{hermes,agents,shell}.rs` + docs.