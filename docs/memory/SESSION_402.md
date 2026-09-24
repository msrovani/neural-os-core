# SESSION_402 — Self-deadlocks no path de slog + EMIT_GUARD + mesh Master/Memory + mitigações

**Motivo:** boot 8G/8c congelava pós-`inference` (sem fase 3/6/7); depois no grow 1GB+. Diagnóstico via QEMU monitor (RIP/RSP) + `llvm-nm` + `memsave`/`xp` da stack: 3 self-deadlocks de lock não-reentrante + 1 guarda estrutural. Commit `2f571fae` (11 arquivos).

## Deadlocks (todos provados com backtrace da stack real)
- **D1 `BOOT_LOG` reentrante:** `dispatch_bytes` segura o guard através de `write_to_disk_journal→append_raw→persist_now→log_no_flush→slog` (objdump: drop após o journal). BSP em `TicketLock<BootLog>::lock` com a própria linha "OK 32633" na stack. Fix: `append_raw` só bufferiza (persist fica p/ `log_quiet`/`flush`).
- **D2 `LAST`/`CLAIMED_HEAP`:** `if let Some(p) = *M.lock()` + relock no corpo (k_hal `aios_adapt`, k_nano `allocator`). Fix: single-lock. Auditoria dos 10 sites: 2 reais, 8 falsos positivos.
- **D3 `PRE_FAT_BUF`:** spin em `buffer_log` durante grow 1GB+ (holder nunca identificado — APs HLT, handlers lock-free). Mitigação: `try_lock` best-effort (serial já fluiu) + dreno no persist ok (buffer saturava em 512 e fossilizava).
- **Guarda estrutural `EMIT_GUARD`:** `format!` do journal aloca → heap pressionado → grow → slog aninhado → `BOOT_LOG.lock` com outer segurando (backtrace com format! aninhado + RDI=BOOT_LOG + IF=0). `dispatch_bytes` agora desvia aninhado p/ `puts` lock-free (`[NESTED]`): **133 intercepções** validadas, 0 faults.

## Itens do pacote mitigação
- **AP offload (item 1):** oráculo recomenda manter sync+hardening (acima); AP workers seguem inviáveis (IDT/IPI/TLB residuals).
- **KVM (item 2):** N/A no Windows — WHPX já é o nativo em A; TCG em B é por RAM; I4 teve 1 violação transiente auto-diagnosticada.
- **#412 WASM:** `promote_ephemeral_to_wasm` real (Op→wasm→wasmi+register+persist) + `reload_persisted_wasm_skills` wirado no boot; teste host verde. Residual: `genesis_spawn`, LLM emitir op-IR.
- **Modelo FAT32:** wire boot `HWEXPRT4.BIN→register_bytes(HwExpert)`; `is_sandbox()`==true p/ qualquer hv matava o ramo ATA (removido); gate p/ configs sem modelo (grow 1GB+ em A com FALCON3). `LOADED` validado em B; Trinity segue `hwexpert=ABSENT` (slot não alimenta router — follow-up).

## Validação
`cargo check --release` 0 erros · par mesh A 8G/8c WHPX (`Master`) + B 4G/4c TCG (`Memory`), peers=1 bilateral, desktop 3min+ (orb+HUD+TTS), 0 faults. Infra: `ovmf_code/vars.fd` recriados do QEMU; netmode de A p/ `0x13E000000` (falso BRIDGE por colisão com modelo); socket mesh sem reconnect (reiniciar A exige reiniciar B).
