# SESSION_269 — Self-heal BOOT.LOG: 1ª falha sem spam

**Data:** 2026-08-17  
**Foco:** `BOOT.LOG flush FALHOU - ATA PIO: overwrite_boot_log falhou` repetido infinitamente ≠ AIOS.

## Sintoma

No log (serial/ramlog) apareciam dezenas/centenas de:

```
[T+N] [R0] [k-nano] [LOG] [info] - BOOT.LOG flush FALHOU - ATA PIO: overwrite_boot_log falhou
```

Causas compostas:

1. **Amplificador:** em falha, `SINCE_FLUSH` não resetava → após 16 linhas **cada** `log_quiet`/`append_raw` re-chamava `persist_now`.
2. **Fallback cego:** USB-MSC falhou/ausente → martelava ATA (HD interno sem `BOOT.LOG`) para sempre.
3. **SysInfoAgent** a cada 50 ticks chamava `ensure_persisted` sem backoff.
4. **Zero self-heal:** só logava o mesmo erro; não diagnosticava, não skipava backend inadequado, não publicava `HEALTH_ISSUE`.

## O que uma IA deveria fazer na 1ª ocorrência

| Passo | Ação |
|-------|------|
| Observe | Uma linha com backend + motivo tipado (`BootLogMissing` / `IoFail` / …) |
| Diagnose | Quais backends existem; quem tem `BOOT.LOG` no root FAT |
| Act | Re-probe USB-MSC (live stick); **skip permanente** backend sem arquivo; `HEALTH_ISSUE:I5:boot_log:…` uma vez |
| Backoff | 50→100→…→3200 ticks (padrão mesh) — sem spam |
| Verify | Retry imediato pós-heal; sucesso limpa breaker |

## Fix (`k_nano::boot_logger`)

- `OverwriteResult` tipado (`Ok` / `NoFatParts` / `BootLogMissing` / `IoFail`)
- Circuit breaker: `BACKEND_SKIP`, `NEXT_RETRY_TICK`, `FAIL_STREAK`, `HEAL_FIRED`
- 1ª falha → `heal_on_first_failure` (MSC re-probe + HEALTH_ISSUE) + log único com backoff
- ATA/AHCI/NVMe sem `BOOT.LOG` → `mark_skip` (não martelar HD interno)
- `log_quiet` / `append_raw` / `ensure_persisted` respeitam backoff
- `try_ensure_usb_msc` gated `target_os = "none"` (host não SEGv em xHCI)
- Aceita partição ESP `0xEF` na busca (além de 0x0B/0C/1C/73)

## Verificação

- `cargo test -p k-nano --features fat-boot-log boot_logger` → **6/6 PASS**
- `cargo check -p neural-kernel --features fat-boot-log` → **0 erros**

## Lição

Log repetido da mesma falha sem act/backoff é antítese da premissa AIOS (ADR-0088). Self-heal = observe → plan → act → verify **na primeira vez**; depois silêncio + retry inteligente.
