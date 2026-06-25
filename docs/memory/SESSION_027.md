# Sessão 027 — Sprint 23 Bugfix: e1000 DMA + /ping Command (v0.20.1)

**Data:** 2026-06-25
**Versão:** v0.20.1
**Foco:** Correção de Page Fault no e1000, comando `/ping`, skip DHCP

## Objetivo

Testar a network stack no QEMU com o novo comando `/ping` e corrigir bugs que impediam o boot completo com e1000.

## Dificuldades e Barreiras

### 1. Page Fault INSTRUCTION_FETCH em 0x216299

**Sintoma:** `[SECURITY] Page Fault detectado em VirtAddr(0x216299). Acesso negado. Error code: PageFaultErrorCode(INSTRUCTION_FETCH)`

**Causa raiz:** `allocate_contiguous()` em `memory.rs` iniciava a busca a partir do **bit 0** (endereço físico 0x0). As 66 páginas alocadas pelo driver e1000 durante `init()` caíam nos frames 2-67 (phys 0x2000-0x43000), abaixo de 1 MB. O bootloader (`map_physical_memory`) **não mapeava estas páginas** no offset de memória virtual porque a UEFI não as reporta como `Usable`. Ao acessar os descritores RX/TX ou buffers DMA nesses endereços, o CPU tentava executar código de dados corrompidos → INSTRUCTION_FETCH.

**Correção:** `memory.rs:132` — `allocate_contiguous()` alterado de `let mut i = 0` para `let mut i = self.next_free_bit`, pulando os frames < 1 MB que não são mapeados pelo bootloader. Os descritores agora caem em frames > 256 (phys 0xCE7000+), que estão na região `Usable` do mapa de memória UEFI.

**Lição:** O bootloader v0.9.34 com `map_physical_memory` só mapeia regiões reportadas como `Usable` pela UEFI. Regiões < 1 MB (frames 2-159) são marcadas como livres pelo `BitmapFrameAllocator` para o trampoline SMP, mas não estão no mapa virtual. `allocate_frame()` (usado pelo stress test) começa de `next_free_bit=256` e funciona; `allocate_contiguous()` começava de 0 e quebrava.

### 2. DHCP sem resposta no QEMU TCG

**Sintoma:** DHCP Discover enviado 5 vezes, sem OFFER.

**Causa:** O spin loop de espera (`for _ in 0..20000000 { core::hint::spin_loop() }`)  não dá tempo suficiente para o QEMU processar a resposta do servidor DHCP interno (slirp). O slirp processa pacotes durante o event loop do QEMU, que roda entre instruções TCG. Com TCG puro em loop apertado, I/O não é processado.

**Correção temporária:** DHCP pulado. IP estático `10.0.2.15` + gateway MAC hardcoded `52:54:00:12:34:56` (padrão QEMU slirp).

**Pendência:** Implementar DHCP com timeout baseado em timer ticks (LAPIC timer) e `hlt()` para dar oportunidade ao QEMU processar I/O.

### 3. ARP sem resposta no init_network

**Sintoma:** ARP request para 10.0.2.1 enviado, mas `driver.recv()` retorna None.

**Causa:** Mesmo problema do DHCP — spin loop sem yield. O ARP reply do slirp não chega porque o QEMU não processa a resposta.

**Solução:** Gateway MAC hardcoded para QEMU slirp.

### 4. Dificuldade técnica: QEMU headless no ambiente

**Problema:** O ambiente de desenvolvimento (terminal remoto) não permite `-serial stdio` interativo. Foi necessário usar `-serial file:` e analisar logs a posteriori.

**Solução alternativa:** Logs seriais salvos em arquivo, lidos após boot de 25-45s.

## Correções Aplicadas

| Arquivo | Mudança |
|---|---|
| `src/memory.rs` | `allocate_contiguous()`: `i = 0` → `i = self.next_free_bit` |
| `src/hermes.rs` | +Command::Ping, +parse `/ping <ip>` |
| `src/net.rs` | +`ping()` function, DHCP skip, static IP + hardcoded GW MAC |
| `src/net.rs` | Debug prints removidos após diagnóstico |
| `src/e1000.rs` | Debug prints removidos após diagnóstico |
| `src/proto.rs` | DHCP/DNS funções marcadas `#[allow(dead_code)]` |
| `src/main.rs` | +Command::Ping handler, help atualizado |

## Resultados QEMU

- **Boot completo:** ✅ PCI → ACPI → APIC → SMP → e1000 → Executor (6 tasks)
- **e1000 Init:** ✅ `Init OK. rx_desc=0xce7000 tx_desc=0xce8000`
- **Link:** ✅ UP
- **Executor:** ✅ Watchdog rodando 11000+ ticks estável
- **EchoSkill:** ✅ Executada com sucesso
- **Page Fault anterior:** ❌ Eliminado (fix `allocate_contiguous`)
- **DHCP:** ⚠️ Pulado (pendente refactor com timer-based wait)
- **ARP:** ⚠️ Gateway MAC hardcoded para QEMU slirp
- **cargo check --release:** ✅ 0 erros, 35 warnings (policy)
- **cargo bootimage --release:** ✅ 0 erros

## Pendências

1. Refatorar DHCP com timer ticks em vez de spin loops para QEMU compatibilidade
2. ARP dinâmico com timeout não-bloqueante
3. Teste interativo do `/ping` com `-serial stdio` no terminal local
4. Verificar gateway MAC do QEMU 11.0.50 — validar se 52:54:00:12:34:56 está correto
