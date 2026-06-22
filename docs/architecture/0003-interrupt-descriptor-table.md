# ADR-0003: Interrupt Descriptor Table (IDT)

**Status:** Accepted  
**Date:** 2026-06-21  
**Driver:** Necessidade de capturar exceções de CPU como pré-requisito para Page Fault handling e sistema de memória semântica.

## Context

O Ring 0 do Neural Microkernel precisa proteger o sistema contra falhas de hardware. Sem uma IDT, qualquer exceção da CPU (divisão por zero, página não encontrada, dupla falha) causa um triple fault e reinicialização da máquina sem nenhum log.

Para construir o sistema de memória semântica (mapeamento de arquivos em páginas), precisamos obrigatoriamente de um handler para **Page Faults**. A IDT é a estrutura que a CPU consulta ao disparar uma exceção — sem ela, não há como capturar PF.

## Decision

Implementar a IDT usando a crate `x86_64` (v0.14.11), que fornece tipos seguros para todas as estruturas do processador x86-64.

### Estruturas utilizadas

| Tipo | Propósito |
|---|---|
| `InterruptDescriptorTable` | Tabela de 256 entradas que mapeia número de interrupção → handler |
| `InterruptStackFrame` | Estado da CPU salvo na pilha ao entrar no handler (RIP, CS, RFLAGS, RSP, SS) |
| `TaskStateSegment` (TSS) | Segmento que contém as pilhas de Interrupt Stack Table (IST) |
| `GlobalDescriptorTable` (GDT) | Necessário para carregar o seletor do TSS via `ltr` |
| `SegmentSelector` | Seletor que aponta para o descritor TSS na GDT |

### Handlers implementados

| Exceção | Vetor | Erro | Comportamento |
|---|---|---|---|
| **Breakpoint** (`#BP`) | 3 | Não | Loga VGA + serial, retorna (continua execução) |
| **Double Fault** (`#DF`) | 8 | Sim (u64) | Loga VGA + serial, entra em pânico (aborta) |

### Stack switching para Double Fault

Double Fault é crítica porque pode ocorrer quando a pilha atual está corrompida (ex: stack overflow). Sem um stack switch, o handler tenta usar a mesma pilha corrompida, resultando em Triple Fault.

**Solução:** Um IST (Interrupt Stack Table) entry no TSS define uma pilha separada para o handler de Double Fault:

```
TSS.IST[0] ──> [STACK de 20KB estática]
                    ^
                    └── CPU faz `mov rsp, IST[0]` ao entrar no handler #DF
IDT[#DF].stack_index = 0
```

A pilha é alocada como `static mut [u8; 20KB]` e seu endereço é calculado em tempo de inicialização via `lazy_static!`.

### Sequência de inicialização

1. `vga_buffer::init(offset)` — mapeia VGA text buffer na memória física
2. `interrupts::init_idt()`:
   a. Cria e carrega GDT com descritor de code segment (ring 0) + TSS
   b. Carrega seletor CS com o novo GDT (via `set_cs`)
   c. Carrega TSS (via `ltr`)
   d. Carrega IDT (via `lidt`)
3. `int3()` — breakpoint forçado para testar handler
4. Loop infinito

## Consequences

**Positive:**
- Exceções de CPU agora são capturáveis com log visível
- Double Fault não causa mais triple fault (stack switch)
- Pré-requisito para Page Fault handling (próximo passo)
- Código usa APIs seguras da crate `x86_64` em vez de `unsafe` direto

**Negative:**
- `spin::Mutex` ainda não é IRQ-safe — se uma exceção ocorrer enquanto o lock do VGA está held, deadlock
- Pilha do Double Fault é fixa em 20KB — se o handler usar mais, corrompe memória adjacente
- GDT atual recria segmentos do zero (não estende a do bootloader)

**Risks:**
- `static mut` para a pilha DF é tecnicamente UB se acessada concorrentemente (não ocorre na prática: só a CPU escreve nela)
- Mudança de GDT via `set_cs` pode quebrar se o novo code segment não for compatível com o estado atual da CPU

## Alternatives Considered

1. **Inline assembly para `lidt`** — Mais controle, mas o `x86_64` crate abstrai com segurança e já é dependência necessária.
2. **`lazy_static!` no lugar de `static mut`** — Pilha precisa de endereço fixo conhecido em tempo de compilação; `static mut` é a única opção em `no_std` sem alocador.
3. **PIC / APIC** — Não necessário neste estágio. Após IDT, o próximo passo é habilitar o hardware timer (PIT) para preempção.

## References

- Intel SDM Vol. 3, Chapter 6: Interrupt and Exception Handling
- AMD APM Vol. 2, Section 8: Interrupts and Exceptions
- Blog_OS: "Interrupts" and "Double Faults" — https://os.phil-opp.com/
