# SESSION_281 — K22 metal: ICR x2APIC + GDT por CPU (ADR-0088)

**Data:** 2026-08-22  
**ADR:** 0088 (premissa), 0055/0057 (SMP), 0065 P13 (TSS AP)  
**IDEA:** #492 (em andamento), #512 (premissa aplicada)

## Objetivo

Todos os lógicos do MADT no hardware real (i5 7ª + Core 7 240H). Falcon3/HW Expert no pendrive **não** executam em K22.

## Observe (metal)

- Imagem parou em **K22** (`init_smp` / INIT-SIPI-SIPI). K23 só se `init_smp` retorna.
- **i5 7ª:** reboot. **240H:** freeze.
- Modelos no stick são contexto de imagem, não causa do hang.

## Inferência

1. CPUID x2APIC → kernel liga EXTD e escrevia ICR com bits 14/15 (assert/level) e shorthand. SDM §10.12.9: bits 12–19 **reservados**. WRMSR 0x830 → `#GP` no BSP = reboot Kaby Lake.
2. INIT deassert **não existe** em x2APIC (Linux idem). O write era outro `#GP` / no-op perigoso.
3. 240H: MADT híbrido N IDs × 3 retries × ~250 ms parece freeze; firmware EXTD=1 + MMIO SVR = `#GP` se não pulado.
4. GDT da crate `x86_64` 0.14 = **8 slots**. 1 TSS compartilhado + `sti` no AP = stack IST do BSP = reboot. Tratar esse teto como política = **bypass ADR-0088 §4** (mesmo erro que `MAX_APS=7`).

## Act

- `x2apic_icr_value`: dest[63:32] | delivery | vector; bits 12–19 = 0.
- Deassert só xAPIC; x2APIC INIT = um IPI dirigido.
- Firmware já EXTD → skip SVR MMIO; `enable_x2apic_this_cpu` em cada AP.
- MADT type 0+9 **dedup**; id>255 liga x2APIC em vez de pular o core.
- IPI “all” = loop dirigido (shorthand ilegal em x2APIC).
- `k_nano::gdt`: tabela própria, 1 TSS por CPU (`MAX_APS+1`).
- IST AP no **heap** (VA contígua). `ltr` do slot + `sti` se IST ok.
- Idle: `hlt` só com IF=1; senão pause/mwait.

## Verify (host)

- `cargo test -p k-nano x2apic_icr` → 2/2 PASS.
- `cargo check -p k-nano --release --target x86_64-unknown-none` → 0 erros.

## Limites

- Aceite **metal** (não QEMU): K23 + `online == madt_enabled - 1`.
- BSS `MAX_APS=511` / PerCpu array = dívida (Vec no boot se MADT maior).
- `ap_pollable` / matmul AP = residual 0057 WS-F (agora com IDT+TSS por CPU o caminho existe).
- Rebuild `usb_hw.img` ainda não gravado neste SESSION.

## Remember

Teto de crate ≠ silício. Workaround (`sti` off) não é destino. ICR x2APIC: zero bits reservados.
