# ADR-0082: Ring3 Isolation — Registro Completo de Itens

> **Conflito de ID:** `0082` canônico no INDEX é **HardwareInfo** (`0082-hardware-info-registry.md`). Este registro e `0082-ring3-isolation-production.md` são o **checklist Ring3** (execução → ADR-0077). Não misturar com HardwareInfo.

**Data:** 2026-08-02  
**Fonte:** `docs/architecture/0082-ring3-isolation-production.md`  
**Status:** Proposed — registro estruturado para execução e rastreamento

---

## 1. Itens Principais (MVP Scope — Must Have)

### Item 1: Deep L4 Clone — Address Space Isolation Real
| Campo | Valor |
|-------|-------|
| **ID** | R3-01 |
| **Sub-itens** | 1.1 `create_sandbox_as()` from-scratch<br>1.2 Copiar só P4[511] (kernel text) + P4[256..510] (HHDM)<br>1.3 User VAs (P4[0..255]) vazias — mapeadas sob demanda<br>1.4 Remover `clone_current()` para Ring3 (shallow clone = compartilha PTs kernel) |
| **Objetivo** | Isolamento real de address space: sandbox AS não compartilha page tables inferiores com kernel |
| **Fundamento** | ADR-0041 §7: "Shallow L4 compartilha PageTables inferiores do kernel" — known limitation. Produção exige deep clone. |
| **Conexões** | `address_space.rs` (336 LOC), `user_mode.rs` (enter_user_mode recebe AS), `isolation_ring.rs` (sandbox AS) |
| **Goal** | `create_sandbox_as()` retorna AS isolado; `demo_ring3()` roda em sandbox AS; acesso a kernel VA → #GP contido |

---

### Item 2: Per-Process Kernel Stacks (RSP0)
| Campo | Valor |
|-------|-------|
| **ID** | R3-02 |
| **Sub-itens** | 2.1 TSS array per-CPU + per-process<br>2.2 `set_rsp0(process_id)` no context switch<br>2.3 `enter_user_mode()` recebe `rsp0` dedicado<br>2.4 Remover `ponytail: single kernel stack` (user_mode.rs:292) |
| **Objetivo** | Cada processo Ring3 tem stack kernel própria para trap CPL=3→0 |
| **Fundamento** | Single TSS com RSP0 mutável = race condition em SMP; `interrupts.rs` já tem `TSS_ARRAY` esqueleto |
| **Conexões** | `interrupts.rs` (477 LOC), `user_mode.rs`, scheduler futuro |
| **Goal** | Context switch troca RSP0; 2 processos Ring3 simultâneos não corrompem stack |

---

### Item 3: ELF Loader Mínimo (Userland)
| Campo | Valor |
|-------|-------|
| **ID** | R3-03 |
| **Sub-itens** | 3.1 Parse ELF64: program headers `PT_LOAD`<br>3.2 Mapear `.text` (RX), `.data`/`.bss` (RW)<br>3.3 Relocations: `R_X86_64_RELATIVE` (PIE) + `R_X86_64_64`<br>3.4 Retornar `entry_point`, `stack_top`, `phdrs` mapeados<br>3.5 Self-test: ELF "hello world" compila + roda |
| **Objetivo** | Carregar binários userland reais (não stub hardcoded) |
| **Fundamento** | `user_mode.rs` hoje usa stub assembly fixo. WASM B/C e apps nativos precisam de loader real. |
| **Conexões** | `user_mode.rs` (`run_elf`), `address_space.rs` (mapeia segments), `isolation_ring.rs` (`ring3_run_native`) |
| **Goal** | `run_elf("hello.elf")` → processo Ring3 imprime "Hello from Ring3" via syscall |

---

### Item 4: SYSCALL/SYSRET Fast Path
| Campo | Valor |
|-------|-------|
| **ID** | R3-04 |
| **Sub-itens** | 4.1 MSR `IA32_LSTAR` (entry), `IA32_STAR` (segmentos), `IA32_FMASK` (RFLAGS mask)<br>4.2 Preservar ABI registrador: RAX=nr, RDI/RSI/RDX/R10/R8/R9=args<br>4.3 `int 0x90` mantido como fallback/compat<br>4.4 `syscall.rs`: `syscall_entry` assembly + `dispatch_syscall()` Rust |
| **Objetivo** | Syscall rápida (100-200 ciclos vs ~1000 de `int 0x90`) |
| **Fundamento** | `int 0x90` é PoC. Produção exige SYSCALL/SYSRET. WHPX/TCG/KVM suportam. |
| **Conexões** | `syscall.rs` (340 LOC), `interrupts.rs` (IDT), `user_mode.rs` (retorno) |
| **Goal** | `rdtsc` antes/depois de syscall mostra <300 ciclos; `int 0x90` ainda funciona para compat |

---

### Item 5: Sandbox AS + `ring3_run_native()` Funcional
| Campo | Valor |
|-------|-------|
| **ID** | R3-05 |
| **Sub-itens** | 5.1 `ring3_run_native(code, caps)` → compila/valida → `run_elf()` em sandbox AS<br>5.2 Fault isolado (acesso kernel VA) → #GP → processo morto, kernel vivo<br>5.3 `isolation_ring_available()` retorna `true` quando sandbox pronto<br>5.4 Integração `hermes::app_factory::register_native_ring()` |
| **Objetivo** | Isolamento real para código nativo não-confiável (WASM B/C) |
| **Fundamento** | `isolation_ring.rs` linha 74: `Err("não implementado")`. WASM B/C gated por isso. |
| **Conexões** | `isolation_ring.rs`, `user_mode.rs`, `address_space.rs`, `hermes/app_factory.rs` |
| **Goal** | `ring3_run_native(add_wasm, Cap::ENTER_USER)` → executa `fn add(a,b) -> a+b` em Ring3 → retorna 42 |

---

### Item 6: CapGate Host Functions Reais
| Campo | Valor |
|-------|-------|
| **ID** | R3-06 |
| **Sub-itens** | 6.1 `aios_send_tcp` → `net::send()`<br>6.2 `aios_write_ring` → `ring::write()`<br>6.3 `aios_map_fb` → `fb::map()`<br>6.4 `aios_pin_dma` → `dma::pin()`<br>6.5 `aios_map_file` → `fs::mmap()`<br>6.6 Validação Cap + serial DENY/ALLOW log |
| **Objetivo** | Host functions funcionais para sandbox (não stubs `Ok(0)`) |
| **Fundamento** | `capability_gate.rs` linhas 78-109: todos stubs. Sandbox precisa de I/O real. |
| **Conexões** | `capability_gate.rs` (110 LOC), `syscall.rs` (dispatch), backends k-nano/k-hal |
| **Goal** | Sandbox chama `aios_send_tcp` → pacote sai na NIC; `aios_map_fb` → framebuffer mapeado USER |

---

## 2. Itens Fora de Escopo (Explicitamente Nice-to-Have)

| ID | Item | Motivo |
|----|------|--------|
| R3-N1 | Multi-threaded user processes (TLS, futex, pthreads) | Complexidade O(n²); sem consumidor imediato |
| R3-N2 | Signals (SIGSEGV, SIGKILL, etc.) | Requer scheduler + process group |
| R3-N3 | Swap / page eviction / huge pages | Demand paging atual é pre-filled; swap = infra separada |
| R3-N4 | Capability delegation / attenuation / revocation | Cap bitflags suficientes para MVP |
| R3-N5 | ASID/PCID / PKU | x86_64 baseline funciona sem; otimização futura |
| R3-N6 | vDSO | SYSCALL/SYSRET já resolve latência |
| R3-N7 | IOMMU / DMA isolation per-process | Requer HW + k-HAL ownership; pós-v2.0 |
| R3-N8 | Streaming GGUF mmap | Parser GGUF existe; integração = item separado |

---

## 3. Fases de Implementação (Detalhado)

### Fase 1: Fundação (Semanas 1-2)

| Task ID | Arquivo | Ação | Verificação |
|---------|---------|------|-------------|
| F1.1 | `address_space.rs` | `create_sandbox_as()` from-scratch; `copy_kernel_mappings()` | `demo_as_r1_r3_shallow()` adaptado usa sandbox AS |
| F1.2 | `interrupts.rs` | `TSS_ARRAY: [TaskStateSegment; MAX_PROCS]`; `set_rsp0(pid)` | 2 processos Ring3 → RSP0 distintos |
| F1.3 | `user_mode.rs` | Fix `clone_current()` bug (linha 313); `enter_user_mode(aspace, rsp0)` | `demo_ring3()` roda sem crash higher-half |
| F1.4 | `syscall.rs` | `syscall_entry` asm (SYSCALL); `IA32_LSTAR/STAR/FMASK` init | `rdtsc` syscall <300 ciclos |

**Otimização F1:** `copy_kernel_mappings()` usa `memcpy` de PTEs (não walk recursivo) — ~500ns vs ~5µs.

---

### Fase 2: ELF Loader + Sandbox (Semanas 3-5)

| Task ID | Arquivo | Ação | Verificação |
|---------|---------|------|-------------|
| F2.1 | `elf_loader.rs` (novo) | Parse ELF64: `PT_LOAD` → segments; relocations `RELATIVE` + `64` | `load_elf()` retorna entry + stack + segments mapeados |
| F2.2 | `user_mode.rs` | `run_elf(path)` → `load_elf()` → `create_sandbox_as()` → mapeia → `enter_user_mode()` | `run_elf("hello.elf")` → "Hello from Ring3" |
| F2.3 | `isolation_ring.rs` | `ring3_run_native()` implementado; usa `run_elf()` | `ring3_run_native(add_fn, Cap::ENTER_USER)` → 42 |
| F2.4 | `capability_gate.rs` | Host functions reais (6.1-6.5) | Sandbox `aios_send_tcp` → log NIC TX |

**Otimização F2:** ELF loader só `RELATIVE` (PIE) na Fase 2; `64` (non-PIE) na Fase 3. PIE cobre 90% dos binários Rust.

---

### Fase 3: W^X Arena Ring3 + WASM B/C (Semanas 6-7)

| Task ID | Arquivo | Ação | Verificação |
|---------|---------|------|-------------|
| F3.1 | `exec_arena.rs` | VA USER (`0x0000_5000_0000_0000`); `map_user_page()` + NX toggle | `jit_write_exec()` em sandbox AS → executa código |
| F3.2 | `hermes/app_factory.rs` | `register_native_ring()` → `ring3_run_native()` | WASM B: Cranelift compila → arena Ring3 → executa |
| F3.3 | `hermes/wasmi_rt.rs` | Path A inalterado; B/C gated por `isolation_ring_available()` | `isolation_ring_available()` = true → B/C ativos |

**Otimização F3:** Arena Ring3 reusa `exec_arena.rs` logic; só muda VA base + flags USER. Cranelift `jit-cranelift` feature já compila no_std.

---

### Fase 4: Validação (Semana 8)

| Task ID | Critério | Verificação |
|---------|----------|-------------|
| F4.1 | Boot QEMU 8 fases + tick | `logs/boot.txt` mostra `[TIMER] tick=` incrementando |
| F4.2 | ELF "hello world" Ring3 | Serial: `[RING3] Hello from Ring3` via syscall `write` |
| F4.3 | Fault isolation | Sandbox acessa `0xFFFF_8000_0000_0000` → #GP → `[RING3] Process killed` + kernel vivo |
| F4.4 | WASM B executa | `cranelift` compila `add` → `ring3_run_native` → retorna 42 |
| F4.5 | `cargo check --release` = 0 | Zero erros, zero warnings novos |

---

## 4. Análises Técnicas

### A. `clone_current()` Bug (user_mode.rs:313)
**Problema:** `raw_vec` overflow no higher-half ao clonar page tables.
**Causa:** `AddressSpace::clone_current()` aloca frames via `alloc_frame()` que retorna frames no higher-half; `raw_vec` não suporta endereços >4GB.
**Fix:** `create_sandbox_as()` aloca L4 frame via `alloc_frame()` mas **não clona PTs inferiores** — evita o bug completamente.
**Verificação:** `demo_ring3()` roda sem `ponytail` warning.

### B. Single TSS Race Condition
**Problema:** `set_rsp0()` muta TSS global; 2 processos Ring3 em CPUs diferentes → RSP0 race.
**Solução:** `TSS_ARRAY[CPU][PROC]` — per-CPU + per-process. `interrupts.rs` já tem `TSS_ARRAY` esqueleto (linha 43-44 `TssCell`).
**Verificação:** Boot `-smp 4` + 2 processos Ring3 → `set_rsp0` chamado no context switch sem race.

### C. SYSCALL/SYSRET em WHPX/TCG
**Risco:** WHPX pode não expor MSRs `IA32_LSTAR`/`STAR`/`FMASK`.
**Mitigação:** `int 0x90` mantido como fallback. `syscall.rs` detecta `IA32_LSTAR` writable → usa SYSCALL; senão `int 0x90`.
**Verificação:** Boot WHPX + TCG + KVM — todos mostram syscall funcionando.

### D. ELF Relocations Mínimas
**Análise:** Binários Rust (PIE) usam 95% `R_X86_64_RELATIVE`. `R_X86_64_64` só em non-PIE (C legacy).
**Decisão:** Fase 2 = só `RELATIVE`. Fase 3 = `64` se necessário.
**Verificação:** `rustc -C relocation-model=pic` → `readelf -r` mostra só `RELATIVE`.

---

## 5. Sugestões de Implementação

### 1. **Comece pelo `create_sandbox_as()`** (F1.1)
- Menor risco, maior impacto: desbloqueia todo o resto
- Teste unitário: `demo_as_r1_r3_shallow()` adaptado
- Não mexe em `user_mode.rs` ainda

### 2. **ELF Loader como crate separado** (`neural-kernel/src/elf_loader.rs`)
- Não polui `user_mode.rs`
- Reutilizável para `hermes::elf_loader` (cross-os)
- Testável isoladamente: `load_elf(&hello_elf_bytes)`

### 3. **CapGate host functions = thin wrappers**
```rust
fn host_send_tcp(args: &[u64]) -> Result<u64, &'static str> {
    let (ip, port, buf_ptr, len) = decode_args(args);
    net::send(ip, port, unsafe { slice::from_raw_parts(buf_ptr as *const u8, len) })
}
```
- Valida Cap no `check()`; implementação delega para backend existente
- Sem lógica nova — só wiring

### 4. **W^X Arena Ring3 = copy-paste + VA change**
- `exec_arena.rs` já tem `jit_write_exec()` funcional
- Mudar `ARENA_VA` para USER range + `USER_ACCESSIBLE` + NX toggle
- Mesmo self-test (`mov eax, 42; ret` → 42)

---

## 6. Verificações de Qualidade (Quality Gates)

| Gate | Comando | Critério |
|------|---------|----------|
| **Compile** | `cargo check --release` | 0 erros |
| **Boot** | `./run-qemu-whpx.ps1` (timeout 80s) | 8 fases + `[TIMER] tick=` |
| **Ring3 Demo** | Serial log | `[RING3] Hello from Ring3` |
| **Fault Isolation** | Serial log | `[RING3] Process killed` + kernel tick continua |
| **WASM B** | `hermes` command | `wasm_b add 2 3` → `5` |
| **No Regress** | `diff` boot log antes/depois | Mesmas fases, mesmos agents, mesmos ticks |
| **Unsafe Audit** | `cargo geiger` | Zero `unsafe` novo sem safety comment |

---

## 7. Modulações (Adaptações por Contexto)

| Contexto | Modulação |
|----------|-----------|
| **QEMU TCG (sem KVM)** | `int 0x90` fallback ativo; SYSCALL desabilitado via feature `syscall_fast` |
| **WHPX** | `ring3_is_safe()` = false → `isolation_ring_available()` = false → WASM B/C off |
| **KVM** | Full path ativo; `ring3_is_safe()` = true |
| **HW Real** | `ring3_is_safe()` = true (após validação); testar `-cpu host` |
| **Low Memory (<2GB)** | `MAX_PROCS` = 2; `MAX_PAGES` sandbox = 8; arena = 1 page |
| **Debug Build** | `TRY_ENTER_RING3` = false; PoC desabilitado; logs verbose |

---

## 8. Resultados Esperados (Entregáveis)

### Código Novo (~3.400 LOC)
| Arquivo | LOC | Tipo |
|---------|-----|------|
| `address_space.rs` | +200 | Modificação (create_sandbox_as) |
| `interrupts.rs` | +150 | Modificação (TSS_ARRAY, set_rsp0) |
| `user_mode.rs` | +300 | Modificação (fix bug, enter_user_mode AS+RSP0) |
| `syscall.rs` | +400 | Modificação (SYSCALL entry, dispatch) |
| `elf_loader.rs` | ~800 | **Novo** (ELF64 loader) |
| `isolation_ring.rs` | +200 | Modificação (ring3_run_native impl) |
| `capability_gate.rs` | +300 | Modificação (host functions reais) |
| `exec_arena.rs` | +150 | Modificação (VA USER, NX) |
| `hermes/app_factory.rs` | +100 | Modificação (register_native_ring wiring) |
| `hermes/wasmi_rt.rs` | +50 | Modificação (B/C gate) |
| **Testes/Integração** | ~500 | Demos, self-tests, smoke |

### Artefatos de Validação
- `hello.elf` — ELF64 mínimo (`.text` + `.data` + reloc) para teste Ring3
- `add.wasm` — WASM B compilado via Cranelift (`fn add(a,b) -> a+b`)
- Boot log baseline (antes) vs. pós-implementação (diff)

### Métricas de Sucesso
| Métrica | Target |
|---------|--------|
| Boot time overhead | <5% (vs baseline) |
| Syscall latency (SYSCALL) | <300 ciclos |
| Ring3 fault containment | 100% (kernel nunca crasha) |
| WASM B execution | Functional (add 2+3=5) |
| `cargo check --release` | 0 erros |
| Unsafe blocks novos | ≤5 (todos com safety comment) |

---

## 9. Rastreabilidade (Traceability Matrix)

| Requisito ADR-0082 | Task ID | Arquivo | Verificação |
|-------------------|---------|---------|-------------|
| Deep L4 clone | F1.1 | `address_space.rs` | `create_sandbox_as()` test |
| Per-process RSP0 | F1.2 | `interrupts.rs` | SMP 2 procs Ring3 |
| Fix clone_current | F1.3 | `user_mode.rs` | `demo_ring3()` roda |
| SYSCALL/SYSRET | F1.4 | `syscall.rs` | `rdtsc` <300 ciclos |
| ELF loader | F2.1 | `elf_loader.rs` | `load_elf(hello.elf)` |
| run_elf | F2.2 | `user_mode.rs` | "Hello from Ring3" |
| ring3_run_native | F2.3 | `isolation_ring.rs` | `add_fn` → 42 |
| CapGate host fns | F2.4 | `capability_gate.rs` | `aios_send_tcp` → NIC TX |
| W^X Ring3 | F3.1 | `exec_arena.rs` | `jit_write_exec` USER |
| WASM B/C | F3.2 | `app_factory.rs` | `wasm_b add 2 3` → 5 |
| Validação full | F4.1-F4.5 | Boot + logs | Todos PASS |

---

## 10. Decisões de Depreciação (Formal)

| ADR-0041 Seção | Status | Substituído Por |
|----------------|--------|-----------------|
| §3 Non-goals (Ring3) | **Deprecated** | ADR-0082 §2 (MVP Scope) |
| §4 P9 (Ring3) | **Deprecated** | ADR-0082 Items R3-01 a R3-06 |
| §7 Checklist (Ring3) | **Deprecated** | ADR-0082 §6 Quality Gates |
| §8 Next Steps (Ring3) | **Deprecated** | ADR-0082 §3 Phases |

**ADR-0041 permanece ativa para:** Mapa R0–R3 (§9.2), Cap Matrix (§9.5), Migration H0–H5 (§9.8), Pacotes A/B + N1–N5.

---

**Registro completo.** Pronto para execução faseada ou revisão de maintainer.