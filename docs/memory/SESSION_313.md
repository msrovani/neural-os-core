# SESSION 313 — xHCI metal + UI liveness

**Data:** 2026-09-05  
**Escopo:** persistência `BOOT.LOG`/`NSGDB.BIN` no pendrive e freeze do desktop no Alienware.

## Evidência

Três boots no metal chegaram ao desktop, mas o volume `E:` manteve:

- `BOOT.LOG` placeholder;
- `NSGDB.BIN` zerado;
- orb, relógio e mouse congelados após o primeiro frame;
- runtime/IRQs/APs ainda ativos em background.

Multi-porta, multi-xHCI e endpoint dinâmico (`3771456` → `494b965`) não
resolveram. Isso descartou “stick não é a primeira porta/EP1” como causa única.

## Causas encontradas

### 1. Event Ring programado 0x20 bytes antes

O Interrupter Register Set 0 começa em `RTSOFF + 0x20`. O driver gravava
`ERSTSZ/ERSTBA/ERDP` diretamente em `RTSOFF + 0x08/+0x10/+0x18`, isto é, na
área reservada após `MFINDEX`. Em silício, Command/Transfer Events não tinham
anel válido; `Enable Slot`, `Address Device` e BOT não podiam completar.

### 2. Normal TRB inválido e sucesso rejeitado

`bulk_transfer` colocava `IOC` no DWORD 2. Nesse DWORD o bit 5 pertence ao
`TRB Transfer Length`, portanto cada transferência anunciava 32 bytes extras.
O IOC correto é o bit 5 do DWORD 3. Além disso, o código aceitava completion
code `0`, mas xHCI define `Success=1` e `Short Packet=13`.

### 3. Bring-up dependia de tolerâncias do QEMU

Faltavam requisitos de takeover real:

- PCI Memory Space + Bus Master;
- UEFI/BIOS ownership handoff via USB Legacy Support Capability;
- `HCCPARAMS1.CSZ` (contextos de 32 **ou 64** bytes);
- Scratchpad Buffer Array em `DCBAA[0]`;
- validação de `PAGESIZE`, halt/reset/CNR/run;
- Warm Port Reset (`WPR/WRC`) para SuperSpeed/CAS;
- acknowledge de `ERDP.EHB`.

Referências: [xHCI 1.2b](https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf),
[Linux xhci-caps.h](https://github.com/torvalds/linux/blob/111e7b23/drivers/usb/host/xhci-caps.h),
[Linux xhci-port.h](https://github.com/torvalds/linux/blob/111e7b23/drivers/usb/host/xhci-port.h).

### 4. Freeze: I/O síncrono dentro do scheduler

`SysInfoAgent` repetia enumeração xHCI/BOT depois do desktop. Um timeout dentro
de `Agent::tick` bloqueia o scheduler cooperativo; Display e Input param, mas
IRQs e APs continuam — exatamente o sintoma observado.

Há um segundo risco: `smp-runqueue` executa ticks em AP sob o lock global
`AGENT_TICK_BUSY`. Um tick longo de Cortex/Hermes pode impedir o BSP de entrar
no tick do Display. O offload de **ticks de agents** fica gated até existir
isolamento por-agent; kernels de compute nos APs permanecem ativos.

Por fim, animação não deve depender de `TIMER_TICKS`: o Display usa o tick do
scheduler para orb/cursor e mantém o timer somente como fonte do relógio.

## Implementação

- `k_nano::xhci`: Event Ring em `RTSOFF+0x20`, TRB/CC corretos, Bus Master,
  firmware handoff, CSZ, scratchpads, PAGESIZE, reset/run com timeout, WPR e EHB.
  O consumer agora respeita o Producer Cycle State e avança eventos em ordem;
  bulk usa timeout TSC de 1s em vez de 80 mil spins dependentes da CPU.
  O handoff também desliga/limpa SMIs legadas e Enable Slot usa o Protocol Slot
  Type da capability correspondente à root port.
- `UsbMassStorage::probe`: proibido após `UI_LIVE`; enumeração é DriverInit.
- `SysInfoAgent`: pós-desktop apenas observa/remonta; não executa probe síncrono.
- `smp::runqueue`: gate honesto para offload de ticks; AP compute preservado.
- `DisplayAgent`: render usa tick do scheduler; dock/clock continua usando timer.
- `poll_mouse`: `try_lock` evita o Display esperar por um HC ocupado.

## Validação

- `cargo check --release`: **PASS, 0 erros**.
- `cargo test -p k-nano --lib`: **193/193 PASS**.
- Testes novos cobrem CSZ 32/64, layout de Normal TRB, CC 1/13 e scratchpads.
- `cargo test -p agent-core --lib`: **1/1 PASS**.
- Testes Jarbas: PASS.

## Gate de metal

Ainda exige novo boot no Alienware. Aceite:

1. desktop continua animado e mouse responde por pelo menos 2 minutos;
2. `E:\BOOT.LOG` começa com BOM + `[S] neural-os-core` e contém checkpoints;
3. `E:\NSGDB.BIN` deixa de ser todo zero e sobrevive ao reboot;
4. se MSC ainda falhar, capturar serial com as linhas `xHCI firmware handoff`,
   `ctx=`, `scratchpads=`, `cmd CC/TIMEOUT` e `Bulk err/TIMEOUT`.
