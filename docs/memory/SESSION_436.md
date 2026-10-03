# SESSION_436 — Watchdog de silêncio no kernel (`[SILENCE]`) — OOM-HALT para spin SEM OOM

**Data:** 2026-10-03
**Branch:** main | **Base:** s435 (telemetria TALC) / s438 (fixes #PF)
**Motivação:** lab longo s435 (logs 183000/185325) — stall silencioso determinístico 2/2 boots:
log congela T+23334/T+23437, QEMU vivo ~1 core em spin, **SEM OOM-HALT** (não é `oom()`) e
sem #PF novo. O `[OOM-HALT]` da s434 só cobre morte dentro do alloc; um spin em qualquer
outro path (lock de serial/log, loop de agente, critical section com IF=1) morre mudo.
Pedido: BOOT.LOG parado + CPU ativa → carimbar stamps de todos os cores.

## 1. Desenho (módulo `k_nano::silence_watchdog`, observe-only)

**Premissa física do detector:** a IRQ do timer dispara em ~60–100 Hz mesmo durante spins
de agente (IF=1). Se o timer continua avançando e NENHUMA linha de log sai, alguém está
preso num loop que não loga — isso é observável de dentro do próprio handler do timer.

- **`note_log_emit()`** — chamado nos choke points únicos de emissão:
  `serial::dispatch_bytes` (console+BOOT.LOG, incluindo o fallback `[NESTED]`) e
  `boot_logger::buffer_log` (`log_quiet`/`append_raw`/`log_no_flush`). Custo: 1 store Relaxed.
- **`on_timer_irq()`** — chamado do `timer_handler` (após `TIMER_TICKS.fetch_add`):
  1. carimba `CORE_LAST_IRQ_US[0]` (idade do timer = prova de vida do detector);
  2. `decide()` pura (testável com tempo injetado): gate `TIMER_TICKS ≥ 1800`
     (boot tem silêncios legítimos — FAT walk, model load), `LAST_EMIT_US != 0`,
     idade ≥ **60s** (prefill medido ~57s com logs por slice — não falso-positiva),
     rate-limit **1 dump / 10s** (cadência do OOM-HALT s434).
- **`dump_line()`** — linha única `[SILENCE]` via `interrupts::puts` (escrita serial
  **lock-free** dos handlers de IRQ — NUNCA `SERIAL.lock()`, pois o spinner pode estar
  segurando exatamente esse lock: é UMA causa de silêncio). Zero alloc, buffer fixo 640B.
  Conteúdo: idade do silêncio, tick, dump#, heap atual/budget, APs online, #PFs, última
  exceção (kind/IP/idade) + **stamps por core**:
  - `irq: c0=..s ... c7=..s` — idade do stamp do timer IRQ (só c0 tem amostra: IRQs
    roteiam ao BSP neste kernel; demais = `–` honesto, n/a ≠ 0);
  - `prog: cN=..s` — idade do stamp de progresso (`note_core_progress`): core com idade
    crescendo = sem progresso (spin/bloqueio); idade fresca = loop vivo que não loga.
- **`note_core_progress()`** — chamado do `ap_idle_loop` (APs, 1 load+store Relaxed por
  iteração) e do heartbeat pós-tick do scheduler no bin (core do BSP). Guard: só carimba
  após o 1º timer IRQ (garante PerCpu/GS válido — evita ler `gs:[8]` cedo demais).
- **Observe-only (lição s429-lab):** NUNCA age (sem reboot/park/hlt) — só prova vida.

**Limite honesto:** se o spin segurar IF=0 NO core do timer, o próprio watchdog morre
junto (nada em kernel-user pode vigiar isso) — aí o instrumento é o QEMU-monitor /
`tools/watch_corruption.ps1` (lição s438). O `[SILENCE]` cobre a classe IF=1 (a comum).

## 2. Arquivos

| Arquivo | Mudança |
|---|---|
| `crates/k_nano/src/silence_watchdog.rs` | **NOVO** — módulo completo + 5 testes |
| `crates/k_nano/src/lib.rs` | `pub mod silence_watchdog;` |
| `crates/k_nano/src/serial.rs` | `note_log_emit()` no `dispatch_bytes` (normal + `[NESTED]`) |
| `crates/k_nano/src/boot_logger.rs` | `note_log_emit()` no `buffer_log` (common site) |
| `crates/k_nano/src/interrupts.rs` | `on_timer_irq()` no `timer_handler` |
| `crates/k_nano/src/smp/ap_work.rs` | `note_core_progress()` no topo do `ap_idle_loop` |
| `crates/neural-kernel/src/main.rs` | `note_core_progress()` no heartbeat pós-tick (bin) |

## 3. Validação

- **Testes:** k-nano **249/249** `-t1` (244 + 5 novos: `idade_nunca_negativa_e_na_quando_sem_amostra`,
  `sink_capa_e_trunca_sem_panic`, `sink_num_formata`, `decide_pura_threshold_e_rate_limit`,
  `desarmado_sem_log_nao_dumpan`).
- **Build:** `cargo nk` (touch main.rs) — **0 erros** (3m14s; warnings = Known Warnings).
  `cargo build --release -p boot` + `python tools/build_image.py` (lição s431: cargo nk
  NÃO regenera uefi.img).
- **Strings de código ativo:** `[SILENCE]`, `log parado ha`, `| irq:`, `| prog:`, `dump#`
  — todas presentes no `target/uefi.img`. No ELF o literal aparece como imediato `mov rax`
  (compilador divide o literal — count byte-a-byte deu 0; a prova canônica é o uefi.img).
- **QEMU 8GB/8c WHPX (log 110255):** boot saudável, sistema vivo até **T+83156** (~20min
  de runtime), zero `[SILENCE]` (sem falso-positivo), zero OOM. **O stall do lab s435
  (T+23334, 2/2 boots) NÃO reproduziu nesta run** — passou 3,5× o ponto anterior.
  O disparo real do `[SILENCE]` continua pendente de um stall ao vivo (residual).

## 4. Lições

- **`10_000_000 < 10_000_000` é falso:** o rate-limit "a cada 10s" libera EXATAMENTE aos
  10s — o teste que esperava `None` aos 10s exatos era o bug, não a lógica.
- **Literal longo em release vira imediatos:** `[SILENCE]` entra no ELF via `mov rax, imm64`
  dividido em pedaços — procurar a string inteira no ELF dá falso negativo; provar no
  uefi.img (ou aceitar que o literal fica em imediatos na .text).
- **Stamp por core sem ler GS no IRQ:** IRQs roteiam ao BSP; ler `gs:[8]` no handler antes
  do PerCpu pronto = risco de #PF em IRQ. Solução: stamp fixo em c0 no IRQ + stamps de
  progresso fora do IRQ com guard "1º timer IRQ já ocorreu".

## 5. Residual / próximos passos (s436 continua)

- [ ] Bughunt do stall pós-a2_proof-done (item TODO): carimbar enter/exit de
  `JarbasAgent::tick` + slog no path do eco `[JARBAS]`; replicar sem modelo (a2_proof-only).
- [ ] Capturar um `[SILENCE]` real ao vivo (o stall não reproduziu nesta run — quando
  reproduzir, o dump entrega `prog: cN=` do core preso de graça).
- [ ] Opcional: `EXTRA_STAMP_FN` (padrão HEARTBEAT_FB_FN) p/ o bin acrescentar job infer
  ativo/agente corrente na linha `[SILENCE]` (hoje o dump é 100% self-contained k_nano).
