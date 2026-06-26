# Sessão 029 — Sprint 24: smoltcp + e1000 removal + SMP huge page fix

**Data:** 25/06/2026
**Versão:** v0.24.1+build46

## Objetivo
Migrar o agente de rede manual para smoltcp (TCP/IP stack maduro), remover e1000 legado, e corrigir bugs de SMP/APIC.

## Dificuldades e Erros Corrigidos

### 1. smoltcp `Interface::new()` causava lentidão no boot
**Problema:** Criar `NetStack` (com `Interface::new()`) durante o init bloqueante de boot atrasava o SMP e causava timeouts.

**Solução:** Inicialização lazy no agente. O agente cria o `NetStack` apenas quando começa a rodar (primeiro tick), não durante o boot.

### 2. API smoltcp 0.13.1 diferente da documentada
**Problema:** A API do smoltcp mudou significativamente entre versões. Meu código inicial usava `Interface::new(device, config)` mas a API real é `Interface::new(config, &mut device, now)`. `poll()` não retorna `Result` e sim `PollResult`.

**Solução:** Ler o código fonte real (`cargo/registry/src/`) para descobrir a assinatura exata. Usar destructuring `let Self { iface, phy, sockets } = self` para split borrows.

### 3. Non-blocking HTTP API design
**Problema:** A `http_get()` bloqueante travava o executor cooperativo por 200 iterações.

**Solução:** Nova API não-bloqueante: `http_new()` cria socket e retorna `HttpConn`, `http_poll()` avança 1 estado por tick (Connecting → Sending → Receiving → Done/Failed).

### 4. SMP trampoline corrompia dados da BIOS — **BUG CRÍTICO**
**Problema:** O código legado em `smp/mod.rs` fazia:
```rust
let pt_base = (pd0 & 0x000ffffffffff000) + phys_offset;
let pte_ptr = (pt_base + 0x200) as *mut u64;
```
**Assumia** que PD[0] é uma entrada de 4KB. Quando o bootloader usa páginas de 2MB (HUGE_PAGE flag bit 7), `pd0 & 0x000ffffffffff000` aponta para o **endereço físico do dado** (ex: `0x000` para o primeiro 2MB), não para uma tabela de páginas L1. Escrever em `PT[64]` (offset 0x200) corrompia:
- A IVT/BIOS data em `PA 0x200` → APs não bootavam (intermitente)
- Eventualmente, entradas da tabela de páginas do APIC → Page Fault `MALFORMED_TABLE` em ~tick 1600

**Impacto:** Intermitente — às vezes 3 APs bootavam, às vezes 0. Page fault aparecia apenas após ~1600 ticks.

**Solução:** Substituir manipulação raw de PTE por `OffsetPageTable::map_to()`:
```rust
let mut mapper = OffsetPageTable::new(page_table, phys_offset);
mapper.map_to(page, frame, flags, &mut allocator).unwrap().flush();
```
O `map_to()` gerencia HUGE_PAGE/4KB/1GB páginas corretamente, recriando níveis intermediários (splitting) quando necessário.

### 5. proto.rs legado corrompido durante limpeza
**Problema:** Ao remover funções E1000-dependentes do proto.rs, um replace acidental deixou resíduos de sintaxe (`pkt.extend_from_slice` solto no meio do arquivo).

**Solução:** Reescrita completa do proto.rs com apenas os utilitários necessários (eth_header, ip_header, ip_checksum, parse_arp_reply, parse_icmp_reply, parse_http_response).

## Decisões Arquiteturais

1. **smoltcp substitui proto.rs para TCP/IP** — smoltcp gerencia ARP, ICMP, TCP, fragmentação IP. O proto.rs vira só utilitários de construção de pacotes raw.
2. **API não-bloqueante para HTTP** — `http_new() → http_poll() → Done/Failed`. 1 poll por tick do executor.
3. **e1000 removido** — RTL8139 é o único driver de rede (testado, TX funcional).
4. **time_utils** — `datetime()` movido para módulo próprio, disponível globalmente.

## Estado Atual

- `cargo check --release`: 0 erros, ~40 warnings esperados
- QEMU RTL8139: 3 APs online, executor 7 tasks, estável 13.200+ ticks
- smoltcp: poll por tick, HTTP GET google → timeout (NAT slirp esperado)
- SMP: OffsetPageTable::map_to(), sem corrupção de tabela
- Page fault LAPIC: não reproduzido após correção

## Próximos Passos
- Sprint 25: Cortex LLM (BitNet + intent routing neural)
- Smoltcp DNS resolve (UDP socket)
- HTTP para host local (10.0.2.2) para validar TCP end-to-end
