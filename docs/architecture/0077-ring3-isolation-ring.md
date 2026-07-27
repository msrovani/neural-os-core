# ADR-0077: Ring3 Isolation Ring — execução nativa isolada (ex-ADR-0059 F6)

**Data:** 2026-07-26
**Status:** Proposed — **BLOQUEADOR conhecido** (habilitar hoje = triple-fault → reboot loop). NÃO habilitado; kernel em **porto seguro** (ver §4).
**Lifecycle (INDEX):** `pesquisa`
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

- **Bricar o boot:** hoje habilitar = **triple-fault → reboot loop** (§3). Qualquer erro (page tables, TSS/IST, GDT, iretq, CR3, syscall) → reset/corrupção.
- **Complexidade/manutenção:** AS por-sandbox, TSS RSP0 + **IST** por vetor, ABI de syscall, save/restore, ciclo de vida (spawn/kill/timeout/quota).
- **Overhead:** troca de CR3 (flush TLB sem PCID), transições de anel, marshaling — pode superar o ganho em tarefas curtas.
- **Divergência QEMU×HW** (TCG/KVM/WHPX/HW): pode ficar flaky → gating por hypervisor.
- **Meio-feito é MAIS perigoso que nenhum:** confiaria em algo com bug de gate/paginação → escalonamento de privilégio.
- **Soft-float/sem-SSE:** JIT nativo deve respeitar (Cranelift default pode emitir SSE → `#UD`).
- **DMA/IOMMU:** CPL=3 **não** barra DMA; sem IOMMU, caps de DMA ficam **negadas** mesmo com Ring3.

### Conclusão estratégica

O Caminho A (wasmi) já entrega a **funcionalidade** "IA cria app em runtime" com segurança. O F6 agrega **performance nativa** e a **trilha self-hosting (C)** — não é pré-requisito funcional. Por isso: **F3/F4 (wasmi) primeiro**; F6 como projeto de kernel dedicado.

## 3. Diagnóstico confirmado

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
| `isolation_ring_available()` | **`false`** (nenhum ring nativo registrado) | `crates/hermes/src/app_factory.rs` |
| Caminhos B/C nativo | **gated** (`AWAITING_ISOLATION`) | `app_factory::execute` |
| Código de IA não-confiável | roda **só no wasmi** (sandbox software) | `crates/hermes/src/wasmi_rt.rs` |
| `exec_arena` (F7) | roda **só código próprio/confiável** em Ring 0 | `neural-kernel/src/exec_arena.rs` |
| `isolation_ring::init_connectors` | loga diagnóstico mas **não registra** ring | `neural-kernel/src/isolation_ring.rs` |
| Boot QEMU | 8 fases + scheduler vivo + sem panic | evidência em `logs/` |

**Invariante:** enquanto **nenhum** "ring nativo" for registrado no seam (§5) e `TRY_ENTER_RING3=false`, o kernel **não executa nenhum código nativo não-confiável** — o pior caso é wasmi (sandbox) ou negação honesta. **Não regredir esse invariante** ao atacar o F6.

## 5. Conectores no código (seams para quando atacarmos o F6)

Deixados prontos para a implementação futura plugar sem refatorar:

1. **Seam de registro (hermes):** `hermes::app_factory::register_native_ring(fn)` + `native_ring_registered()`. `isolation_ring_available()` passa a refletir **se um ring nativo validado foi registrado**. `execute()` dos Caminhos B/C chama o ring registrado; sem registro → `AWAITING_ISOLATION` (comportamento atual).
2. **Módulo de implementação (neural-kernel):** `neural-kernel/src/isolation_ring.rs` — `init_connectors()` (chamado no boot; **hoje NÃO registra** — ring não pronto) e `ring3_run_native()` (site futuro da execução isolada; hoje retorna `Err("F6: Ring3 não implementado")`). Documenta os blocos a reusar: `exec_arena` (W^X codegen), `user_mode::enter_user_mode` (iretq), `address_space` (AS/CR3), `syscall`/`capability_gate` (gate), `interrupts` (GDT user + TSS/IST + handlers).
3. **Flag de experimentação:** `TRY_ENTER_RING3` (bool) permanece o interruptor do `iretq` real para a sessão de debug.

Quando o F6 estiver validado (§6), `isolation_ring::init_connectors()` chamará `register_native_ring(ring3_run_native)` → `isolation_ring_available()` vira `true` → B/C liberados (sob HITL).

## 6. Critérios de aceite (gate para habilitar — não ligar meio-pronto)

- [ ] **iretq estável:** blob nativo roda em **CPL=3** e retorna ao kernel limpo (sem #PF após `mov cr3`).
- [ ] **AS isolado:** kernel mapeado **supervisor-only** no AS do sandbox; sandbox só acessa suas páginas + página de gate; kernel text + IDT + handlers de falta + **IST** alcançáveis para tratar traps de CPL=3.
- [ ] **Contenção de falta:** #PF/#GP/#UD **forçados** no sandbox → **mata o sandbox, kernel continua** (não triple-fault, não halt global).
- [ ] **Syscall gate:** ABI (registrador ou página dedicada) mediada por **CapGate**; sem cap → deny.
- [ ] **DMA/MMIO negados:** caps `PIN_DMA`/`MAP_FB`/MMIO **negadas** ao sandbox (sem IOMMU).
- [ ] **Soft-float:** JIT nativo sem SSE (respeita a config do kernel) ou trap tratado.
- [ ] **Gating por hypervisor:** se instável em WHPX/HW, `isolation_ring_available()` fica `false` naquele ambiente.
- [ ] `cargo check --release` 0 erros; **boot QEMU sem reboot loop**; self-test do ring PASS.

Só com **todos** ✅ → `register_native_ring(...)` → B/C nativo liberado (HITL forte).

## 7. Plano (sessão dedicada, modo depurador)

1. Reproduzir o triple-fault com instrumentação (log pré/pós `mov cr3`, dump CR2/err/RIP, checar IST/TSS).
2. Corrigir o AS do sandbox: mapear kernel higher-half como supervisor-only + garantir IDT/handlers/IST no clone; considerar AS dedicado (não clone raso) com só o necessário.
3. Fault handlers com recuperação por-sandbox (reusar `user_mode::fault_abort`, sem storm).
4. Gate de syscall real (substituir staging por atomics).
5. `exec_arena` → emitir em página **user** RX no AS do sandbox (não Ring 0).
6. Self-test: rodar blob que faz syscall gated + provocar falta → sandbox morto, kernel vivo.
7. Passar §6 → registrar o ring → liberar B/C.

## 8. Referências

- Código/seams: `crates/hermes/src/app_factory.rs`, `crates/neural-kernel/src/{isolation_ring,exec_arena,user_mode,address_space,syscall,capability_gate,interrupts}.rs`.
- ADRs: ADR-0041 (capability rings P0–P9), ADR-0059 (Runtime App Factory; A/F7), ADR-0052 (contrato de artefato).
- Externos: rustc-lite/ClaudioOS (self-hosting bare-metal), MCP-SandboxScan (sandbox WASM), Intel SDM (Ring3/TSS/IST/iretq).
