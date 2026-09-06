# HW DIAG s316 — Instrumentação do freeze metal (Alienware)

**Fonte:** ora-1 (diagnóstico ranked) · **Base:** SESSION_313/314/315
**Sintoma:** desktop chega ao 1º frame e congela (orb/relógio/mouse); `BOOT.LOG`
placeholder; `NSGDB.BIN` zerado; runtime/IRQs/APs vivos em background.

## Instrumentação incluída (s316)

| Item | Arquivo | O que faz |
|------|---------|-----------|
| Watchdog de tick | `agent-core/src/lib.rs` | Mede ms em torno de cada `agent.tick()`; > `TICK_WATCHDOG_MS` (500ms) → reporta |
| Report no kernel | `neural-kernel/src/main.rs` | `[Sched] [warn] "tick lento: <agent> levou N ms"` |
| TSC visível | `k_nano/src/tsc.rs` | Calibração HPET/PIT = `ok`; estimativa CPUID = `warn` (antes: trace mudo) |
| TIMEOUT com ms | `k_nano/src/xhci/bringup.rs` | `cmd TIMEOUT ms=N edq=… evt=…` |

Custo: 2 `rdtsc` + 2 divisões por agent tick — desprezível. Sem registro dos
hooks, watchdog = no-op (branch previsível).

## Checklist de captura (próximo boot no metal)

1. Gravar imagem com a instrumentação (`cargo build --release` + `build_image.py --hw --unified`).
2. Capturar serial COM1 em arquivo (TTL adapter) — **as linhas-chave agora são
   `ok`/`warn`, visíveis sem `boot-trace`**. Sem serial: FB mostra `warn` também.
3. Após o freeze, copiar `E:\BOOT.LOG` e `E:\NSGDB.BIN` (placeholder/zerado = prova).

## Greps decisivos (serial ou FB)

| Grep | Significado | Hipótese |
|------|-------------|----------|
| `tick lento: <agent> levou N ms` | Qual agente bloqueou o scheduler | H1/H2/H6 |
| `cmd TIMEOUT ms=N … evt=0x0 0x0 0x0 0x0` | Event ring zerado/corrompido | H3 (DMA/frame allocator) |
| `cmd TIMEOUT ms=N … evt≠0` (bounded) | Eventos não chegam, wait expirou | H1 (xHCI contract) |
| Sem `cmd TIMEOUT` + hang dentro de wait USB | Wait nunca expira | H2 (TSC congelado/garbage) |
| `tsc_hz=… Hz via cpuid (estimativa)` [warn] | Budgets derivam de estimativa | H2 |
| `TSC calibrado N MHz via HPET/PIT` [ok] | TSC sã — H2 descartado | — |
| `MSC bringup OK port=…` | Enumeração OK → falha é FAT | H4 |
| `MSC bringup FAIL em todas as portas CCS` | Enumeração falhou | H1 |
| `Bulk timeout (demais omitidos)` | BOT transfers expirando | H1 |
| `flush BOOT.LOG ok=…` | Persistência OK (ainda nunca visto) | — |
| `flush FALHOU - … sem particao FAT32` | MSC OK mas FAT/partição falha | H4 |
| `[MEM] RAM detectada / frame allocator reserva` | Comparar reservas vs watermark 16GB | H3 |

## Árvore de decisão

1. `tick lento:` aponta o agente → o bloqueio está lá (input/USB = H1/H2).
2. Sem `tick lento` + `cmd TIMEOUT ms=N` → **H1**: wait bounded expirou, eventos
   xHCI não são entregues no metal (contract: Interrupter/IOC/USBLEGSUP).
3. `cmd TIMEOUT … evt=0x0×4` → **H3**: DMA/frame allocator sobrescreveu o ring
   (residual SESSION_252) — comparar reservas de memória no log.
4. `tsc_hz … cpuid [warn]` ou hang sem timeout → **H2**: calibração TSC ruim.
5. `MSC bringup OK` + `flush FALHOU` → **H4**: partição FAT/BPB no stick
   (skip exFAT; GPT CRC fail → NoFatParts → `mark_skip` sem retry).
6. Tela estática mas `[DSP_TICK]`/`[MOUSE] CLICK` avançando → **H5**: FB UC
   lento no metal (PCIe), render roda sem pintar.

## QEMU não reproduz

Contract xHCI real (interrupter, handoff, scratchpads), calibração TSC real,
watermark do allocator com 16GB, FB UC via PCIe, timing de sticks reais.
Falhou 4 fixes de enumeração MSC (`3771456`→`80c7d78`) — nenhum tocava no
event-delivery contract ou nos budgets derivados de TSC, onde vivem H1/H2.
