# ADR-0082: Ring3 Isolation — Produção (Sucessor de ADR-0041 §P9+)

> **Conflito de ID:** `0082` canônico no INDEX é **HardwareInfo** (`0082-hardware-info-registry.md`). Este arquivo e `0082-ring3-isolation-registry.md` são o **checklist Ring3** (execução → **ADR-0077**). Não misturar com HardwareInfo. **ADR-0102** (#545) filtra itens deste checklist via Onda 6 — não substitui a 0077.

**Data:** 2026-08-02  
**Status:** Proposed — checklist de execução Ring3; **subordinado a ADR-0077** (autoridade) e **ADR-0102** (filtro Onda 6)  
**Supersedes:** ADR-0041 §3 (non-goals), §4 (P9), §7 (checklist), §8 (next steps) — **apenas para Ring3**  
**Relacionada:** ADR-0041, ADR-0042, ADR-0059 (WASM B/C), **ADR-0077** (canônico Ring3), **ADR-0102** (filtro sandbox B/C), `0082-ring3-isolation-registry.md`  
**Sprint:** ADR-0100 Onda 6 (T-051–T-057) — enquadrado em **Pós-gate / Layer S**, não sprint paralelo  

---

## 1. Contexto — Estado Real (Agosto 2026)

| Subsistema | Arquivo | LOC | Status | Gap para Produção |
|------------|---------|-----|--------|-------------------|
| Ring3 entry/exit | `user_mode.rs` | 387 | PoC ✅ | Per-process stacks, ELF loader, signals, preemption |
| Address Space | `address_space.rs` | 336 | PoC 🟡 | Shallow L4 clone (compartilha PTs kernel) → deep clone + ASID |
| Demand Paging | `demand_page.rs` | 137 | PoC ✅ | try_lock em #PF, sem swap/eviction |
| Capability Gate | `capability_gate.rs` | 110 | PoC ✅ | Stub host fns, sem delegation/revocation |
| Syscall | `syscall.rs` | 340 | PoC ✅ | `int 0x90` → SYSCALL/SYSRET, vDSO |
| GDT/TSS/IDT | `interrupts.rs` | 477 | Prod 🟢 | Single TSS → per-CPU + per-process RSP0 |
| Exec Arena (W^X) | `exec_arena.rs` | 151 | PoC 🟡 | Ring 0 only → Ring3 sandbox + NX |
| Isolation Ring | `isolation_ring.rs` | 75 | **Stub** 🔴 | `ring3_run_native()` = `Err("não implementado")` |
| File mmap | `gguf_mmap.rs` | 265 | PoC 🟡 | Prefix only → streaming + parser |
| ELF Loader userland | — | 0 | **Ausente** 🔴 | Load sections, relocations, interpreter |

**Conclusão:** ADR-0041 P0–P9 PoC ✅ completo (non-fatal demos). **Isolamento de produção = não iniciado.** O esforço estimado original (~3.000 LOC) subestima — isolamento real exige address spaces verdadeiros, não shallow clones.

---

## 2. Decisão — Escopo Mínimo Viável (MVP Ring3)

**Não vamos fazer "microkernel completo".** Vamos entregar **isolamento Ring3 funcional para WASM B/C (native JIT)** — o único consumidor real no roadmap.

### MVP Scope (Must Have)

| # | Item | Critério de Aceite |
|---|------|-------------------|
| 1 | **Deep L4 clone** | `create_sandbox_as()` cria AS do zero; copia só P4[511] (kernel text) + HHDM; **não compartilha PTs inferiores** |
| 2 | **Per-process kernel stacks** | Cada processo Ring3 tem seu RSP0; `set_rsp0()` no context switch |
| 3 | **ELF loader mínimo** | Carrega `.text` (RX), `.data`/`.bss` (RW), resolve relocations `R_X86_64_RELATIVE` + `R_X86_64_64`; entry point → `enter_user_mode()` |
| 4 | **SYSCALL/SYSRET** | Substitui `int 0x90`; fast path; preserva ABI registrador (RAX, RDI, RSI, RDX, R10, R8, R9) |
| 5 | **Sandbox AS + `ring3_run_native()`** | Cria AS isolado → mapeia código W^X (USER RX) → `enter_user_mode()` com Cap mínima → fault isolado não derruba kernel |
| 6 | **CapGate host functions reais** | `aios_send_tcp`, `aios_write_ring`, `aios_map_fb`, `aios_pin_dma`, `aios_map_file` — implementações mínimas que validam Cap + chamam backend |

### Explicitly Out of Scope (Nice to Have → Futuro)

- Multi-threaded user processes (TLS, futex, pthreads)
- Signals (SIGSEGV, SIGKILL, etc.)
- Swap / page eviction / huge pages
- Capability delegation / attenuation / revocation
- ASID/PCID / PKU
- vDSO
- IOMMU / DMA isolation per-process
- Streaming GGUF mmap (parser integration)

---

## 3. Arquitetura — Address Space Isolation

### Princípio: **No Shared Page Tables**

```
Kernel AS (CR3 base)
├── P4[511] → Kernel text (RX)          ← COPIADO para sandbox AS
├── P4[256..510] → HHDM (RW)            ← COPIADO para sandbox AS
├── P4[0..255] → User VAs (USER)        ← **VAZIO** no kernel AS
└── ... (restante)

Sandbox AS (novo CR3)
├── P4[511] → Kernel text (RX)          ← Shared (read-only, global)
├── P4[256..510] → HHDM (RW)            ← Shared (kernel access only)
├── P4[0..255] → User VAs (USER)        ← **PRÓPRIO** — isolado
│   ├── 0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF: user code/data/stack
│   └── 0x0000_5000_0000_0000: W^X arena (USER RX)
└── ... (restante vazio)
```

### Mudanças em `address_space.rs`

```rust
// NOVO: create_sandbox_as() — from scratch, não clone
pub fn create_sandbox_as() -> Result<AddressSpace, &'static str> {
    let l4_frame = alloc_frame()?;
    let mut aspace = AddressSpace { l4_frame };
    
    // Copia SÓ P4[511] (kernel text) + P4[256..510] (HHDM)
    copy_kernel_mappings(&mut aspace)?;
    
    // User VAs começam vazias — mapeadas sob demanda
    Ok(aspace)
}

// REMOVER: clone_current() para Ring3 (usa shallow clone só para kernel threads)
```

---

## 4. Plano de Implementação (Fases)

### Fase 1 — Fundação (1-2 semanas)

| Task | Arquivo | Descrição |
|------|---------|-----------|
| 1.1 | `address_space.rs` | `create_sandbox_as()` from-scratch; remover shallow clone para Ring3 |
| 1.2 | `interrupts.rs` | Per-process TSS array; `set_rsp0(process_id)` no context switch |
| 1.3 | `user_mode.rs` | Fix `clone_current()` bug (linha 313); `enter_user_mode()` recebe `AddressSpace` + `RSP0` |
| 1.4 | `syscall.rs` | `SYSCALL`/`SYSRET` path (MSR `IA32_LSTAR`, `IA32_STAR`, `IA32_FMASK`); preserva `int 0x90` como fallback |

### Fase 2 — ELF Loader + Sandbox (2-3 semanas)

| Task | Arquivo | Descrição |
|------|---------|-----------|
| 2.1 | `elf_loader.rs` (novo em `neural-kernel`) | Parse ELF64: program headers → `PT_LOAD` segments; mapeia `.text` (RX), `.data`/`.bss` (RW); relocations `RELATIVE` + `64`; retorna `entry_point`, `stack_top` |
| 2.2 | `user_mode.rs` | `run_elf(path)` → `load_elf()` → `create_sandbox_as()` → mapeia segments → `enter_user_mode(entry, stack, sandbox_cr3, Cap::ENTER_USER)` |
| 2.3 | `isolation_ring.rs` | `ring3_run_native(code, caps)` → compila/valida → `run_elf()` em sandbox AS |
| 2.4 | `capability_gate.rs` | Implementar host functions mínimas: `aios_send_tcp` → `net::send()`, `aios_write_ring` → `ring::write()`, `aios_map_fb` → `fb::map()`, `aios_pin_dma` → `dma::pin()`, `aios_map_file` → `fs::mmap()` |

### Fase 3 — W^X Arena Ring3 + Integração WASM B/C (1-2 semanas)

| Task | Arquivo | Descrição |
|------|---------|-----------|
| 3.1 | `exec_arena.rs` | Mover arena para VA USER (ex: `0x0000_5000_0000_0000`); `map_user_page()` com `USER_ACCESSIBLE` + `NX` toggle; `jit_write_exec()` em sandbox AS |
| 3.2 | `hermes/app_factory.rs` | `register_native_ring()` → usa `ring3_run_native()`; WASM B (Cranelift) → emite código nativo → arena Ring3 → executa isolado |
| 3.3 | `hermes/wasmi_rt.rs` | Path A (wasmi) inalterado; Path B/C gated por `isolation_ring_available()` |

### Fase 4 — Validação (1 semana)

| Task | Critério |
|------|----------|
| 4.1 | Boot QEMU: 8 fases + `[TIMER] tick=` |
| 4.2 | `demo_ring3()` roda ELF "hello world" (`.text` + `.data` + reloc) |
| 4.3 | Sandbox fault (acesso kernel VA) → `#GP` contido, processo morto, kernel vivo |
| 4.4 | WASM B: Cranelift compila `fn add(a,b) -> a+b` → executa em Ring3 → retorna resultado |
| 4.5 | `cargo check --release` = 0 erros |

---

## 5. Riscos e Mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|-------|---------------|---------|-----------|
| `clone_current()` bug (linha 313) bloqueia Fase 1 | Alta | Bloqueia tudo | Fix prioritário — `raw_vec` overflow no higher-half |
| Single TSS → race em SMP | Média | Crash | Per-CPU TSS array (já existe `TSS_ARRAY` em `interrupts.rs`) |
| ELF relocations complexos | Média | Atraso Fase 2 | Começar só `RELATIVE` (PIE); `64` depois |
| SYSCALL/SYSRET em WHPX/TCG | Baixa | Instabilidade | Testar em KVM + TCG; fallback `int 0x90` mantido |
| WASM B/C não prontos | Baixa | Esforço desperdiçado | Path A (wasmi) já roda; B/C = opt-in gated |

---

## 6. Estimativa Realista

| Fase | Semanas | LOC Estimado | Engenheiros |
|------|---------|--------------|-------------|
| 1. Fundação | 1-2 | ~800 | 1-2 |
| 2. ELF + Sandbox | 2-3 | ~1.500 | 2 |
| 3. W^X + WASM B/C | 1-2 | ~800 | 1-2 |
| 4. Validação | 1 | ~300 | 1 |
| **Total** | **5-8** | **~3.400** | **2-3** |

> **Nota:** Estimativa original ADR-0041 (~3.000 LOC) era otimista — não contava deep clone, ELF loader, SYSCALL/SYSRET, per-process TSS.

---

## 7. Gate de Entrada (Pré-requisitos)

Antes de iniciar Fase 1:

- [ ] `cargo check --release` = 0 erros (baseline)
- [ ] Boot QEMU 8 fases + tick OK (baseline)
- [ ] `clone_current()` bug fixado (user_mode.rs:313)
- [ ] ADR-0041 mantida para mapa R0–R3; esta ADR **só** para Ring3 isolamento

---

## 8. Referências

- ADR-0041 — K³CHJ Capability Rings (mapa R0–R3 canônico)
- ADR-0042 — N1–N5 adequação + wire crates
- ADR-0059 — Runtime App Factory (WASM A/B/C)
- ADR-0077 — Isolation Ring Connectors
- `crates/neural-kernel/src/user_mode.rs` — PoC atual
- `crates/neural-kernel/src/address_space.rs` — Shallow clone atual
- `crates/neural-kernel/src/isolation_ring.rs` — Stub atual
- SESSION_241 — Ring3/SFI map completo (explorer)

---

## 9. Decisão de Depreciação

**ADR-0041 §3 (non-goals Ring3), §4 (P9), §7 (checklist Ring3), §8 (next steps Ring3) são DEPRECADOS para escopo Ring3** — substituídos por este checklist **e** pelo gate de aceite **ADR-0077 §6**.

**Autoridade Ring3:** **ADR-0077** (decisão + §6). Este arquivo e `0082-ring3-isolation-registry.md` são **checklists de execução** subordinados. **ADR-0102** filtra itens (ex.: R3-01 deep clone → compartilhar PTs kernel com USER off; R3-04 SYSCALL → adiado; ver §2 da 0102).

ADR-0041 permanece **válida e ativa** para:
- Mapa de privilégio R0–R3 + k-HAL ownership (§9.2)
- Cap Matrix (§9.5)
- Migration Roadmap H0–H5 (§9.8)
- Pacotes A/B + N1–N5 (ADR-0042)