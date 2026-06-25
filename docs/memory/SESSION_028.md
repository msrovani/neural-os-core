# Sessão 028 — Network Sprint: e1000 Debug + RX/TX Diagnosis (v0.20.2)

**Data:** 2026-06-25
**Versão:** v0.20.2
**Foco:** e1000 DMA debug, TDT/TDH protocol fix, RX/TX investigation

## Descobertas Críticas

### 1. e1000 TX não funcionava — TDT == TDH

**Sintoma:** ARP requests enviados mas TPT=0. O e1000 nunca incrementava o contador de pacotes transmitidos.

**Causa raiz:** O registrador TDT (Transmit Descriptor Tail) era escrito com `idx` (o índice do descritor atual). Após init, TDH=0 e TDT=0 → TDT == TDH → o hardware interpreta como "anel vazio" e não processa nenhum descritor.

**Correção:** `self.w32(REG_TDT, ((idx + 1) % NUM_DESC) as u32)` — TDT sempre aponta para o próximo descritor após o que preparamos. Assim TDT != TDH e o hardware processa.

### 2. Número mínimo de descritores: 48

**Sintoma:** RX nunca recebia pacotes.

**Causa:** Usávamos NUM_DESC=32, mas o 82540EM (QEMU device 0x100E) requer no mínimo 48 descritores RX (documentado no driver Linux e1000).

**Correção:** `const NUM_DESC: usize = 48;`

### 3. PTHRESH do RXDCTL = 8

**Sintoma:** RX nunca recebia pacotes mesmo com NUM_DESC=48.

**Causa:** RXDCTL com PTHRESH=0 faz o hardware não fazer prefetch de descritores.

**Correção:** `self.w32(REG_RXDCTL, (8u32 << 16))` — PTHRESH=8 como recomendado pelo driver Linux.

### 4. Ordem de init: RCTL.EN antes de RDT

**Sintoma:** RX não funcionava.

**Causa:** RDT era escrito antes de RCTL.EN. O hardware ignora RDT enquanto RX está desabilitado.

**Correção:** Escrever RCTL com EN=1 primeiro, depois de um pequeno delay, escrever RDT.

### 5. Offsets de estatísticas incorretos

**Sintoma:** TPT e TPR liam 0, dando falsa impressão de que nada era transmitido/recebido.

**Causa:** Usávamos offsets 0x10C0/0x1080 (de outro chip e1000). O 82540EM usa 0x0400C (TPT) e 0x04010 (TPR).

### 6. TX descritor não completava writeback

**Status ATUAL (NÃO RESOLVIDO):** Mesmo com TDT corrigido, o descritor TX é lido pelo hardware (length=42, TDH avança) mas TPT permanece 0. O `qemu_send_packet()` não está sendo chamado ou retorna 0. Suspeita: QEMU TCG não processa o TX completion de forma síncrona durante a escrita MMIO do TDT.

## Arquitetura Neural de Rede (Implementada)

### Hardware Discovery First
`init_driver_network()` — mínimo: detecta e1000, inicia driver, publica `HW_NET_E1000` no EventBus.

### Daemon de Descoberta
`network_bootstrap()` — ARP periódico + timeout via hlt(), IP estático fallback.

### Health Monitoring
`network_health_daemon()` — async, monitora link periodicamente.

### IA Decide
`/ping`, `/fetch`, `/netdiag` roteados pelo MLP para `NetDiagnosticSkill`.

## Debug Methods Adicionados
- `debug_mmio_read(reg)` — le qualquer registrador MMIO
- `debug_rx_desc(idx)` — status de descritor RX
- `debug_tx_desc(idx)` — status de descritor TX

## Próximos Passos (Para Casa)
1. Investigar por que TPT=0 mesmo com TDT corrigido — possível bug no QEMU 11.0.50 e1000
2. Testar com `-device e1000,netdev=net0 -netdev user,id=net0` em vez de `-nic user,model=e1000`
3. Verificar se qemu_send_packet() é chamado (usar `-trace e1000*`)
4. Se TPT continuar 0, testar com modelo RTL8139
5. Testar DHCP com timer-based wait após RX funcionar
