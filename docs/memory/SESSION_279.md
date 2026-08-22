# SESSION_279 — SMP AIOS: Observe MADT, não teto de cores

**Data:** 2026-08-21  
**ADR:** 0088 (premissa), 0055 (SMP canônica), 0057 (wake sequencial), 0061 (BitNet/Xeon residual NUMA)  
**IDEA:** #492 (SMP completo — em andamento)

## Objetivo

SMP no QEMU e no HW real. AIOS: **detectar o x86 e usar o que existe**. Não hardcode de 8 lógicos. Não doutrina “teto = MADT ∩ RAM”.

## O que é AIOS aqui (memorizado)

1. IA desde o boot (ADR-0088): Observe→Plan→Act→Verify→Remember.
2. MADT Enabled **é o inventário** (Intel E+P+HT, AMD, Xeon) — não um cap de produto.
3. RAM é custo de stack por AP, não política de “quantos cores pode usar”.
4. Teto numérico (`MAX_APS=7`, `.min(8)`, guess `bsp+1`) é anti-AIOS.
5. Copiar **ideias** do Redox (jmp no IP=0, `ready` lowmem, PTE executável), não o blob NASM, não spin infinito.
6. HITL: BSS `MAX_APS=511` ainda é dívida de array (PerCpu/TSS); se MADT for maior → log error, não “usar menos de propósito”.
7. FeatureGate TCG `max_aps=4` (ADR-0055) continua gate de **ambiente**, não de silício.

## Mudanças

- Trampoline: `jmp` 16-bit no byte 0; `sipi_hit`/`ready` via HHDM; PTE identity sem NX.
- Bin `init_smp` → `k_nano::smp::init_smp`.
- MADT type 0/9 só Flags bit0 Enabled; IDs `u32`; `lapic_id()` x2APIC = MSR 802 inteiro.
- Sem guess sequencial se MADT vazio.
- Work-stealing / Cortex matmul sem `.min(8)`; CorePools `Vec`; 0x1A no AP.
- Sem cap ¼ heap. Wake = lista MADT (menos BSS 511 se estourar).

## Evidência (check)

- `cargo check --release -p k-nano` 0 erros.
- `k_ai`/`cortex`/`hermes`/`jarbas` 0 erros.
- `neural-kernel` `--target x86_64-unknown-none` 0 erros.

## Limites

- Aceite QEMU TCG (`sipi_hit`/`ready`/`ONLINE`/`counter`) = este SESSION após rebuild `uefi.img`.
- `ap_pollable` / GDT 1 TSS / runqueue 16 = residual 0057 WS-F.
- Não Ring3 / `register_native_ring`.

## Próximo

Boot TCG `-smp 2` NoDisk; ler serial `SMP:`. Metal: `online == madt_enabled - 1`.
