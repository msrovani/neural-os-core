# ADR-0077: Ring3 Isolation Ring — execução nativa isolada (ex-ADR-0059 F6)

**Data:** 2026-07-26
**Status:** Proposed — **B/C nativo ainda gated**. Demos P6 TCG (SESSION_278): `TRY_ENTER_RING3=true`, iretq+CPL3 + fault-containment PASS **nessa sessão**. WHPX/HW e `register_native_ring` **não** liberados. Sem Ring3, código de IA não-confiável continua só wasmi.
**Lifecycle (INDEX):** `fazendo`
**Filtro de execução / recusa process-OS:** **ADR-0102** (#545). O §6 desta ADR **não muda**. Plano Fuchsia Job/Handle/ExceptionChannel (rascunho 0102) está rejeitado.
**Extraído de:** ADR-0059 §F6 (esta ADR é o lar dedicado do "ring de isolamento").
**Nota sobre o número:** Originalmente proposto como ADR-0060 no PR #5, mas `0060` já estava alocado para BEI (BitNet Ecosystem Intelligence). Renumerado para 0077.
**Depende de / reusa:** ADR-0041 (Ring3/AS/CR3/int 0x90 PoC), ADR-0059 F7 (`exec_arena` W^X — codegen nativo).
**Destrava:** ADR-0059 Caminhos **B** (Cranelift JIT wasm→nativo) e **C** (Rust-subset nativo, à la rustc-lite). Sem esta ADR, B/C ficam **gated** (execução nativa negada).

---

## 1. Contexto e objetivo

A ADR-0059 executa apps de IA de forma segura pelo **Caminho A (wasmi)** — sandbox por software, já em produção. Os Caminhos **B/C** geram **código nativo** (via Cranelift), que roda com **privilégio de kernel** se não houver isolamento de hardware. Executar código de IA (não-confiável) em nativo **sem um ring de isolamento** seria um furo catastrófico.

**Objetivo desta ADR:** especificar e (futuramente) implementar um **ring de isolamento Ring3** que permita rodar um blob nativo em **CPL=3**, com:

- **isolamento de memória** (kernel supervisor-only; sandbox só vê suas páginas + gate),
- **gate de syscall** mediado por **CapGate**,
- **contenção de falta** ("mata o sandbox, não o kernel"),
- **caps de DMA/MMIO negadas** (sem IOMMU, DMA é fora de escopo).

Só quando esses critérios (§6) passarem, `isolation_ring_available()` vira `true` e B/C nativo é liberado (sob HITL forte).

## 2. Ganhos × perdas de habilitar o F6

### Ganhos

- **Destrava B/C** (código nativo seguro): RustCoder → Cranelift → nativo em sandbox.
- **Isolamento por hardware (CPL=3):** sandbox não toca memória do kernel nem executa instrução privilegiada — mais forte que o SFI por software do wasmi.
- **Performance nativa** (~5–10× vs. interpretador wasmi) para skills pesadas.
- **Contenção de falta real** + base de modelo de processo/multi-app.
- **Syscall gated com least-privilege** + **trilha self-hosting** (Caminho C / self-update nativo).

### Perdas / riscos

- **Bricar o boot:** erro em page tables, TSS/IST, GDT, iretq, CR3 ou syscall ainda pode resetar. SESSION_278 fechou o loop TCG conhecido; WHPX/HW **não** têm o mesmo aceite.
- **Complexidade/manutenção:** AS por-sandbox, TSS RSP0 + **IST** por vetor, ABI de syscall, save/restore, ciclo de vida (spawn/kill/timeout/quota).
- **Overhead:** troca de CR3 (flush TLB sem PCID), transições de anel, marshaling — pode superar o ganho em tarefas curtas.
- **Divergência QEMU×HW** (TCG/KVM/WHPX/HW): pode ficar flaky → gating por hypervisor.
- **Meio-feito é MAIS perigoso que nenhum:** confiaria em algo com bug de gate/paginação → escalonamento de privilégio.
- **Soft-float/sem-SSE:** JIT nativo deve respeitar (Cranelift default pode emitir SSE → `#UD`).
- **DMA/IOMMU:** CPL=3 **não** barra DMA; sem IOMMU, caps de DMA ficam **negadas** mesmo com Ring3.

### Conclusão estratégica

O Caminho A (wasmi) já entrega a **funcionalidade** "IA cria app em runtime" com segurança. O F6 agrega **performance nativa** e a **trilha self-hosting (C)** — não é pré-requisito funcional. Por isso: **F3/F4 (wasmi) primeiro**; F6 como projeto de kernel dedicado.

## 3. Diagnóstico (histórico) + SESSION_278

Habilitar `TRY_ENTER_RING3` **antes** do HHDM no sandbox AS + GDT user na GDT **carregada** + TSS.RSP0 dava triple-fault (`#PF` pós-`mov cr3`). SESSION_278 corrigiu isso em **QEMU TCG NoDisk**. O texto antigo de “reboot loop hoje” **não** descreve o tree pós-278.

**Ainda aberto:** WHPX `#GP` OVMF (não é o mesmo bug); `ring3_is_safe` só KVM; B/C HITL; `isolation_ring_available()=false`.

## 4. Porto seguro do kernel — por que está OK HOJE

O kernel **não** libera B/C nativo. Demos P6 em TCG usam `TRY_ENTER_RING3=true` (SESSION_278). Invariante de produto: **wasmi** para IA não-confiável até `register_native_ring` + §6 completo em HW.

| Salvaguarda | Estado | Onde |
|---|---|---|
| `TRY_ENTER_RING3` | **`true`** (constante em `k_nano::paging`; facade em `user_mode.rs`) | `crates/k_nano/src/paging.rs` |
| Demos P6 no boot (2026-09-01) | **Stubs** `Ok(())` — boot loga OK **sem** `iretq`. SESSION_278 é evidência histórica, não o path atual. Honesty = ADR-0102 H2 | `neural-kernel/src/user_mode.rs` |
| `isolation_ring_available()` | **`false`** (nenhum ring nativo registrado) | `crates/hermes/src/app_factory.rs` |
| Caminhos B/C nativo | **gated** (`AWAITING_ISOLATION`) | `app_factory::execute` |
| Código de IA não-confiável | roda **só no wasmi** | `crates/hermes/src/wasmi_rt.rs` |
| `exec_arena` (F7) | roda **só código próprio/confiável** em Ring 0 | `neural-kernel/src/exec_arena.rs` |
| `isolation_ring::init_connectors` | loga diagnóstico; `register_native_ring` **comentado** mesmo se `ring3_is_safe()` | `neural-kernel/src/isolation_ring.rs` |
| `ring3_run_native` | delega a `ring3_run_native_blob` (W^X USER + `enter_user_mode`); **não** é mais `Err("não implementado")` | `k_hal::cap_gate` → `k_nano::paging` |
| Boot QEMU | 8 fases + scheduler vivo | evidência em `logs/` (P6 atual ≠ SESSION_278) |

**Invariante:** `isolation_ring_available()==false` (nenhum ring nativo registrado) ⇒ IA não-confiável **só** wasmi. `TRY_ENTER_RING3=true` no tree **não** é liberação de B/C. Demos P6 no boot, em 2026-09-01, são stubs — não tratar log `P6 Ring3 OK` como evidência (ADR-0102 H2). **Não** registrar `register_native_ring` até §6 passar em HW.

## 5. Conectores no código (seams para quando atacarmos o F6)

Deixados prontos para a implementação futura plugar sem refatorar:

1. **Seam de registro (hermes):** `hermes::app_factory::register_native_ring(fn)` + `native_ring_registered()`. `isolation_ring_available()` passa a refletir **se um ring nativo validado foi registrado**. `execute()` dos Caminhos B/C chama o ring registrado; sem registro → `AWAITING_ISOLATION` (comportamento atual).
2. **Módulo de implementação:** `neural-kernel/src/isolation_ring.rs` — `init_connectors()` (boot; **não registra** — linha `register_native_ring` comentada mesmo em KVM) e `ring3_run_native()` (delega a `k_nano::paging::ring3_run_native_blob`). Reusar: `enter_user_mode` / `create_sandbox_as` / `jit_write_exec_user` em `k_nano::paging`; CapGate em `k_hal`; GDT user + TSS.RSP0 em `k_nano::{gdt,interrupts}`. ELF: `elf_loader.rs` já parseia; o wire `load_and_spawn` está comentado (ADR-0102).
3. **Flag de experimentação:** `TRY_ENTER_RING3` em `k_nano::paging` (bool) permanece o interruptor do `iretq` real. Demos no bin têm de **chamar** esse path — stub `Ok(())` não conta (ADR-0102 H2).

Quando o F6 estiver validado (§6), `isolation_ring::init_connectors()` chamará `register_native_ring(ring3_run_native)` → `isolation_ring_available()` vira `true` → B/C liberados (sob HITL).

## 6. Critérios de aceite (gate para habilitar — não ligar meio-pronto)

- [x] **iretq estável (TCG):** SESSION_278 `SUCCESS iretq+CPL3` NoDisk. Metal/WHPX aberto.
- [x] **Contenção de falta (TCG):** fault-containment demo PASS.
- [x] **DMA/MMIO negados (demo):** CapGate sandbox deny PIN_DMA/MAP_FB (SESSION_278).
- [x] **Soft-float SSE:** `#UD` contido no demo → **redefinido** `0102 §4.5 T-056` (verificador opcode **ou** XSAVE; `CR0.EM=0` no tree `simd.rs:25`).
- [ ] **AS isolado** — `create_sandbox_as` L4 novo + P4[511]+HHDM com `USER_ACCESSIBLE` mascarado (`paging.rs:267-273`); sem deep clone total — **T-053** (`0100`).
- [ ] **Syscall gate** — `int 0x90` DPL=3 + mailbox USER `0x7000_0030_2000` (`ring3.rs:14`/`paging.rs:299`) + CapGate deny DMA/FB — **T-053**.
- [ ] **WHPX+HW** — `iretq+CPL3` estável **metal** (T-052) e WHPX separado de `#GP` OVMF (T-051) — não só TCG.
- [ ] **`register_native_ring`** — `HW_GATE_PASSED && can_iretq && probe_done && hv==None` (`ring3.rs:35-43`) + HITL Escalate → `isolation_ring_available()==true` — **T-054/T-055** (`0100`).

> Granularidade `0102 §11.5` fechada: o checkbox composto foi cindido em 4 — espelha `0100` T-053/054/055. `0102 §10` `L → 4/6-8/cauda ∞` já reflete H2+T-052 como cauda.

Só com **todos** ✅ → `register_native_ring(...)` → B/C nativo liberado (HITL forte).

## 7. Plano (execução)

O debug-loop histórico (reproduzir triple-fault → AS → fault → syscall → arena USER → §6) **já teve o trecho TCG** na SESSION_278. A ordem vigente é **ADR-0100 Onda 6** filtrada por **ADR-0102**:

1. Honesty: feature `ring3` propaga ao `k_nano`; destubar P6; cisão `can_iretq` / `can_register`.
2. T-051 WHPX vs `#GP` OVMF; T-052 metal um notebook.
3. T-053 §6 desta ADR em HW → T-054 `register_native_ring` + HITL → T-055.
4. T-056 JIT sem SSE; T-057 pin DMA só depois.
5. **Não** HandleTable / ExceptionChannel / SYSCALL/SYSRET nesta onda (0102 §4–§5).

## 8. Referências

- Código/seams: `crates/hermes/src/app_factory.rs`, `crates/neural-kernel/src/{isolation_ring,exec_arena,user_mode,address_space,syscall,capability_gate,interrupts}.rs`.
- ADRs: ADR-0041 (capability rings P0–P9), ADR-0059 (Runtime App Factory; A/F7), ADR-0052 (contrato de artefato), **ADR-0102** (filtro sandbox vs process-OS; honesty tree), ADR-0100 Onda 6 (T-051–T-057).
- Externos: rustc-lite/ClaudioOS (self-hosting bare-metal), MCP-SandboxScan (sandbox WASM), Intel SDM (Ring3/TSS/IST/iretq).
