---
name: self_heal
description: Analisa erros do kernel e sugere acao de recuperacao automatica
required_tokens: [1]
requires_network: false
---

# Self Heal Skill

Quando um `KERNEL_ERROR`, `HEALTH_ISSUE`, ou mensagem de erro do sistema chegar, diagnostique e sugira acao corretiva.

## Workflow

1. Identifique a **classe do erro**:
   - **Memory**: Page Fault, OOM, heap corruption, dangling pointer
   - **Execution**: Double Fault, General Protection Fault, invalid opcode, divide error
   - **Resource**: IRQ storm, timer starvation, out-of-memory, DMA failure
   - **Logic**: assertion failed, estado inconsistente, timeout de driver
   - **External**: rede indisponivel, dispositivo desconectado, IO timeout
2. Verifique o **historico**: se j[a tentou essa estrategia antes, sugira alternativa (evite loop de reboot)
3. Sugira uma **RecoveryAction** viavel no contexto bare-metal:
   - `restart_daemon` — reinicia um agente especifico (ex: NetAgent, WifiAgent)
   - `create_skill` — gera um skill corretivo para o padrao
   - `log_and_continue` — erro nao fatal, registra e segue
   - `restore_checkpoint` — restaura estado anterior (recomendado para Double Fault)
   - `reset_device` — re-inicializa um dispositivo especifico
   - `escalate` — erro grave que precisa de intervencao humana
4. Se for **Double Fault**, recomende restore de checkpoint como prioridade
5. Se for **Page Fault** em endereco conhecido, diagnostique causa provavel (stack overflow, null pointer, acesso a memoria nao mapeada)

## Tratamento de Erros

- **Multiplos erros em sequencia**: se o mesmo erro aparecer 3+ vezes em 100 ticks, sugira escalar para humano
- **Erro desconhecido**: se nao reconhecer o codigo, classifique como External e recomende log_and_continue com observacao
- **Recuperacao falhou**: se a acao sugerida ja foi tentada sem sucesso, nao repita — sugira escalate

## Exemplos

Input: "Page Fault em 0x180fee000b0 — Write access"
Output: "**MemoryFault** — acesso de escrita a endereco nao mapeado.
Provavel causa: stack overflow ou ponteiro corrupto.
Sugiro: verificar page table nas proximidades, compactar heap.
Se recorrente, restart do agente afetado."

Input: "Double Fault err=0"
Output: "**ExecutionFault** — Double Fault critico.
Sugiro: restore de checkpoint imediatamente.
Se falhar, reiniciar core AP.
Causas comuns: IST stack insuficiente, falha em handler de interruptor aninhado."

Input: "ATA timeout no setor 0x1A2B3C"
Output: "**ExternalFault** — I/O timeout no disco.
Sugiro: log_and_continue + reset_device (ata).
Se persistir, verificar cabo/conexao no HW real.
QEMU: provavel simulacao lenta, ignore."

Input: "ERRO: NETSTACK travado por 500 ticks"
Output: "**ResourceFault** — NETSTACK sem progresso.
Sugiro: restart_daemon (NetAgent).
Se recorrente (2+ vezes), reset_device (e1000/rtl8139)."

## Regras de Seguranca

- Nunca sugira `restore_checkpoint` para erros fatais que podem corromper o checkpoint atual
- `escalate` so como ultimo recurso depois de 3 tentativas de recuperacao automatica
- Nao reinicie dispositivos que estao em uso por outros agentes sem verificar dependencias
- Para Page Faults em endereco 0x0, diagnostique null pointer — nao tente restaurar
