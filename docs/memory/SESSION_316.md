# SESSION_316 — Freeze bisector HW real (s318→s327) + MSC Port Power

**Data:** 2026-09-06/09 · **HW:** Alienware 16c/16GB, pendrive unified · **Branch:** main

## Objetivo
Debug do freeze determinístico em HW real (boot completo → desktop → congela)
+ track MSC (CCS=0 em todas as portas dos 2 xHCI).

## Metodologia — escada de instrumentos FB (bisector)
Cada build adicionou UM instrumento lock-free/volatile no FB real (o serial é
invisível no metal):

| Build | Instrumento | Evidência |
|---|---|---|
| s318 | `diag_mark` 1–8 (barras, canto sup. direito) | ~8 barras = render tick-1 completo |
| s319 | `diag_stamp_agent` (nome do agente, y=14) | agente travado = `cortex_llm` |
| s320 | `diag_stamp_exception` (vermelho y=48, bridge EXC_FB_FN) | SEM exceção = hang, não fault |
| s321 | `tick_stage(n)` (barras y=32, bridge TICK_STAGE_FN) | 2 barras = hang em TRINITY.lock+classify |
| s322 | HDA DMA fix (pool PMM 64KB) | sem mudança = HDA refutado |
| s323 | watchdog pré-lock TRINITY (estampa t/s + degrada) | tick SOBREVIVEU; rodou até tick 1370 e congelou em `network_agent` — 2º hang |
| s324 | cap 64 no poll ZERO-delay + POLL_LOOP_CAP + stages 1–6 no net | freeze idêntico @1370 |
| s325 | imagem completa (todas as sessões) | boot COMPLETO + freeze @1370 em network_agent |
| s326 | heartbeat `T=<hex>` y=80 (timer IRQ) + dígito `S<n>` y=64 + repr(C) TicketLock | discriminador vivo-vs-morto |
| s327 | PP=1 em todas as portas pós-HCRST (estudo Redox) | track MSC |

## Descobertas (lições)
1. **TicketLock sem reentrância**: double-lock no mesmo contexto = self-deadlock
   eterno (serving só avança quando o guard externo cai). IRQ que toque
   TicketLock enquanto thread segura = deadlock IRQ-vs-thread.
2. **lazy_static wrapper ZST**: `&TRINITY` aponta pro wrapper, não pro valor —
   read cru lia estáticas adjacentes (t=0xffffffff807f293d = lixo). Usar `&*TRINITY`.
3. **repr(Rust) reordena campos**: instrumento que lê struct via ponteiro cru
   exige `#[repr(C)]` (TicketLock ticket@0/serving@8).
4. **smoltcp poll() spin**: `loop { poll; if poll_delay()==ZERO continue }` gira
   para sempre quando TX degradado + tráfego LAN real (broadcast/ARP/mDNS).
   QEMU/slirp nunca entrega. Fix: cap 64 + contador POLL_LOOP_CAP.
5. **xHCI HCRST pode deixar PP=0** → CCS lê 0 para sempre → "nenhuma porta
   CCS" no metal (QEMU mantém PP=1 e mascarava). Fix: RMW PP=1 em todas as
   portas pós-run + dump PORTSC pre/pos.
6. **Estudo Redox (lib-1)**: Redox NUNCA seta PP mas VERIFICA (pânica se PP=0
   com CCS=1); PORTSC = RMW preservando flags; USB3 CSC só chega com PED;
   hub port power/reset = class requests ao hub device; USBLEGSUP não fazem
   (nós fazemos — bare-metal); Interrupter Set 0 = RTSOFF+0x20 (SESSION_313 ✓).
7. **Instrumento deve ser validado contra o que lê**: stamp com VA de kernel
   onde esperava contador = lendo struct errada (2 bugs do instrumento pegos
   pelos próprios valores absurdos).

## Estado
- Freeze @ tick 1370 em network_agent AINDA ABERTO (s326 heartbeat/S<n>
  aguardando boot do operador para discriminador vivo-vs-morto)
- MSC: fix PP commitado (511791c4), aguardando boot s327
- Imagens: target/usb_hw_s327.img (kernel F1D886A7…)

## Commits
c798dc72 s318 bisector · 799522f6 s319 agent stamp · 314169fb s320 exc stamp ·
80f61a12 s321 sub-stages · 499a5f28 s322 HDA DMA pool · 17c2f4eb s323 watchdog
TRINITY · 8b47fcc0 s324 poll cap · b684a786 s326 heartbeat+repr(C) ·
511791c4 s327 PP fix · docs: 7ce7045f a1abbbb6 f4ae2500 f05531c1 b2eaa7c5
d5f7bf1c 1200fc9c 01c038ce 63caf5ba
