# SESSION_438 — bit-engine Unified: clean build + QEMU 8c/8GB 1h (crash/#PF)

**Data:** 2026-10-02
**Escopo:** `cargo clean` + build from-scratch (incl. nsgdb 1.3) + QEMU 8c/8GB com
todos os devices de HW, mantido ativo com monitoramento de log, corrigindo os
`#PF`/freezes observados. **GOAL literal:** "1h sem crash ou #PF".

> Nota de numeração: `SESSION_437` já pertencia à frente tokenizer Falcon3
> (outro dono). Este registro usa o próximo número livre (lição SESSION_264/277
> — nunca sobrescrever sessão de outra frente).

---

## 1. Build from-scratch

- `cargo clean` → 34751 arquivos / 71.2GiB removidos. `target/` inteiro apagado.
- Preservados antes do clean: `disk_qemu.raw` (3.2GB), `FALCON3.BIN` (989MB),
  `HINT.BIN`, `bpe_vocab.bin`, `ovmf_*.fd`, `netmode.flag` → restaurados depois.
- **Bloqueio de compilação resolvido:** `k_ai/sgdb/nsgdb_bridge.rs::resolve_conflict_nsgdb`
  — o neural-sgdb **1.3.0** passou a devolver `ResolveOutcome` (não `()`); a bridge
  fazia `.map_err` sobre `Result<ResolveOutcome, _>` (E0308). Fix: `.map(|_outcome| ())`.
- `cargo build --release -p boot` → **0 erros** (~1m16s), gera `target/uefi.img`
  + `ovmf_*.fd` (bindeps compila o kernel `x86_64-unknown-none`). `cargo test -p cortex` verde.

## 2. QEMU 8c/8GB (`run-qemu-whpx.ps1 -Smp 8 -RamGB 8`)

Devices: OVMF pflash, e1000 (user/slirp), intel-hda + hda-duplex (dsound), qemu-xhci
+ usb-tablet/kbd, virtio-gpu; modelos via QEMU loader (`FALCON3.BIN` @0x100000000,
`bpe_vocab.bin` @0x13DF00000). Rodado em ~8 ciclos + 2 com **QEMU monitor**
(`info registers` por vCPU) para diagnóstico decisivo.

## 3. Fixes entregues (10 arquivos, cada um resolve um #PF/freeze real)

| # | Bug | Fix | Arquivo |
|---|-----|-----|---------|
| 1 | **Crash #PF**: `BeiState.cell_network` (Arc em +0x18) corrompido para `0x6` → lock do Mutex em `0x16` → storm | guard `all_ptrs_valid()` (valida Arc::as_ptr high-half) antes de qualquer `.lock()`; tick pulado + warn rate-limited | `hermes/src/bei.rs` |
| 2 | **Overflow de heap**: `realloc` copiava `layout.size()` (antigo) no buffer `new_size` — overflow em shrink | `layout.size().min(new_size)` | `k_nano/src/allocator.rs` |
| 3 | **Flood de log**: 44.187 `grow entry` + 44.187 `budget cap`/10min com bump no teto | cap-check ANTES de `heap_observe`/entry-log + `CAP_REFUSED_LOG_AT` (1 log por HEAP_LIMIT) | `k_nano/src/allocator.rs` |
| 4 | **Spin eterno no `TicketLock<VecDeque<Event>>`** (BSP 100% CPU, log congelado) | aquisição **bounded** (`lock_bounded` = try_lock + 256 spins + desistir) em `publish`/`try_receive`/`has_pending`/`pending_len`/`subscriber_count` | `event-bus/src/bus.rs` |
| 5 | Latch do scheduler sem deadline + `process_wakes` self-wake infinito | `try_with_agent_tick_lock_ms(BSP_TICK_LOCK_BUDGET_MS=50)` no call-site + cap de 64 wakes/chamada | `agent-core/src/lib.rs`, `k_nano/src/async_rt.rs` |
| 6 | Hooks indiretos (`static mut` fn-ptr) chamados via `transmute` sem validar → branch p/ lixo | `hook_ptr_ok(p)` (range `.text` high-half) antes de `transmute` | `agent-core/src/lib.rs` |
| 7 | Handler #PF lia `ip`/`sp` sem checar mapeamento → fault nested; streak contava o IP do handler e parqueava o BSP | guards `k_nano::memory::is_page_present` no dump de `ip_bytes`/`stk` | `neural-kernel/src/interrupts_ext.rs` |
| 8 | **HDA MMIO com base nula**: `AudioMixerAgent::tick` → `playback_free_mono_samples` → `r32(bar=0, 0x13c)` → #PF | guard `bar==0` em `r32/w32/r16/w16/r8/w8` (choke point único) | `k_nano/src/audio/hda.rs` |
| 9 | Bump pode devolver ponteiro que envolve p/ VA baixa | `if aligned_ptr < heap_start { null }` | `k_nano/src/allocator.rs` |
| 10 | Park do #PF storm era **mudo** (sem CR2 no BOOT.LOG) | park agora loga `cr2=` + `try_flush_ramlog()` | `neural-kernel/src/interrupts_ext.rs` |

## 4. Método de diagnóstico (o que funcionou)

1. `run-qemu-whpx.ps1` (sem monitor) + leitura do serial — mas o serial é **lossy
   sob SMP** e o `#PF storm` esconde-se no meio do spam de matmul.
2. Relançar **manualmente** com `-monitor tcp:127.0.0.1:4447`; no freeze, `cpu N` +
   `info registers` por vCPU → RIP de cada core.
3. Simbolizar `ip`/stack com `llvm-nm -C --numeric-sort target/limine-esp-tree/kernel.elf`
   (nearest-symbol ≤ endereço).
4. Foi assim que o spin do EventBus (fix 4) e a origem HDA (fix 8) foram localizados.

**Arnês entregue:** `tools/watch_corruption.ps1` (parse OK) — lança QEMU com monitor,
detecta freeze/storm, extrai `ip=`/`cr2=` e dumpa+simboliza os RIPs automaticamente.

## 5. Causa-raiz NÃO fechada (o GOAL não foi atingido)

Cada run de ~7–15min termina num `#PF` de **site rotativo**, sempre da mesma
**família**: um ponteiro corrompido para valor pequeno/limiar:

| Run | Valor | Site |
|-----|-------|------|
| A | `0x6` | `BeiState.cell_network` (Arc) → lock (fix 1) |
| B | `0x13c` | `hda::r32` (bar=0) (fix 8) |
| C | `0x7ee00001` | bump alloc / `MpmcQueue` (~teto 2030MB) |
| D | `0x42d` | `atomic_compare_exchange_weak` (lock em endereço inválido) |

Eliminar os sites conhecidos só expõe o próximo → a **fonte escritora persiste**.
O **amplificador comum**: 3 faults no mesmo IP → `loop{hlt}` no **BSP** (dono de
scheduler/timer/display) → freeze total, sem reboot.

**Hipótese testada (2c): inconclusiva.** Rodar 2 cores **não** reproduz o storm,
MAS porque o matmul SMP com 1 AP **não funciona** (`barrier timeout pending=1 done=0`,
6s/timeout) → prefill arrasta (68s/layer) → nunca completa job → não exercita o
caminho que corrompe. Logo "2c limpo" ≠ evidência de race. (Residual novo: bug real
de SMP-1-AP no barrier.)

**Conclusão honesta:** não é um bug pontual — é um **wild-write esporádico**. Sem
watchpoint no *writer*, corrigir sites por iteração cega é whack-a-mole. O arnês
está pronto; falta rodá-lo num shell normal (o wrapper de execução mata runs
foreground longos quando o QEMU segura o pipe) e fixar o watchpoint na vítima.

## 6. Gates / evidência

- `cargo build --release -p boot` **0 erros** (recompila k-nano/k-hal/agent-core/event-bus/hermes/kernel/boot).
- `cargo test -p cortex --lib` **118/118** (após Fase 1 do bit-engine).
- QEMU 8c/8GB: 8 fases de boot OK, Runtime vivo, inferência Falcon3 completa jobs
  (`done id=N`, `prefill_done`, `decode_tok/s`) até o `#PF` rotativo.
- `#PF storm` confirmado por QEMU-monitor (`ip`/`cr2` + RIPs) em múltiplos runs.

## 7. Lições

- **Serial é lossy sob SMP; o crash real esconde-se no spam.** O `#PF storm` da
  run B estava *dentro* do spam de matmul e passou batido no primeiro grep; só a
  leitura linha-a-linha completa (`PF_DBG ip=`/`EXC`) o revelou. Filtrar por
  `[T+]`/`[sub]` esconde as linhas do handler (não têm prefixo).
- **Park do BSP = freeze total.** Qualquer fault (mesmo recuperável) vira freeze
  porque o `#PF storm` parqueia em `loop{hlt}` e o BSP dona do scheduler/timer/display.
  Um park fail-closed de CORE só é "vivo" se o core não for o BSP.
- **`TicketLock` não-reentrante + IRQ = deadlock** (contrato do crate): `publish`
  de IRQ enquanto o thread segura o mesmo lock gira para sempre. Aquisição bounded
  (`try_lock`+limite) é o remédio para locks best-effort.
- **`realloc` pode ENCOLHER**: copiar `layout.size()` sem `min(new_size)` transborda.
- **MMIO/`static mut` corrompidos**: validar base (`bar==0`) e ponteiro de hook
  (range `.text`) antes do deref/`transmute` transforma crash em no-op honesto.
- **QEMU-monitor é o instrumento decisivo** quando o serial é lossy: RIP por vCPU
  + `llvm-nm` localizam o site em 1 run.

## 8. Residual / próximo passo

- **GOAL 1h sem crash/#PF: BLOQUEADO** no wild-write. Próximo: `tools/watch_corruption.ps1`
  → identificar a estrutura-vítima pelo `cr2=` do park → re-rodar com `-S -gdb tcp::1234`
  + `watch -l *(unsigned long*)(BASE+OFF)` → o store que zera o ponteiro aparece no 1º hit.
- **SMP-1-AP barrier bug**: `parallel_matmul` com 1 AP trava (`pending=1 done=0`);
  incompatível com `-smp 2` (não é regressão desta sessão; novo).
- **Não commitar tag/release:** GOAL pendente; os 10 fixes são commitáveis individualmente.

**Arquivos:** `crates/k_ai/src/sgdb/nsgdb_bridge.rs`, `crates/hermes/src/bei.rs`,
`crates/event-bus/src/bus.rs`, `crates/agent-core/src/lib.rs`, `crates/k_nano/src/{allocator,async_rt,slog}.rs`,
`crates/k_nano/src/audio/hda.rs`, `crates/neural-kernel/src/interrupts_ext.rs`,
`crates/cortex/src/parallel_matmul.rs`, `tools/watch_corruption.ps1`.
