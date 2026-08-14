# SESSION_262 — Regressão pós-rebuild: scans QEMU-loader #PF + SMP wake com IDs do MADT + freeze self_heal (2026-08-14)

**Escopo:** Regressão de boot após `cargo clean` + rebuild from scratch — HW real (i5-7300HQ /
GTX 1050 / 16GB, boot UEFI do pendrive) parou no K45, depois K51 (init_phase). Investigação
revelou 3 bugs independentes: scans QEMU-loader lendo páginas não-mapeadas, wake SMP usando
guess sequencial em vez dos IDs reais do MADT, e freeze do self_heal no metal sem ATA.
**Status:** ✅ Em andamento — 4 commits (`61682db`, `f11d41e`, `ecb3f6c`) — 0 erros — validado QEMU 6G+loader.

---

## 1. Sintoma

- Após `cargo clean` (5830 arquivos/20.4GiB) + `cargo build --release` from scratch, o boot
  HW real regrediu: freeze no K45 (ADR-0041 demos). Antes chegava ao K53 (scheduler run start).
- No QEMU com `-m 2G` (sem loader), o boot mostrava **#PF storm** que não existia antes.

## 2. Bug 1 — Scans QEMU-loader liam páginas não-mapeadas (#PF storm)

### Causa
Os scans do QEMU-loader (`read_volatile` em ranges fixos) liam **sem checar se a página é
PRESENT**:
- BGE scan `0x100000000..0x180000000` (main.rs:2200)
- `try_expert_qemu_scan` `0x129000000..0x180000000` (main.rs:3124) — RUSTCODER/HWEXPERT
- HWEXPRT4 scan `0x129400000..0x180000000` (main.rs:3300)

Com `-m 2G`, a RAM não alcança `0x129000000` (4.6GB) → o scan lia hole não-mapeado → #PF
(`CR2=pmoff+0x100000000` / `+0x129000000`). No HW real (16GB) os endereços existem, mas o
scan **não deveria crashar em máquina menor** — AIOS mede e pula.

### Fix (`61682db`)
- `k_nano::memory::is_page_present(virt)`: walk PML4→PDPT→PD→PT do CR3 atual, tratando
  HUGE_PAGE 1GB/2MB.
- Todos os 3 scans só leem se `is_page_present(addr + pm)`.
- Stack do Limine 2MB→**8MB** + reserva via RSP 8MB (stack overflow no registro de agentes
  pós-rebuild — CR2=pmoff+0xffff800058263000).

### Validação
QEMU 6G+loader (cenário fiel ao HW): K45 (ADR-0041 demos) passa completo, **0 #PF**, P6/P7/P8 OK.

## 3. Bug 2 — Wake SMP usava guess sequencial em vez dos IDs reais do MADT

### Sintoma
`SMP: hv=baremetal madt_lapics=4 ap_expected=3 allow_smp=true` mas `total_cores=1 apos wake`
— 0 APs acordaram no metal.

### Causa raiz
O `init_smp` do **bin** (`crates/neural-kernel/src/smp/mod.rs:185-188`) usava:
```rust
for i in 0..n_aps { ap_ids[i] = bsp_lapic_id.wrapping_add((i as u8) + 1); }
```
**guess sequencial** (`bsp+1, bsp+2, bsp+3`) em vez dos IDs reais do MADT (`BOOT_APIC_IDS`).
O k_nano (`crates/k_nano/src/smp/mod.rs:377-399`) já usava os IDs do MADT, mas o bin tem sua
**própria cópia** do `init_smp` que ignora o MADT.

No i5-7300HQ (4C/8T com HT), os LAPIC IDs **não são sequenciais** (ex: `0,1,4,5` ou `0,2,4,6`).
Se o BSP é 0, o bin tentava acordar `1,2,3` — mas os APs reais são `1,4,5` (ou `2,4,6`).
INIT-SIPI para ID inexistente → 0 APs acordam.

### Fix (`ecb3f6c`)
O bin `init_smp` agora usa `BOOT_APIC_IDS` (MADT) com fallback sequencial + log
`SMP: ap_ids = [...]` no ramlog (dump BOOT.LOG no FB).

### Lição
**Duas cópias do mesmo módulo (bin vs k_nano) divergem silenciosamente.** O bin `smp/mod.rs`
é uma cópia do k_nano com checkpoints K22 próprios — o fix no k_nano nunca roda. Sempre
verificar qual cópia o bin realmente chama (`crate::smp::init_smp` resolve para o bin).

## 4. Bug 3 — Freeze do self_heal no init_phase (K51) no metal sem ATA

### Sintoma
`INIT1: r1 poll platform → memory → trust → self_heal` e trava no `self_heal` (Oneshot
`BootSelfHealAgent`). O dump do ramlog rodava **antes** do init_phase, então o trace nunca
aparecia no FB.

### Causa
No metal sem ATA (boot USB, `ATA probe=none`), o `BootSelfHealAgent.tick()` fazia
`pci::scan_pci()` + `HardwareInventory::collect()` — inventário pesado que pode travar em
devices com MMIO lento.

### Fix (`ecb3f6c`)
Se `ATA_DRIVER` é `None`, o self_heal pula o scan PCI e faz honest noop
(`run_vid_gated_scan(&[])` + `SystemArchitecture` vazio).

### Instrumentação (`f11d41e`)
- `AgentRegistry.init_trace` (fn pointer zero-dep no agent-core) chamado antes de cada tick
  de Oneshot no init_phase.
- Bin loga `INIT1: r<N> poll <agente>` no ramlog **E no FB** — o último nome na tela revela
  o agente do freeze.

## 5. Lições

1. **Scans de memória fixa devem checar PRESENT** — ler hole não-mapeado = #PF storm. AIOS
   mede e pula (is_page_present).
2. **Duas cópias de módulo divergem** — o fix no k_nano `init_smp` nunca rodou porque o bin
   tem sua própria cópia. Verificar qual cópia o bin chama.
3. **LAPIC IDs com HT não são sequenciais** — wake SMP deve usar os IDs do MADT, não guess.
4. **Dump do ramlog roda antes do init_phase** — trace de init_phase precisa imprimir no FB
   direto, não só no ramlog.
5. **`cargo clean` + rebuild from scratch expõe bugs latentes** que o cache incremental
   mascara (regra já conhecida, reaplicada).

## 6. Pendências

- Confirmar no HW real: `SMP: ap_ids = [...]` mostra IDs reais e `total_cores=4` (ou mais).
- Confirmar que o self_heal passa (sem freeze) e o boot chega ao runtime.
- RAM greeting superestima (18424MB vs 16GB real) — pega o fim do último range do memory
  map que inclui MMIO acima de 16GB. Ajustar `TOTAL_RAM_MB` para somar só ranges USABLE.
- BUG VGA notebook antigo (3ª resolução) segue aberto.