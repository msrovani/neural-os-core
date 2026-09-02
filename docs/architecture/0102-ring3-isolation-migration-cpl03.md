# ADR-0102: Ring3 — um sandbox CPL=3 para B/C, não um OS de processos

**Data:** 2026-09-01  
**Status:** Proposed  
**Lifecycle (INDEX):** `por_fazer`  
**IDEA:** **#545**  
**Sprint:** ADR-0100 **Onda 6** (T-051–T-057) — enquadrada em **Pós-gate / Layer S** (0100 §gate); não é sprint paralelo fora do backlog  
**Evidência de auditoria:** tree 2026-09-01 (`k_nano::paging`, `gdt`, `smp/percpu`, `isolation_ring`, `user_mode`, `elf_loader`, `process`, `simd`) + SESSION_243 / 264 / 278 / 279 / 281 / 262  

**Não substitui:** ADR-0077 (canônico do ring de isolamento / F6), ADR-0059 (App Factory A/B/C), ADR-0041 (Cap P0–P9), ADR-0100 (backlog T-*), ADR-0082 HardwareInfo (canônico 0082), `0060-ring3-isolation-ring.md` (cópia histórica → **0077**).  
**Não substitui** os checklists `0082-ring3-isolation-*.md` (`conflito_id` → 0077): este documento **filtra** o que desses checklists ainda vale.  
**Corrige:** o rascunho 2026-09-01 desta mesma ADR, que propunha Fuchsia (Job/Process/Handle/ExceptionChannel) + seL4 SYSCALL fastpath como arquitetura-base (~15–23 semanas, ~2890 LOC). Esse rascunho está **rejeitado** como plano de implementação.

---

## 0. Decisão (ler primeiro)

1. **Alvo de produto = um sandbox CPL=3** para blob nativo não-confiável (ADR-0059 caminhos B/C). Agentes nativos, Hermes, Cortex, Jarbas, drivers e IRQ **permanecem CPL=0**.
2. **Theseus (PHIS) é o insight de fronteira; Fuchsia não é o modelo.** Isolamento por types Rust no código confiável; wasmi (Caminho A) para IA não-confiável; CPL=3 **só** para nativo B/C.
3. **CapGate de bitflags permanece.** Handle table Zircon, Job hierarchy e exception channels em cadeia ficam **fora** até existir um sandbox vivo em HW **e** um segundo objeto kernel que o bitflag não cubra.
4. **Syscall de produção no prazo da Onda 6 = `int 0x90` + TSS.RSP0** (SESSION_278). SYSCALL/SYSRET é projeto próprio **depois** de GDT SYSRET-compatible, `EFER.SCE`, `IA32_KERNEL_GS_BASE` e prefixo de stack na `PerCpu` — o `syscall_entry` atual **não** está a um “wire” de distância.
5. **Fault = mata o sandbox, kernel vive.** O monitor é Hermes via EventBus (`HEALTH_ISSUE`), não um processo pai em userland.
6. **Execução = Onda 6.** T-051…T-057. Honesty (H1–H3) é pré-requisito **antes** de T-052 — H2 **não** é trivial (2–5 dias; corpos dos demos perdidos no emagrecer). Estimativa realista da onda: **4–8 semanas** (§10).
7. **Emagrecer:** AS / TSS.RSP0 / `iretq` / syscall R0 em `k_nano`; CapGate em `k_hal`; seam `register_native_ring` em `hermes`; bin só facade/`pub use`. **Deletar** espelhos mortos: `neural-kernel/src/smp/percpu.rs` (diverge de `k_nano`), GDT/TSS fantasma em `interrupts_ext.rs` (`set_rsp0` sem callers na GDT nunca carregada). Não nascer `handle_table.rs` / `exception_channel.rs` / `vdso.rs` no bin.

**Invariante (igual ADR-0077):** `isolation_ring_available()==false` ⇒ IA não-confiável **só** wasmi. Não registrar `register_native_ring` até o §6 da 0077 passar em **HW** (T-053 → T-054).

---

## 1. Contexto — o que o rascunho Fuchsia errava

O neural-os-core **não** é um microkernel vazio à espera de userspace POSIX. É um AIOS monolítico: ~50 agentes nativos, EventBus, CapGate, wasmi já em produção. Fuchsia/Zircon precisa de Job/Process/Handle porque **todo** userspace é não-confiável. Aqui a maior parte do código é confiável e **tem** de ver IRQ, MMIO (negado por Cap) e o compositor.

O mapeamento “processo → thread → job = sandbox → agent → fleet” é categoria errada. Fleet é `AgentScheduler` em CPL=0. Sandbox é o anel de isolamento para **um blob**. Não são o mesmo objeto.

Theseus é o contrário do que o rascunho concluiu (“irrelevante”): confirma que CPL=3 **não** é necessário para Hermes/Cortex. É necessário **apenas** para nativo não-confiável.

---

## 2. Estado real do tree (2026-09-01)

O rascunho tratava ELF, ProcessManager, L4 isolado, `map_user_page`, `syscall_entry` e `ring3_run_native` como trabalho novo (~2890 LOC). Quase tudo **já está no disco**. O buraco é honesty de boot + ABI de silício + não ligar B/C cedo.

| Peça | Onde | Estado |
|---|---|---|
| Feature `ring3` no bin | `neural-kernel/Cargo.toml` `default = ["ring3"]` | **Não propaga** para `k-nano` (`features = ["global-alloc"]` só). `SAVED_RIP`/`SAVED_RSP` são `cfg(feature = "ring3")` na crate. Lição SESSION_264. |
| `TRY_ENTER_RING3` | `k_nano::paging` = `true` | Constante liga `iretq`. **Não** está em `user_mode.rs` (facade). |
| Demos P6 no boot | `user_mode::demo_ring3*` | **Stubs** `Ok(())` sem `iretq`. Boot loga `P6 Ring3 OK` sem entrar em CPL=3. SESSION_278 **não** é reproduzível neste path. |
| `create_sandbox_as` | `k_nano::paging` | L4 novo; copia P4[511] + HHDM com `.clone()` **sem** mascarar `USER_ACCESSIBLE` — herança acidental, não invariante (§4.5 N5). Fix: 2 linhas no clone. |
| `map_user_page` / W^X USER | `AddressSpace` + `jit_write_exec_user` | Implementado. Write via HHDM (SESSION_243). `user_arena_self_test` valida bytes, não executa em CPL=3. |
| ELF64 + `R_X86_64_RELATIVE` | `neural-kernel/src/elf_loader.rs` | Existe. `run_elf` comentado em **`user_mode.rs:36-39`**; `load_and_spawn` em `isolation_ring.rs:22-25`; terceiro path em `main.rs`. |
| `ProcessManager` | `neural-kernel/src/process.rs` | `BTreeMap` de metadata. Sem TCB, handles, canal de exceção. `switch_to_proc_tss` é **no-op** (não faz `ltr`). |
| TSS.RSP0 | `k_nano::interrupts` (GDT carregada) | SESSION_278: RSP0=0 → `#PF` no `int 0x90`. Caminho válido. |
| GDT user | `k_nano::gdt` | UCS/UDS na tabela que `lgdt` carrega. Residual: segunda GDT em `interrupts_ext.rs` (fantasma SESSION_278). |
| `syscall_entry` SYSCALL/SYSRET | `paging.rs` | Presente e **inutilizável** (§4). |
| `int 0x90` | IDT DPL=3 | Caminho SESSION_278. Manter. |
| `ring3_run_native_blob` | `k_nano::paging` via `k_hal::cap_gate` | Existe; **sem teardown** de L4/frames (§4.5 N3). |
| Sandbox state | `paging.rs` `SAVED_*` | **Global** `static mut` — não per-CPU (§4.5 N1). `MAX_SANDBOXES=1` até per-CPU-izar. |
| Syscall do blob | `int 0x90` handler | Lê `SYS_NR` de **static de kernel** (`stage_syscall`); `USE_MAILBOX` nunca `true`; sem página USER de mailbox (§4.5 N4). |
| `GS.base` em CPL=3 | `init_bsp_percpu` → `set_gs_base` | **Vaza** ponteiro `PerCpu` para user — `enter_user_mode` não zera GS (§4.5 N2). Afeta `iretq`, não só SYSCALL. |
| Vivência sandbox | `enter_user_mode` | `rflags &= !0x200` (IF=0) + `without_interrupts` → `jmp $` = BSP morto (§4.5 N6). |
| `set_rsp0` | `interrupts_ext.rs:103` | **Zero callers**; escreve em TSS da GDT fantasma — **não** no TSS carregado (`k_nano::interrupts`). Trabalho **novo** (§5). |
| `clone_current()` | `address_space.rs` + demos P4–P9 | **Não** está no path sandbox; 9 callers são demos. R3-01 “remover do sandbox” é no-op — gatear com `cap-demos` ou apagar com demos (§4.5 N7). |
| Bin `smp/percpu.rs` | `neural-kernel/src/smp/` | **Duplicata divergente** de `k_nano` (SESSION_262); `crate::smp::percpu::init_bsp_percpu` no bin é dead code. |
| `init_connectors` | `isolation_ring.rs` | Mesmo com `ring3_is_safe()` **não registra** (`register_native_ring` comentado). Porto seguro por omissão. |
| `ring3_is_safe()` | `paging.rs` | `Kvm=true`; `None` (metal) = **false**; WHPX = false. Copiar “register só KVM” **bloqueia o notebook**. |
| VA user `0x7000_0030_0000` | paging consts | Índice L4 **não** é P4[0] (é ~224). Diagramas “P4[0]=user” estão errados. |

**Compartilhar PTs do kernel (L3+) sem bit USER é correto** após `mov cr3`: o kernel precisa fetch/IDT/IST. Isolamento sandbox↔sandbox já vale porque cada um tem L4 próprio e VA user não passa por P4[511]. “Deep clone de todas as PTs do kernel” não é o próximo passo.

---

## 3. Adoção seL4 / Fuchsia / Theseus (corrigida)

| Fonte | Adotar | Rejeitar agora |
|---|---|---|
| **Theseus** | Fronteira: types no confiável; CPL=3 só no não-confiável | Intralingual como substituto de Ring3 para B/C |
| **seL4** | Ideia de syscall barato; fallback já é `int 0x90` | Fastpath SYSCALL neste tree; CSpace; endpoints; TCB direct-switch; reply cap |
| **Fuchsia/Zircon** | Nada como arquitetura-base. “Thread não trata a própria exceção” = kernel mata sandbox | Job/Process/Handle table/ExceptionChannel/preemption fair |

CapGate monolítico (~10 syscalls) **vence** CSpace e HandleRights para o prazo da Onda 6. DMA sem IOMMU = **deny-by-default** (já é política; T-057 só depois de T-055).

---

## 4. Bloqueios de silício (SYSCALL não é “Fase 3”)

Não ligar `IA32_LSTAR` como otimização até estes quatro pontos existirem **juntos**. WHPX continua recusando `wrmsr` LSTAR (SESSION_243) — fallback `int 0x90` **não** é plano B; é o plano A.

1. **`syscall_entry` vs `PerCpu`.** Sequência real (`paging.rs:655`): `swapgs` → `mov gs:[8], rsp` → `mov rsp, gs:[0]`. Assume GS = `{kstack @0, user_rsp @8}`. `PerCpu` tem `[0]=self_ptr`, `[8]=cpu_id` — colisão de layout.
2. **`GS.base` vaza para CPL=3 no caminho `iretq` (não só SYSCALL).** `init_bsp_percpu` programa `IA32_GS_BASE` com ponteiro do `PerCpu` (`smp/mod.rs:405/426`). `enter_user_mode` não zera `GS` antes do `iretq`. Blob pode `mov rax, gs:[0]` e ler/escrever kernel. Fix Onda 6: `wrmsr IA32_GS_BASE, 0` (ou seletor nulo) imediatamente antes do `iretq`. `IA32_KERNEL_GS_BASE` (`0xC0000102`) continua ausente — relevante só para SYSCALL/`swapgs`.
3. **STAR vs GDT.** SYSRET faz `CS = STAR[63:48]+16`. UCS em `0x08`/`0x10`/`0x18`/`0x20`; TSS em `0x28`. Sem CS32 dummy → SYSRET CS cai no **TSS** → `#GP`. GDT **não** é SYSRET-compatible.
4. **`EFER.SCE` não é ligado** em `init_syscall_fast_path`. Sem SCE, `SYSCALL` é `#UD`.

TSS é **por CPU** (ADR-0057 / SESSION_281). RSP0 dedicado por sandbox exige escrever no TSS **carregado** (`k_nano::interrupts::BSP_TSS_STORAGE`), não no `TSS_ARRAY` fantasma de `interrupts_ext`.

### SSE / xmm e T-056 (correção 2026-09-01)

**`CR0.EM=1` é falso neste tree.** `simd::enable_simd_ex` **limpa** `EMULATE_COPROCESSOR` e liga `OSFXSR` (`simd.rs:24-32`); chamado em `main.rs` no boot. SSE em CPL=3 **não** dá `#UD` por EM.

O risco real é **corrupção silenciosa:** `SAVED_CALLEE` salva só rbx/rbp/r12–r15 (`paging.rs:436`); **não** salva xmm6–xmm15 (callee-saved SysV). Blob com SSE clobbera xmm do chamador kernel.

**T-056 redefinido** — escolher **uma** antes de B/C:

| Opção | O quê | Custo |
|---|---|---|
| **A (recomendada)** | Verificador de opcode no blob: rejeitar SSE/AVX/AVX512 antes do `iretq` | ~80 LOC; dispensa XSAVE por chamada |
| **B** | `fxsave64`/`xrstor64` na fronteira enter/exit | ~512+ bytes + wiring |

Critério H2 “SSE `#UD`” **removido** — substituído por “demo softfloat usa blob sem SSE **ou** fronteira XSAVE”.

---

## 4.5 Gaps de corretude (não são “preempção futura”)

| ID | Gap | Evidência | Fix Onda 6 |
|---|---|---|---|
| **N1** | Estado sandbox **global** | `SAVED_RIP`/`SAVED_RSP`/`SAVED_CALLEE` = `static mut`; `without_interrupts` só local | `MAX_SANDBOXES=1` + `AtomicBool` de posse até per-CPU-izar |
| **N2** | `GS.base` kernel em CPL=3 | §4.2 | Zerar `IA32_GS_BASE` pré-`iretq`; restaurar pós-retorno |
| **N3** | Sem teardown | `ring3_run_native_blob` aloca L4+frames, não libera | `unmap`+`deallocate` em todos os caminhos (Ok/Err/fault) |
| **N4** | Blob sem syscall | Handler lê statics kernel; sem mailbox USER | 1 página USER RW `{nr, arg0, arg1, cap, result, status}` — lição SESSION_278 (não ler RAX no handler) |
| **N5** | HHDM “supervisor-only” por acidente | `create_sandbox_as` clone sem mask USER | Mascarar `USER_ACCESSIBLE` nas entradas P4[511]+HHDM copiadas |
| **N6** | Vivência: IF=0 | `rflags &= !0x200` + `without_interrupts` | **Escolher:** (a) IF=1 + auditar IRQ sob CR3 sandbox + máscara exceto timer, **ou** (b) aceite documentado de DoS por loop infinito |
| **N7** | R3-01 mal endereçado | `clone_current()` só em demos P4–P9 | `#[cfg(feature = "cap-demos")]` ou delete com demos — **não** confundir com path sandbox |

---

## 5. Plano de execução (substitui as 7 fases / 16 semanas)

Ordem canônica = **ADR-0100 Onda 6**. Itens abaixo são o filtro desta ADR sobre esses TODOs — não um segundo backlog.

### Honesty (antes de T-052 — H2 bloqueia metal)

| # | Tarefa | Estimativa | Aceite |
|---|---|---|---|
| H1 | `ring3 = ["k-nano/ring3"]` no bin (espelho SESSION_264) | **~1h** | `SAVED_RIP`/`SAVED_RSP` compilam no `k_nano` do kernel |
| H2 | Restaurar demos P6 **reais** (corpos perdidos no emagrecer — `user_mode.rs:45-48` vazios); `iretq`, fault, CapGate DMA/MMIO; T-056 opção A ou B | **2–5d** | Boot TCG: log `enter_user_mode` / fault real; **sem** H1, `SAVED_RIP=0` → `hlt` IF=0 mata BSP |
| H3 | `ring3_can_iretq()` = **self-test de boot** (`iretq` round-trip inofensivo + timeout TSC); `ring3_can_register_native()` separado; **não** tabela vendor (`ring3_is_safe` hoje: só KVM, nunca exercitado) | **~1d** | Self-test PASS em TCG/WHPX/metal; `register_native_ring` continua gated por T-053 |

**Circularidade evitada:** `can_iretq` não depende de “metal após T-052” — T-052 **valida** em notebook o que o self-test já mediu em dev.

### Sandbox (Onda 6.1–6.2)

| TODO | O que esta ADR permite | O que proíbe | Notas |
|---|---|---|---|
| T-051 | Separar `#GP` OVMF de `#GP` kernel; WHPX = `int 0x90` | Tratar WHPX como alvo de SYSCALL MSR | **Fora do caminho crítico**; flaky (SESSION_251). Não bloqueia H2/H3. |
| T-052 | `iretq`+CPL3 + fault-containment **um** notebook | Liberar B/C só com TCG | **1–3 sem** (build→Rufus→BOOT.LOG); depende Onda 2 SMP metal |
| T-053 | Checklist **0077 §6** em HW | Inventar ExceptionChannel para marcar §6 | |
| T-054 / T-055 | `register_native_ring` + HITL; `isolation_ring_available==true` **só então** | Registrar no TCG porque demo passou | |
| T-056 | Verificador opcode **ou** XSAVE na fronteira; blob B/C sem instruções SIMD até gate passar | Assumir `#UD` por `CR0.EM`; Cranelift em Ring3 antes de T-055 | **2–4d** |
| T-057 | Pin DMA **depois** de T-055; CapGate deny a CPL=3 | `SYS_MAP_FB` USER sem IOMMU | **3–5d**; frame allocator residual (SESSION_252) |

### Trabalho de kernel permitido na Onda 6 (R0/R1)

- `MAX_SANDBOXES = 1` até per-CPU-izar `SAVED_*` (N1).
- Mascarar `USER_ACCESSIBLE` em `create_sandbox_as` (N5).
- Zerar `IA32_GS_BASE` pré-`iretq`; restaurar pós-retorno (N2).
- Teardown L4 + frames em `ring3_run_native_blob` todos os caminhos (N3).
- Página USER de **mailbox** syscall + handler lê mailbox, não static kernel (N4).
- RSP0 no TSS **carregado** (`k_nano::interrupts`) — implementar `set_rsp0_live()`; **não** religar `interrupts_ext::set_rsp0` fantasma.
- Religar `run_elf` (`user_mode.rs`) + `load_and_spawn` (`isolation_ring.rs`).
- Modelo de vivência explícito: IF=1 + IRQ audit **ou** DoS documentado (N6).
- `fault_abort` → kill sandbox + `HEALTH_ISSUE`. Sem spin `SAVED_RIP=0`.
- Deletar `neural-kernel/src/smp/percpu.rs`; GDT fantasma `interrupts_ext` (TSS/RSP0 mortos).

### Explicitamente fora até 1 sandbox vivo em HW

HandleTable Zircon, ExceptionChannel em cadeia, Job/Process hierarchy, SYSCALL/SYSRET, vDSO, PCID/ASID, preemption de sandbox vs scheduler de agentes, Cranelift em Ring3, `create_sandbox_as_v2` como rename cosmética.

SYSCALL vira ADR/TODO **depois** de T-055, com os quatro itens do §4. Preemption de CPL=3 não compete com agentes no PIT ~18 Hz até haver dois sandboxes reais.

### Filtro ADR-0102 sobre checklist `0082-ring3-isolation-*` (R3-*)

| ID | Checklist 0082 | Veredito 0102 | Notas |
|---|---|---|---|
| **R3-01** Deep L4 clone | **Modificado** | L4 novo + P4[511]+HHDM com USER mascarado; compartilhar L3+ sem USER OK. Deep clone total rejeitado. `clone_current()` = demos P4–P9, não sandbox (N7). |
| **R3-02** Per-process RSP0 | **Parcial (trabalho novo)** | RSP0 no TSS **vivo**; `set_rsp0` atual é fantasma. Sem `ltr` por PID; sem `TSS_ARRAY[8]` (SESSION_279). |
| **R3-03** ELF loader mínimo | **Mantido** | `elf_loader.rs` existe; religar `load_and_spawn` / `run_elf` no path isolation. |
| **R3-04** SYSCALL/SYSRET | **Adiado** | Plano A = `int 0x90` até §4 completo + HW sem `#GP` em MSR (SESSION_243). |
| **R3-05** Sandbox + `ring3_run_native()` | **Mantido** | `ring3_run_native_blob` existe; `register_native_ring` só após T-053 em HW. |
| **R3-06** CapGate host reais | **Parcial** | Deny DMA/FB a CPL=3 (T-057); TCP/ring mínimos; sem delegation/revocation. |
| **R3-N1…N8** | **Fora** | Inalterado — fora de escopo até 1 sandbox vivo em HW. |

---

## 6. Critérios de aceite (desta ADR)

Não substituem o §6 da 0077. Somam honesty:

- [ ] H1–H3 no tree (feature propaga; P6 não é stub; predicados cindidos).
- [ ] T-051…T-057 da Onda 6, com wasmi default até T-055.
- [ ] Nenhum `handle_table` / `exception_channel` / `vdso` no bin.
- [ ] `register_native_ring` descomentado **somente** após T-053 em HW.
- [ ] SYSCALL/SYSRET **não** ligado em WHPX; em KVM/metal só após §4.

O rascunho pedia 11 critérios incluindo “ExceptionChannel”, “preemption”, “Cranelift Ring3”, “ProcessManager Zircon”. Esses **não** são aceite desta ADR.

---

## 7. Invariantes

1. `isolation_ring_available()==false` ⇒ só wasmi para IA não-confiável.
2. DMA/MMIO deny no sandbox sem IOMMU (T-057).
3. T-056: fronteira xmm segura (verificador **ou** XSAVE) — **não** `#UD` por `CR0.EM`.
4. `can_iretq` = self-test boot; `can_register` = T-053 HW. TCG/WHPX nunca registram ring.
5. `MAX_SANDBOXES = 1` até `SAVED_*` per-CPU.
6. `GS.base = 0` em CPL=3 durante execução do blob.
7. Fault em CPL=3 mata **sandbox**, não o kernel. Sem spin `SAVED_RIP=0`.
8. Agente nativo ≠ processo userland.

---

## 8. Alternativas rejeitadas

| Alternativa | Por que não |
|---|---|
| Fuchsia como arquitetura-base (rascunho 0102) | Segundo OS; agentes CPL=0 não são jobs; ~100 syscalls para ~10; sem processo pai para exception channel |
| seL4 CSpace | Formalmente pesado; CapGate cobre o gate atual |
| Deep clone de todas as PTs kernel | Custo sem ganho; USER off no subtree compartilhado basta |
| `ltr` por processo / `MAX_SANDBOXES=8` | TSS é per-CPU; teto falso |
| `ring3_is_safe()=true` em TCG como aceite | Gatilho de `register_native_ring`; viola 0077 |
| `ring3_is_safe()` vendor table como `can_iretq` | KVM nunca exercitado; metal bloqueado; usar self-test |
| Critério H2 “SSE `#UD` por CR0.EM” | **Falso** — `enable_simd()` limpa EM no boot |
| Ligar SYSCALL agora porque a naked fn existe | ABI GS/STAR/EFER quebrada; WHPX `#GP` |
| vDSO / PCID nesta onda | Zero sandboxes concorrentes medidos |

---

## 9. Referências

- Canônico Ring3: `docs/architecture/0077-ring3-isolation-ring.md` (substitui `0060-ring3-isolation-ring.md`, cópia histórica)
- Backlog: `docs/architecture/0100-k3chj-backlog-custo-anel.md` Onda 6
- Checklists filtrados: `0082-ring3-isolation-production.md`, `0082-ring3-isolation-registry.md` (subordinados à 0077)
- App Factory: ADR-0059; Cap: ADR-0041; SMP/TSS: ADR-0057
- Sessões: 243 (WHPX SYSCALL MSR), 262 (bin `smp/percpu` duplicado), 264 (feature não propaga), 278 (iretq TCG + GDT + RSP0), 279 (sem teto de cores), **281** (GDT 1 TSS/CPU)
- Código: `crates/k_nano/src/{paging.rs,gdt.rs,interrupts.rs,smp/{mod,percpu}.rs,simd.rs}`, `crates/k_hal/src/cap_gate.rs`, `crates/neural-kernel/src/{isolation_ring.rs,user_mode.rs,elf_loader.rs,process.rs,interrupts_ext.rs,smp/percpu.rs}`, `crates/hermes/src/app_factory.rs`
- SDM: `iretq`, TSS.RSP0, `int` DPL=3, SYSCALL/SYSRET STAR[+16], `IA32_EFER.SCE`, `IA32_KERNEL_GS_BASE`

---

## 10. Estimativa de prazo (honestidade vs ADR-0100 “custo L”)

ADR-0100 marca Onda 6 como **L (1–3 semanas)**. Com gaps N1–N7 e H2 não-trivial:

| Faixa | Semanas | Premissa |
|---|---|---|
| **Otimista** | 4 | H2 restaurado de SESSION_278; T-052 num notebook já validado SMP (Onda 2) |
| **Realista** | 6–8 | H2 do zero; T-052 loop HW; T-057 com frame allocator residual |
| **Cauda** | +∞ | T-051 WHPX flaky; IRQ sob CR3 sandbox (N6 opção a) |

**H2 decide se a onda é honesta** — vendido como trivial, é 2–5 dias + risco de travamento do notebook sem H1.

---

## 11. Conflitos documentais (status pós-auditoria 2026-09-01)

| # | Item | Status |
|---|---|---|
| 1 | `0082-production` “única fonte de verdade” | **Corrigido** — subordinado à 0077 + filtro 0102 |
| 2 | ADR-0041 P6 ✅ + IDEA #475 F4 validada | **Corrigido** — rebaixado para stub/honesty |
| 3 | R3-01 vs 0102 | **Corrigido** — tabela R3-* + N7 |
| 4 | `0060-ring3` stale + AGENTS.md F6→0060 | **Corrigido** |
| 5 | 0077 §6 checkbox composto | **Aberto** — granularidade T-053/054/055 na 0100 permanece mais fina |
| 6–8 | §6 T-056/057, refs 281, SESSION_278 título | **Corrigido** nesta revisão |
