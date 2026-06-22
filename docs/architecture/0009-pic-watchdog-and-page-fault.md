# ADR-0009: PIC Watchdog and Page Fault Safety

**Status:** Accepted  
**Date:** 2026-06-21  
**Driver:** Isolar heranças do x86_64 (PIC 8259A), gerenciar limpeza de memória (FrameDeallocator), e configurar o Watchdog do sistema antes de avançar para Ring 2 (Sandbox WASM).

## Context

Três lacunas estruturais precisavam ser fechadas antes de suportar execução de código no Ring 2:

1. **Hardware Interrupts (PIC 8259A):** Até o Sprint 7, o kernel rodava com interrupções de hardware desabilitadas (IF=0). O PIT (Programmable Interval Timer) não tinha handler, e o PIC estava mapeado no modo padrão (vetores 0-15) que conflita com exceções da CPU.

2. **Memory Safety:** Não havia segurança contra acesso inválido de memória no espaço do kernel. Uma falta de página (`#PF`, vector 14) causaria Double Fault genérico sem diagnóstico.

3. **Frame Leak:** O `BootInfoFrameAllocator` aloca frames fisicamente sem nunca desalocá-los. Para suportar alocação dinâmica de páginas (ex: heap elástico, mapeamento de memória para WASM), precisamos de uma trait de desalocação.

## Decision

### 1. PIC 8259A Remap

Controladores `pic8259::ChainedPics` remapeados:

| PIC | Portas I/O | Vetores | Offset |
|---|---|---|---|
| PIC1 (master) | 0x20 (cmd), 0x21 (data) | 0–7 → 32–39 | 32 |
| PIC2 (slave) | 0xA0 (cmd), 0xA1 (data) | 8–15 → 40–47 | 40 |

ICW (Initialization Command Words) enviados por `ChainedPics::initialize()`. Bloco `unsafe` justificado pela necessidade de garantir que a sequência ICW não seja interrompida.

### 2. PIT Timer — Neural Watchdog

O PIT (IRQ 0, vetor 32) opera no modo padrão (~18.2065 Hz, divisor 65536). O handler:

- Incrementa `TIMER_TICKS` (atomic counter)
- Envia EOI ao PIC1 (`ChainedPics::notify_end_of_interrupt`)
- Não usa locks de VGA/serial para evitar deadlock

**Não** é um escalonador preemptivo clássico — é um *Neural Watchdog* que garante que o sistema "está vivo" e fornece uma base de tempo contínua.

### 3. Page Fault Handler

Vector 14 (`#PF`), handler com `extern "x86-interrupt"`:

- Lê `CR2` (endereço que causou a falha)
- Imprime `[SECURITY] Page Fault detectado em {addr}. Acesso negado.`
- Aborta com `hlt` loop

Funciona como barreira de segurança para o Ring 2: qualquer acesso inválido de um módulo WASM será capturado e diagnosticado.

### 4. FrameDeallocator Trait

```rust
#[allow(dead_code)]
pub trait FrameDeallocator {
    fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>);
}
```

`EmptyFrameDeallocator` como stub no-op. Sprint 9 implementará bitmap allocator.

### 5. Initialization Sequence

```
init_idt()            # Sprints 3-7
init_pics()           # Sprint 8: PIC remap antes de enable()
enable_interrupts()   # Sprint 8: sti (IF=1)
loop { hlt() }        # Watchdog heartbeats
```

## Consequences

**Positive:**
- Kernel agora responde a todos os tipos de interrupção de hardware
- Page Fault tem diagnóstico próprio (não vira Double Fault)
- `FrameDeallocator` trait permite futura desalocação de páginas
- Watchdog contínuo: ~18.2 verificações por segundo

**Negative:**
- `hlt()` + polling do contador não é idle eficiente em termos energéticos (QEMU apenas)
- `notify_end_of_interrupt` é `unsafe` e requer prova manual de que o PIC não está sendo reentrante

**Risks:**
- `spin::Mutex` no PIC pode causar deadlock se handler de timer for interrompido... não aplicável (timer handler é a única função que usa o lock)
- Page Fault handler agora **aborta** o sistema — em produção, deveria matar apenas o processo Ring 2

## Alternatives Considered

1. **APIC (Advanced Programmable Interrupt Controller):** Substitui 8259A em sistemas SMP. Postergado para quando suportarmos multi-core.
2. **IOAPIC:** Para virtualização de IRQ. Complexidade desnecessária no momento.
3. **HPET:** Mais preciso que PIT. Inviável em QEMU sem ACPI.

## References

- Intel 8259A Datasheet: https://pdos.csail.mit.edu/6.828/2018/readings/hardware/8259A.pdf
- PIT 8254: https://wiki.osdev.org/Programmable_Interval_Timer
- pic8259 crate v0.10: https://crates.io/crates/pic8259
- ADR-0003: Interrupt Descriptor Table
