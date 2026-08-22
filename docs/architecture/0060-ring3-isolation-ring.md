# ADR-0060: Ring3 Isolation Ring — execução nativa isolada (ex-ADR-0059 F6)

**Data:** 2026-07-22
**Status:** Proposed — **BLOQUEADOR conhecido** (habilitar hoje = triple-fault → reboot loop). NÃO habilitado; kernel em **porto seguro** (ver §4).
**Lifecycle (INDEX):** `pesquisa`
**Extraído de:** ADR-0059 §F6 (esta ADR é o lar dedicado do "ring de isolamento").
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
- **Bricar o boot:** hoje habilitar = **triple-fault → reboot loop** (§3). Qualquer erro (page tables, TSS/IST, GDT, iretq, CR3, syscall) → reset/corrupção.
- **Complexidade/manutenção:** AS por-sandbox, TSS RSP0 + **IST** por vetor, ABI de syscall, save/restore, ciclo de vida (spawn/kill/timeout/quota).
- **Overhead:** troca de CR3 (flush TLB sem PCID), transições de anel, marshaling — pode superar o ganho em tarefas curtas.
- **Divergência QEMU×HW** (TCG/KVM/WHPX/HW): pode ficar flaky → gating por hypervisor.
- **Meio-feito é MAIS perigoso que nenhum:** confiaria em algo com bug de gate/paginação → escalonamento de privilégio.
- **Soft-float/sem-SSE:** JIT nativo deve respeitar (Cranelift default pode emitir SSE → `#UD`).
- **DMA/IOMMU:** CPL=3 **não** barra DMA; sem IOMMU, caps de DMA ficam **negadas** mesmo com Ring3.

### Conclusão estratégica
O Caminho A (wasmi) já entrega a **funcionalidade** "IA cria app em runtime" com segurança. O F6 agrega **performance nativa** e a **trilha self-hosting (C)** — não é pré-requisito funcional. Por isso: **F3/F4 (wasmi) primeiro**; F6 como projeto de kernel dedicado.

## 3. Diagnóstico confirmado (2026-07-22)

Habilitando `TRY_ENTER_RING3=true` (teste isolado, revertido):
- Boot entra em **reboot loop** — "P6 Ring3 user-mode demo" + "Cap::ENTER_USER deny OK" repetem 3× no log, **sem chegar ao scheduler**.
- O `iretq` real (`user_mode::enter_user_mode`) faz **`#PF err=0x10`** (supervisor, instruction-fetch, not-present) logo após `mov cr3, {user_l4}`.
- O próprio handler de #PF **não roda** sob o CR3 do user (kernel text/IDT/handler/IST não confiavelmente mapeados no clone raso) → **double → triple fault → reset**.

**Causa-raiz (hipótese forte):** o `AddressSpace::clone_current()` é um **clone raso do L4** — herda entradas do kernel, mas ao trocar de CR3 antes do `iretq`, as instruções seguintes / o vetor de #PF / a pilha IST não estão garantidamente presentes e alcançáveis no novo AS.

## 4. Porto seguro do kernel — por que está OK HOJE

O kernel está **estável e seguro** justamente porque o Ring3 está **desligado por design**:

| Salvaguarda | Estado | Onde |
|---|---|---|
| `TRY_ENTER_RING3` | **`false`** (iretq real nunca executa) | `neural-kernel/src/user_mode.rs` |
| `isolation_ring_available()` | **`false`** (nenhum ring nativo registrado) | `hermes/src/app_factory.rs` |
| Caminhos B/C nativo | **gated** (`AWAITING_ISOLATION`) | `app_factory::execute` |
| Código de IA não-confiável | roda **só no wasmi** (sandbox software) | `hermes/src/wasmi_rt.rs` |
| Demo P6 no boot | **non-fatal** (só testa Cap deny + skip iretq) | `user_mode::demo_ring3` |
| `exec_arena` (F7) | roda **só código próprio/confiável** em Ring 0 | `neural-kernel/src/exec_arena.rs` |
| Boot QEMU | 8 fases + scheduler vivo + sem panic | evidência em `logs/` |

**Invariante:** enquanto **nenhum** "ring nativo" for registrado no seam (§5) e `TRY_ENTER_RING3=false`, o kernel **não executa nenhum código nativo não-confiável** — o pior caso é wasmi (sandbox) ou negação honesta. **Não regredir esse invariante** ao atacar o F6.

## 5. Conectores no código (seams para quando atacarmos o F6)

Deixados prontos para a implementação futura plugar sem refatorar:

1. **Seam de registro (hermes):** `hermes::app_factory::register_native_ring(fn)` + `native_ring_registered()`. `isolation_ring_available()` passa a refletir **se um ring nativo validado foi registrado**. `execute()` dos Caminhos B/C chama o ring registrado; sem registro → `AWAITING_ISOLATION` (comportamento atual).
2. **Módulo de implementação (neural-kernel):** `neural-kernel/src/isolation_ring.rs` — `init_connectors()` (chamado no boot; **hoje NÃO registra** — ring não pronto) e `ring3_run_native()` (site futuro da execução isolada; hoje retorna `Err("F6: Ring3 não implementado")`). Documenta os blocos a reusar: `exec_arena` (W^X codegen), `user_mode::enter_user_mode` (iretq), `address_space` (AS/CR3), `syscall`/`capability_gate` (gate), `interrupts` (GDT user + TSS/IST + handlers).
3. **Flag de experimentação:** `TRY_ENTER_RING3` (bool) permanece o interruptor do `iretq` real para a sessão de debug.

Quando o F6 estiver validado (§6), `isolation_ring::init_connectors()` chamará `register_native_ring(ring3_run_native)` → `isolation_ring_available()` vira `true` → B/C liberados (sob HITL).

## 6. Critérios de aceite (gate para habilitar — não ligar meio-pronto)

Evidência SESSION_278+ (QEMU **TCG**, NoDisk): `SUCCESS iretq+CPL3` + fault-containment + **CapGate DMA/MMIO deny** + **soft-float SSE #UD**. **B/C ainda NÃO liberados** — `ring3_is_safe` só KVM; WHPX/HW não medidos.

- [x] **iretq estável (TCG):** blob demo em **CPL=3** + `int 0x90` EXIT + retorno kernel (SESSION_278). WHPX/HW não medidos nesta sessão.
- [x] **AS isolado (parcial):** sandbox `create_sandbox_as` = P4[511] + P4[HHDM] supervisor-only + páginas USER dedicadas (não clone raso). HHDM supervisor é necessário ao kernel com CR3 sandbox; CPL=3 não tem USER no HHDM.
- [x] **Contenção de falta (TCG):** stub força #PF em CPL=3 → `fault_abort` → kernel vivo (`demo_ring3_fault_containment`).
- [~] **Syscall gate:** Cap::ENTER_USER deny OK + EXIT via `int 0x90`; mailbox CPL=3 para nr (prologue clobbera RAX).
- [x] **DMA/MMIO negados (TCG):** `dispatch` deny-by-default `PIN_DMA`/`MAP_DMA`/`MAP_FB`/`PRESENT_FB` enquanto sandbox ativo; stub CPL=3 `SUCCESS CapGate sandbox deny PIN_DMA=1 MAP_FB=1`.
- [x] **Soft-float (TCG):** CR0.EM=1 + `xorps` em CPL=3 → `#UD` → `fault_abort` (`demo_ring3_softfloat_sse`).
- [x] **Gating por hypervisor (porto seguro):** `ring3_is_safe()` permanece **só KVM**; TCG/WHPX não registram `register_native_ring` mesmo com `TRY_ENTER_RING3=true`.
- [x] `cargo build --release` + **boot QEMU TCG sem reboot loop**; self-tests P6 PASS.

Só com **WHPX/HW** + ABI syscall completa + estes ✅ → `register_native_ring(...)` → B/C nativo (HITL forte).

## 7. Plano (sessão dedicada, modo depurador)

1. Reproduzir o triple-fault com instrumentação (log pré/pós `mov cr3`, dump CR2/err/RIP, checar IST/TSS).
2. Corrigir o AS do sandbox: mapear kernel higher-half como supervisor-only + garantir IDT/handlers/IST no clone; considerar AS dedicado (não clone raso) com só o necessário.
3. Fault handlers com recuperação por-sandbox (reusar `user_mode::fault_abort`, sem storm).
4. Gate de syscall real (substituir staging por atomics).
5. `exec_arena` → emitir em página **user** RX no AS do sandbox (não Ring 0).
6. Self-test: rodar blob que faz syscall gated + provocar falta → sandbox morto, kernel vivo.
7. Passar §6 → registrar o ring → liberar B/C.

## 8. Referências

- Código/seams: `hermes/src/app_factory.rs`, `neural-kernel/src/{isolation_ring,exec_arena,user_mode,address_space,syscall,capability_gate,interrupts}.rs`.
- ADRs: ADR-0041 (capability rings P0–P9), ADR-0059 (Runtime App Factory; A/F7), ADR-0052 (contrato de artefato).
- Externos: rustc-lite/ClaudioOS (self-hosting bare-metal), MCP-SandboxScan (sandbox WASM), Intel SDM (Ring3/TSS/IST/iretq).
