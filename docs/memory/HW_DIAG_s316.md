# HW DIAG s316 — Instrumentação do freeze metal (Alienware)

**Fonte:** ora-1 (diagnóstico ranked) · **Base:** SESSION_313/314/315
**Sintoma:** desktop chega ao 1º frame e congela (orb/relógio/mouse); `BOOT.LOG`
placeholder; `NSGDB.BIN` zerado; runtime/IRQs/APs vivos em background.

## Instrumentação incluída (s316 + s317)

| Item | Arquivo | O que faz |
|------|---------|-----------|
| Watchdog de tick | `agent-core/src/lib.rs` | Mede ms em torno de cada `agent.tick()`; > `TICK_WATCHDOG_MS` (500ms) → reporta |
| Report no kernel | `neural-kernel/src/main.rs` | `[Sched] [warn] "tick lento: <agent> levou N ms"` |
| TSC visível | `k_nano/src/tsc.rs` | Calibração HPET/PIT = `ok`; estimativa CPUID = `warn` (antes: trace mudo) |
| TIMEOUT com ms | `k_nano/src/xhci/bringup.rs` | `cmd TIMEOUT ms=N edq=… evt=…` |
| **HUD ao vivo (s317)** | `agent-core` + `jarbas/compositor.rs` | HUD pinta `T<renders> IN:<agent> <s>s` — tick em curso + contador, visíveis no FB **depois** do compositor assumir |

## Leitura do HUD no freeze (s317 — foto da tela basta)

A linha ao lado das barras de core, formato `T123 IN:input_agent 4s`:

| O que aparece no frame congelado | Diagnóstico |
|----------------------------------|-------------|
| `IN:<agent> Ns` com N ≥ 3 | **Tick preso** nesse agente — IRQ vivo, scheduler travado nele (H1/H2) |
| `T<n>` com n pequeno (~dezenas) e sem `IN:` | Render morreu cedo — suspeitar P6 demos Ring3 pós-JARVIS (main.rs:3054+) |
| `T<n>` alto e estável entre boots, tela estática | Render rodava e pixels pararam — H5 (FB UC lento no metal) |
| `T<n>` diferente a cada boot no freeze | Morte em ponto variável — correlacionar com o que mudou |

## Bisector de estágio (s318) — barras no canto sup. direito do FB

`diag_mark(n)` escreve **n barras direto no FB real** (volatile, bypass
back/swap). O frame congelado mostra o **último estágio completo** do tick:

| Barras | Estágio completo por último |
|--------|----------------------------|
| 1 | pós `mouse_poll_bytes` + `poll_mouse` |
| 2 | pós drenos EventBus (ui/llm/stt/render) |
| 3 | pós subscribes lazy (LATENT/MESH) + drenos latent/mesh |
| 4 | pós key/echo/mouse/click/drag/install (pré-render) |
| 5 | pós draw_orb_layer |
| 6 | pós HUD (diag line pintada) |
| 7 | pré-present (windows/dock/cursor prontos) |
| 8 | pós present_frame (swap completo) |

Leitura: freeze com **k barras** → o hang está no estágio k+1. Ex.: 4 barras →
travou dentro do render (orb/HUD); 7 barras → travou no present/swap.

## Bisector v2 (s319) — NOME do agente travado (topo-direita, abaixo das barras)

O scheduler agora carimba o **agente em curso direto no FB real** a cada troca
de agente (`diag_stamp_agent`, bridge fn-pointer agent-core→jarbas). O frame
congelado mostra o **AGENTE TRAVADO** — fecha o blind spot da linha `IN:` do
HUD (que só mostra o último PAINT do display, não quem travou depois).

Leitura do frame congelado s319:
- **Barras** (y=0..12): último estágio completo do tick do display
- **Nome** (y=14..30, ciano): agente em curso quando travou
- Ex.: barras=8 + nome=`hermes_agent` → o render do display completou (tick 1)
  e o hang é no tick do Hermes (ou entre ticks — o nome mostra quem estava
  com o tick em curso)

Nota: o "quadriculado azul" 64×64 no canto sup-esquerdo era o patch demo do
P4 (`draw_checker` em jarbas_fb.rs) — **removido no s319** (present escreve
zeros; prova Cap intacta). Não deve mais aparecer.

## Bisector v3 (s320) — EXCEÇÃO estampada no FB (vermelho, y=48, esquerda)

Mecanismo fechado no s319: **exceção durante o tick do agente → dump só na
serial (invisível no metal) → hlt loop = freeze permanente**. O s320 estampa
a exceção DIRETO no FB (`diag_stamp_exception`, bridge k_nano←jarbas):

- **Texto vermelho em y=48** (abaixo do nome do agente): `#UD ip=… err=…`
- #UD = instrução ilegal (AVX/SSE sem CR4 pronto, código corrompido)
- #GP = proteção (segmento/privilegio/MSR)
- #PF = página (CR2 no serial; o ip localiza a instrução)

Leitura do frame congelado s320: **barras** (estágio do display) + **nome
ciano** (agente em curso) + **exceção vermelha** (o motivo). Com os três, o
gatilho é cirúrgico.

## Bisector v4 (s321) — sub-estágios do tick (linha 2 de barras, y=32)

**s320 negativo valioso: SEM exceção estampada = o freeze NÃO é fault — é
hang genuíno** (loop infinito/spinlock) na janela do agente. O s321 adiciona
sub-estágios do tick via bridge agent-core (`tick_stage(n)`):

| Barras linha 2 | Estágio completo por último (CortexAgent::tick) |
|----------------|--------------------------------------------------|
| (vazia) | entrada do tick / try_receive / pós-tick do scheduler |
| 1 | evento LLM_REQUEST recebido |
| 2 | pós EmotionAnalyzer + AFFECT_SNAPSHOT |
| 3 | pós TRINITY classify |
| 4 | pós prompt build (SKILL_STORAGE) |
| 5 | pós recognize |
| 6 | pós generate_via_model |
| 7 | pós publish LLM_RESPONSE |

Leitura: congelado com **k barras na linha 2** → travou no estágio k+1.

## ROOT CAUSE (s322) — DMA do HDA sobre a imagem do kernel

Cadeia de evidência s319→s321: agente `cortex_llm`, **sem exceção** (s320
negativo), hang com **2 barras** (s321) = `TRINITY.lock()` nunca adquire.

Causa raiz: os buffers DMA do HDA estavam em **phys fixos baixos**
(CORB=0x102000, RIRB=0x102800, capture=0x103000, playback=0x104000) —
**dentro da imagem do kernel** (Limine carrega higher-half em phys ~0x100000).
O DMA da saudação TTS escrevia amostras de áudio sobre .text/.data em
execução. Pior: o **BDL apontava para o próprio buffer de amostras** (LVI=0)
— o controlador lia amostras como {ponteiro u64, len} = DMA para endereço
aleatório. Corrupção → palavra da mutex/código vizinho → spinlock eterno.
QEMU mascarava (HDA half-broken, DMA inerte — SESSION_286).

Fix (commit `499a5f28`): pool DMA de 64KB contíguos reservada do PMM
(`k_nano::memory::HDA_DMA_BASE` + `reserve_hda_dma_pool()` no boot), BDL
próprio (ptr u64 + len + flags), capture/playback sem sobreposição, sem pool
= fallback formant honesto.

**s322 refutou o HDA como trigger** (boot idêntico — hash do pendrive
conferido vs build). Hang determinístico em `TRINITY.lock()` (TicketLock —
FIFO por tickets, SEM reentrância: double-lock no mesmo contexto =
self-deadlock eterno).

## Bisector v7 (s326) — heartbeat do timer + dígito de estágio

**Evidência s325 (fotos 06/09 ~23:20):** boot COMPLETO (todas as fases +
saudação "Upload complete. JARVAS online and ready"), freeze
determinístico de novo em **network_agent @ tick 1370** COM o cap do
poll ativo → o freeze não é (só) o loop ZERO-delay. Stamp TRINITY com
`s=` presente mas valores = VAs de kernel → **repr(Rust) reordena
campos** — o read cru pegava bytes do `data`, não os contadores.

s326 (`b684a786`):
- **`#[repr(C)]` no TicketLock** — ticket@0/serving@8 fixos; se o stamp
  continuar mostrando VA = corrupção real
- **Dígito `S<n>` em y=64** — o estágio do tick em curso, legível em foto
- **Heartbeat `T=<hex8>` em y=80** a cada tick do timer IRQ —
  **discriminador vivo-vs-morto**:
  - Tela congelada + **T avançando** = thread girando com IRQ viva
  - T congelado = freeze DENTRO de IRQ (ou IF=0)

Leitura do próximo boot (fotos: canto sup. direito + linha vermelha):
- **S<n>** = estágio exato do freeze no network_agent (3=dentro do
  ns.poll, 5=dentro do dhcp/static, ≤2=lock)
- **T avançando vs congelado** = thread-spin vs IRQ-deadlock
- **TRINITY BUSY t/s pequenos** = lock realmente disputada

## Evidência do boot s316 (Alienware, 2026-09-06)

- **16 CPU cores online, 15780MB RAM** — SMP metal aceso (marco Onda 2).
- `MSC FAIL em todos os xHCI` (00:0d.0 portas=4; 00:14.0 portas=16) — falha de
  **enumeração** (portas CCS/hub), ANTES de comandos: nenhum `cmd TIMEOUT` no FB.
- `E:\BOOT.LOG` placeholder (timestamp FAT 1600 = nunca escrito); `E:\NSGDB.BIN`
  8MB **todo zero** — persistência nunca rodou (coerente com MSC FAIL).
- Freeze do orb logo após `JARVIS online and ready` — sem evidência pós-graphics
  no FB (gap corrigido no s317 pelo HUD ao vivo).

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
