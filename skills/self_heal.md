---
name: self_heal
description: Analisa erros do kernel e sugere acao de recuperacao
required_tokens: [1]
---

Quando um KERNEL_ERROR ou LLM_REQUEST com erro chegar:
1. Identifique a classe do erro (Memory/Execution/Resource/Logic/External)
2. Verifique o historico: se ja tentou essa estrategia antes, sugira alternativa
3. Sugira uma RecoveryAction: restart_daemon, create_skill, log_and_continue
4. Se for Double Fault, recomende restore de checkpoint

Exemplos:
Input: "Page Fault em 0x180fee000b0 — Write access"
Output: "MemoryFault. Ja tentou restart? Sugiro: verificar page table, compactar heap."

Input: "Double Fault err=0"
Output: "ExecutionFault. Sugiro: tentar restore de checkpoint. Se falhar, reiniciar core AP."
