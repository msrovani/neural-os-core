# SESSION_222: Power Management — P-state, C-state, S3 Suspend/Resume

**Data:** 2026-07-26
**Sprint:** v1.9.12-power
**Build:** `cargo check --release` — 0 erros (warnings preexistentes)

## Motivação

O neural-os-core não tinha gerenciamento de energia de CPU:
- **P-states** (frequência): ausente — CPU sempre em clock fixo
- **C-states** (idle): só `hlt` (C1), sem suporte a estados mais profundos
- **S3 suspend**: ausente — só S5 (shutdown) via ACPI PM1a_CNT
- **S3 resume**: ausente — sem trampoline de wake

O ClaudioOS (ADR-0062 P20) tem ACPI S3/S5 com 921 LOC em `power.rs`.
Nosso S5 já existia; implementamos S3 + cpufreq + MWAIT = melhor dos dois mundos.

## O que foi implementado

### 1. `k_nano/src/cpufreq.rs` — P-state + Governor (~240 LOC)

**P-state via MSR:**
- `IA32_PERF_CTL` (0x199): escreve ratio alvo (100 MHz units)
- `IA32_PERF_STATUS` (0x198): lê ratio atual
- `IA32_ENERGY_PERF_BIAS` (0x1B0): dica de política energética (0=perf, 15=power)

**Governor:**
- `Performance` → P0 max (mínima latência)
- `Powersave` → Pn min (mínimo consumo)
- `Ondemand` → escala por carga via `ondemand_tick(work_pending)`

**Detecção:**
- CPUID leaf 0x16 (Skylake+) para frequência base/máxima
- Fallback CPUID leaf 1 EBX bits 31:22 para ratio máximo
- Probe MSR write-take-effect (testa se escrever PERF_CTL altera PERF_STATUS)
- QEMU: MSR writes são no-op silenciosos (seguro)

**APERF/MPERF:**
- `IA32_APERF` (0xE8) / `IA32_MPERF` (0xE7): ciclos reais vs máximos
- `actual_ratio()` retorna frequência real (detecta thermal throttle)

### 2. `k_nano/src/smp/ap_work.rs` — MWAIT real (~+40 LOC)

- `platform_probe::CpuFeatures::mwait` detectado via CPUID.1:ECX[3]
- `ap_idle_loop`: se MWAIT disponível, executa `monitor`/`mwait` em vez de `hlt`
- `MONITOR_FLAG` alinhado a 64 bytes (cache line), com `AtomicU8` para wake
- `enqueue()` incrementa MONITOR_FLAG para acordar APs antes do IPI
- `set_mwait_hint(cstate)`: configura profundidade C1–C6 (padrão C1)
- `has_pending()`: usado pelo governor ondemand
- Fallback `hlt` em CPUs sem MWAIT

### 3. `k_nano/src/suspend_resume.rs` — S3 Suspend/Resume (~200 LOC)

**S3 entry:**
- Salva CR3 (page tables) e RSP (kernel stack)
- Escreve FACS waking vector (32-bit + 64-bit) apontando para trampoline
- Save e1000 NIC context (16 regs + MTA de 128 entradas)
- Park APs via `send_ipi_halt()`
- Set powersave governor + EPB=15
- Write SLP_TYP=3 + SLP_EN → PM1a_CNT → platform SLP_S3#

**Trampoline de resume (64-bit na posição física 0x7000):**
```
mov rax, <saved_CR3>      ; restore page tables
mov cr3, rax
mov rax, <saved_RSP>      ; restore kernel stack
mov rsp, rax
mov rax, <s3_resume_entry> ; jump to C handler
jmp rax
```
- 64 bytes (cabe em 1 página abaixo de 1 MB)
- Firmware UEFI salta em modo 64-bit para o endereço físico
- FACS waking vector escrito antes do suspend

**`s3_resume_entry()` — C handler no higher-half:**
- Re-inicializa APIC (SVR + EOI)
- Re-inicializa PIT (timer modo 3, divisor 65536)
- Restaura EPB para 6 (balanced)
- Seta flag `S3_RESUMED` = 1
- **Residual:** restore e1000 + unpark APs (precisa estado global do driver)

### 4. Integração no scheduler (`neural-kernel/src/main.rs`)

- Closure `halt` do `registry.run()` chama `cpufreq::ondemand_tick(ap_work::has_pending())`
- Governor Ondemand escala frequência por carga real da fila

### 5. Outras mudanças

| Arquivo | Mudança |
|---------|---------|
| `k_nano/src/acpi.rs` | + parse `\_S3` DSDT, + FACS parser (waking vector), + `FACS_PHYS` |
| `k_nano/src/platform_probe.rs` | `CpuFeatures::mwait` + `has_mwait()` |
| `k_nano/src/apic.rs` | + `send_ipi_reschedule_to(apic)` directed IPI |
| `k_nano/src/e1000.rs` | Todos registros `pub` (para save/restore) |
| `k_nano/src/scheduler/core_pair.rs` | `send_wake_ipi` agora usa APIC real |
| `k_nano/src/hardware/probe.rs` | + `cpufreq::probe_and_init()` |
| `k_nano/src/lib.rs` | + `cpufreq`, + `suspend_resume` |

## Build

```powershell
cargo check --release 2>&1 | tail -1
# Finished `release` profile [optimized] target(s) in ...
# 0 errors
```

## Referências

- ADR-0062 (ClaudioOS P20 power management — 921 LOC `power.rs`)
- Intel® 64 and IA-32 Architectures SDM Vol 3: MSRs IA32_PERF_STATUS/IA32_PERF_CTL (14.1.4)
- Intel® SDM: MWAIT/MONITOR (Chapter 9: C-state management)
- ACPI Spec 6.4: FACS (Table 5.12.1), `\_S3` (Section 5.8.2)

## Próximos passos (residuals)

| Item | Prioridade |
|------|-----------|
| Restore e1000 NIC completo no resume (precisa state global exposto) | Médio |
| Unpark APs pós-resume (chamar wake_aps_sequential) | Médio |
| Teste S3 em HW real (laptop com suporte ACPI S3) | Alto |
| IOAPIC save/restore (redirection entries) | Baixo |
| Driver hooks AHCI/NVMe/USB para save/restore | Baixo |
