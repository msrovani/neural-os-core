# Sessão 037 — Self-Healing Kernel (Bloco Único, Sprints 32-36)

**Data:** 26/06/2026
**Versão:** v0.36.0

## Visão Geral
Sprints 32-36 foram consolidados em **1 bloco coeso**: Self-Healing Kernel.
O que era 5 sprints separados virou um pipeline único de auto-cura.

## O Bloco Self-Healing

| Mini-sprint | O que entrou |
|---|---|
| **SelfHeal + EventBus** | `SelfHeal::analyze()`, `KERNEL_ERROR` topic, panic_handler modificado |
| **Feedback loop** | `lessons: Vec<FailedStrategy>`, `already_tried()` → evita repetir falhas |
| **Failure Taxonomy** | `FailureClass::{Memory,Execution,Resource,Logic,External}`, `default_recovery()` |
| **Exception handlers** | Page Fault, Double Fault, GPF com SelfHeal + classificação |
| **Respawn + Corrective** | `RESPAWN_QUEUE`, executor recria tasks, `LLM_REQUEST` com contexto do erro |

## Pipeline Final
```
ERRO → FailureClass::classify() → SelfHeal::analyze()
  → already_tried()? → alternativa
  → RecoveryAction:
      RestartDaemon → RESPAWN_QUEUE → executor recria
      CreateSkill → pending_fixes pendente
      LogAndContinue → non-fatal
  → LLM_REQUEST (corrective prompting) → LLM sugere
  → Se falhar → lessons.push() → próxima tenta DIFERENTE
```

## Arquivos criados/modificados no bloco
| Arquivo | Linhas | Função |
|---|---|---|
| `self_heal.rs` | ~100 | SelfHeal, FailureClass, RecoveryAction, lessons |
| `interrupts.rs` | +15 | Page Fault + Double Fault com SelfHeal |
| `task/executor.rs` | +10 | RESPAWN_QUEUE check a cada tick |
| `main.rs` | +30 | spawn_task_by_name, corrective prompting |
| `conversation.rs` | +1 | EventKind::KernelError |
| `prepare_hw_dataset.py` | +30 | Error recovery training data |
