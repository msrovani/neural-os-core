# SESSION_243 — ADR-0082: Isolamento Ring3 de Produção — Fases 1–4 (2026-08-03)

**Objetivo:** Implementar o isolamento Ring3/SFI de produção (ADR-0082, que depreca ADR-0041 §P9+ para escopo Ring3): address space real, syscall rápido, ELF loader, sandbox W^X e integração WASM B/C gated. Validar boot completo com WHPX/TCG.

**ADR:** `docs/architecture/0082-ring3-isolation-production.md` + `0082-ring3-isolation-registry.md` (docs commitados no `1cd17fd` do usuário).

---

## Fase 1 — Fundação (commit `8d3eb90`)

| Item | Arquivo | Entrega |
|------|---------|---------|
| F1.1 | `address_space.rs` | `create_sandbox_as()` from-scratch (kernel supervisor-only P4[≥256], sem PTs compartilhadas) + `frame_for_virt()` |
| F1.2 | `interrupts.rs` | `TSS_ARRAY[8]` per-process (RSP0/IST dedicados) + `switch_to_proc_tss(pid)` |
| F1.3 | `user_mode.rs` | `demo_ring3` usa `create_sandbox_as` (fix bug higher-half `clone_current`) + `run_elf()` |
| F1.4 | `syscall.rs` | SYSCALL/SYSRET fast path: `init_syscall_fast_path()` (wrmsr LSTAR/STAR/FMASK) + naked entry + `dispatch_syscall()` |
| — | `main.rs` | wiring `init_syscall_fast_path()` pós-IDT (depois movido pós-probe — ver F4) |

## Fase 2 — ELF Loader + Sandbox (commit `8d3eb90`)

| Item | Arquivo | Entrega |
|------|---------|---------|
| F2.1 | `elf_loader.rs` | Merge com ADR-0076 original (preservou API `ElfLoader::load`/`is_valid_elf`/`load_and_spawn`): `create_sandbox_as`, flags RX/RW por segmento (PF_X), relocations `R_X86_64_RELATIVE` (PIE base=0), `elf_boot_self_test()` |
| F2.2 | `user_mode.rs` | `run_elf(data)` → load_and_spawn → run_process |
| F2.3 | `isolation_ring.rs` | `ring3_run_native()` implementado (era `Err("não implementado")`) |
| F2.4 | `capability_gate.rs` | `host_send_tcp_payload()` real (udp_exchange no kernel) + `parse_dotted_ipv4` |

## Fase 3 — W^X Arena USER + WASM B/C (commit `1450108`)

| Item | Arquivo | Entrega |
|------|---------|---------|
| F3.1 | `address_space.rs` + `exec_arena.rs` | `set_user_leaf_flags()` (flip RW→RX preserva USER) + `jit_write_exec_user(aspace, code)` + `user_arena_self_test()` |
| F3.2 | `isolation_ring.rs` | `ring3_run_native()` dual path: ELF64 loader \| blob nativo (jit_write_exec_user + stack USER + enter_user_mode) |
| F3.3 | `app_factory.rs` (hermes) | já correto — B/C gated por `isolation_ring_available()` = `native_ring_registered()` |

## Fase 4 — Validação (fixes `6b073bf` + `4c7a2e9`)

### Bug 1: SYSCALL/SYSRET #GP no WHPX (fix `6b073bf`)
- **Sintoma:** WHPX 1 core → `#GP ip=0xffffffff8006057e` (wrmsr) logo após "IDT carregada", boot trava (log 4.5KB).
- **Causa raiz:** `init_syscall_fast_path()` rodava **antes** do `platform_probe::detect()` (HardwareDiscovery). `HV_KIND` ainda era default `0` → `hv_from_u8(0)=HypervisorKind::None` (confundido com HW real). O gate liberava o `wrmsr`, e o WHPX rejeita a escrita → #GP (TCG permitia no-op, por isso nunca apareceu).
- **Fix:** `platform_probe::probe_done()` (distingue não-probado de HW real) + gate `probe_done() && hv ∈ {None, Kvm}`; WHPX/TCG/VBox/VMware → fallback `int 0x90` (DPL=3 já ativo). `init_syscall_fast_path()` movido para **após** o probe.
- **Evidência:** `[SYSCALL] gated off (probe=true hv=MicrosoftHv) - fallback int 0x90` — boot 8 fases OK, log 23KB.

### Bug 2: #PF no USER arena (fix `4c7a2e9`)
- **Sintoma:** TCG 2 cores 8G sem disk → `#PF CR2=0x0000500000000000` (= ARENA_VA).
- **Causa raiz:** `jit_write_exec_user` escrevia via `ARENA_VA as *mut u8` (CR3 do kernel), mas a página só está mapeada no **sandbox AS** → #PF. Mesmo no `user_arena_self_test` que executava o blob via transmute em Ring 0 com CR3 do kernel.
- **Fix:** escrita via **HHDM no frame** (`hhdm_mut::<u8>(frame)`); self-test valida folha USER mapeada + bytes via HHDM (execução real = `ring3_run_native` em CPL=3).

### Bug 3: ELF self-test "no program headers" (fix `4c7a2e9`)
- **Sintoma:** `[ELF] [selftest] - load fail: ELF: no program headers`.
- **Causa raiz:** offsets errados no header sintético: `e_phentsize` em `52..54` (correto `54..56`), `e_phnum` em `54..56` (correto `56..58`).
- **Fix:** offsets ELF64 corretos (54/56/58/60).

## Verificação final (TCG 2 cores 8G, sem disk — ATA PIO + FAT32 trava boot)

```
[T+1] P6 Ring3 OK (SUCCESS iretq+CPL3 marker=3352494e470001 Cap::ENTER_USER)
[T+1] ELF loader self-test PASS (entry=0x1000 stack_top=0x700000404000 — RX code + RW BSS)
[T+1] W^X USER arena self-test PASS (6 bytes USER RX @0x500000000000)
[T+1] P7 demand-page OK / P8 vring OK / P9 gguf-mmap OK
[T+1] ISO-RING: Ring3 environment UNSAFE — native ring NOT registered; wasmi (A) ativo (TCG gate honesto)
[T+2] AgentFleet 54 agents + Runtime scheduler
[T+2] NET-HW VERDICT=PASS reason=rx_count=8 at_runtime
[T+1023] WASMI runtime self-test PASS (add(2,3)=5) — ADR-0059 A
```

- `cargo check --release` (workspace) = **0 erros**.
- WHPX 1 core: gate validado (sem #GP, 8 fases) mas QEMU WHPX instável nesta máquina (~60s, "Ignoring request for interrupt vector 0" — conhecido AGENTS.md).
- Boot completo (8 fases + Runtime + tick) validado em TCG 2 cores 8G sem disk.

## Commits

| Commit | Conteúdo |
|--------|----------|
| `8d3eb90` | Fase 1+2 (fundação + ELF loader + sandbox + ring3_run_native + cap host real) |
| `1450108` | Fase 3 (arena W^X USER + WASM B/C gated) |
| `6b073bf` | Fix SYSCALL/SYSRET gated por hypervisor real (WHPX #GP) |
| `4c7a2e9` | Fix Fase 4: 3 bugs sandbox W^X (HHDM write, selftest sem exec Ring0, offsets ELF) |

## Lições (memorizadas no AGENTS.md)

1. **Feature que escreve MSR no boot precisa do hypervisor REAL** — `hypervisor()` retorna `None` (default 0) antes do probe; indistinguível de HW real. Gate de features sensíveis (wrmsr) exige `probe_done()`.
2. **WHPX rejeita `wrmsr` dos MSRs SYSCALL (LSTAR/STAR/FMASK) → #GP**; TCG permite no-op (bug mascarado). Fallback `int 0x90` obrigatório em WHPX/TCG.
3. **Escrever em VA que só existe no sandbox AS, com CR3 do kernel, dá #PF** — escrever via HHDM no frame físico.
4. **Self-test que executa código USER com CR3 do kernel dá #PF** — validar folha + bytes, não executar (execução real = CPL=3).
5. **Boot TCG trava no ATA PIO/FAT32** (conhecido) — validar runtime com `-NoDisk` (script mesh faz isso).
6. **ELF64 header offsets**: e_phentsize@54, e_phnum@56, e_shentsize@58, e_shnum@60.
7. **Bin já tinha `elf_loader.rs` (ADR-0076)** — sempre `git show HEAD:<path>` antes de sobrescrever arquivo existente; fundir preservando API.
