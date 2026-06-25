# Relatório para Continuar em Casa — Network Debug v0.20.2

## Contexto
Estamos depurando o e1000 no neural-os-core. O TX envia pacotes (driver.send() copia dados, seta descritor, escreve TDT) mas o QEMU never incrementa TPT (Total Packets Transmitted). O descritor é lido (length aparece, TDH avança) mas o pacote não é transmitido.

## Ambiente
- QEMU 11.0.50 (v11.0.0-12631-g54e84cdc7a)
- Windows + MinGW
- `-nic user,model=e1000` (82540EM, device 0x100E)
- Boot normal: `cargo bootimage --release && qemu-system-x86_64 -m 2G -serial stdio -nic user,model=e1000 -drive format=raw,file=bootimage-neural-kernel.bin -no-reboot -smp 4 -nographic`

## O que já foi corrigido (Funcionando)

### TX: TDT != TDH (Corrigido)
`send()` em `e1000.rs:251`:
```rust
self.w32(REG_TDT, ((idx + 1) % NUM_DESC) as u32);
```
Antes: `TDT = idx` → TDT == TDH → nada processado
Depois: `TDT = (idx+1) % NUM_DESC` → TDT != TDH → descritor lido

### NUM_DESC: 32 → 48 (Corrigido)
Mínimo para 82540EM é 48 (Linux e1000 driver docs).

### RXDCTL PTHRESH: 0 → 8 (Corrigido)
Linux driver recomenda PTHRESH=8 para prefetch de descritores RX.

### RCTL antes de RDT (Corrigido)
Ordem de init corrigida: RCTL.EN primeiro, delay, RDT depois.

### Offsets TPT/TPR corrigidos (Corrigido)
TPT=0x0400C, TPR=0x04010 (não 0x10C0/0x1080).

## O que NÃO funciona (Precisa Investigar)

### TPT=0 sempre
Mesmo com TDT corrigido, TPT stays 0. Evidências:
- TDH avança (descritor é "processado")
- length no descritor mostra 42 (hardware leu)
- Mas TPT=0 (pacote não enviado)

### Suspeitas
1. **QEMU 11.0.50 bug**: e1000_send() pode não chamar qemu_send_packet()
2. **Cache coherency**: O descritor lido pelo e1000 pode ter dados antigos (cached)
3. **TDT/TDH wrap**: Talvez o (idx+1) % NUM_DESC cause wrap inesperado

### Para testar em casa
1. `QEMU -trace e1000\*` para ver se e1000_send é chamado
2. Testar com modelo RTL8139: `-nic user,model=rtl8139`
3. Testar com `-device e1000,netdev=net0 -netdev user,id=net0` (explícito)
4. Verificar se o buffer_addr no descritor é válido (físico < 4 GB)
5. Adicionar debug no QEMU monitor: `info qtree` para ver status do e1000
6. Testar com smp=1 (evita race conditions do TCG multi-core)

## Arquivos Modificados

### crates/neural-kernel/src/e1000.rs
- NUM_DESC: 32→48
- TDT: `idx` → `(idx+1) % NUM_DESC`
- RXDCTL: PTHRESH 0→8
- Ordem init: RCTL antes de RDT
- Debug methods: debug_mmio_read, debug_rx_desc, debug_tx_desc

### crates/neural-kernel/src/net.rs
- init_driver_network() — mínimo, publica HW_NET_E1000
- network_bootstrap() — ARP periódico com hlt(), IP antes do ARP
- network_health_daemon() — async, monitora link
- ping/http_get — com spin loops (chamados do executor)
- NetConfig com online flag
- Estatísticas TPT/TPR com offsets corretos

### crates/neural-kernel/src/main.rs
- Boot flow: init_driver → bootstrap → executor
- network_health_daemon spawnada
- wait_ticks removido (não mais necessário)

### Outros
- hermes.rs: +Command::Ping
- AGENTS.md, STATE.md, CHANGELOG.md, SESSION_027.md, SESSION_028.md
- IDEA_BANK.md: #250-252 (/ping, DHCP timer, ARP não-bloqueante)

## Build
```powershell
& "C:\Users\CH P2-P3\.cargo\bin\cargo.exe" bootimage --release
```
0 erros, ~35 warnings (expected per policy)

## Teste Rápido
```powershell
qemu-system-x86_64 -m 2G -serial stdio -nic user,model=e1000 -drive format=raw,file=target\x86_64-unknown-none\release\bootimage-neural-kernel.bin -no-reboot -smp 4 -nographic
```
Digite `/netdiag` ou `/ping 10.0.2.2` no console serial.
