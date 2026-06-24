# ADR-0016: Network Strategy for Neural OS Hermes

**Status:** Draft (pré-MVP)  
**Date:** 2026-06-23  
**Driver:** Um AIOS sem rede é inviável como sistema moderno — weight updates, skill distribution, remote inference e knowledge retrieval exigem conectividade. O roadmap atual difere rede para Sprint 24+ (pós-MVP), mas o Hermes Chat (Sprint 20) já expõe a lacuna.

---

## 0. Por que rede é crítica para o AIOS

| Caso de uso | Descrição | Quando precisa existir |
|---|---|---|
| **Weight updates** | Atualizar MLP do intent router, architecture detector, trust cache via rede em vez de recompilar | Sprint 23+ |
| **Skill distribution** | Instalar/atualizar skills de um registry remoto (MCP Hub) | Sprint 23+ |
| **Remote inference** | Fallback quando NPU/CPU local não consegue executar um tensor grande | Sprint 24+ (WASM) |
| **Knowledge retrieval** | Query a bases de conhecimento externas (Wikipedia, docs, etc.) | Sprint 23+ |
| **Telemetry** | Report de health do sistema, crash logs, estatísticas de uptime | Sprint 23+ |
| **Multi-agent** | Agentes Hermes conversando entre dispositivos na mesma rede | Sprint 25+ (WASM) |
| **Model download** | Baixar pesos de modelos neurais (ternários, 2-bit) de um registry | Sprint 23+ |

**Conclusão:** Rede não é um luxo pós-MVP. É um requisito de bloco 4/5 se o Hermes Chat tiver qualquer capacidade de "aprender" ou "buscar informação".

---

## 1. Análise de Viabilidade Técnica

### Stack de rede bare-metal em Rust no_std

| Camada | Solução | Crate | Status no ecossistema |
|---|---|---|---|
| Driver NIC | VirtIO-net (PCI 1AF4:1041) | `virtio-drivers` (rcore-os) | ✅ no_std, PCI MMIO, suporta net |
| TCP/IP | smoltcp v0.13+ | `smoltcp` | ✅ no_std, 0BSD, ~41K SLoC, ARP/IP/TCP/UDP/DNS |
| HTTP cliente | Mínimo (~200 LOC) sobre smoltcp TCP | Custom | Precisamos escrever (ou usar `embedded-nal`) |
| DNS | smoltcp `dns` feature | `smoltcp` | ✅ nativo |
| TLS | `embedded-tls` ou `p256` + AES | `embedded-tls` / `crypto` | ⚠️ Complexo, postergar para pós-MVP |

### Pré-requisitos já atendidos

1. ✅ **PCI scan** (Block 1, Sprint 18) — já detecta `1AF4:1041` (VirtIO-net)
2. ✅ **Heap allocator** (Sprint 4 + Sprint 19) — LockedHeap 3.5 MB + Slab
3. ✅ **APIC + IOAPIC** (Block 1) — interrupções para RX/TX notificações
4. ✅ **SMP** (Block 2) — processamento de packets em cores separados

### Riscos técnicos

| Risco | Mitigação |
|---|---|
| `virtio-drivers` depende de `Hal` trait (PCI ECAM) | Adaptar: nosso PCI é CF8/CFC, não ECAM. Precisamos implementar `virtio-drivers::Hal` sobre nosso PCI |
| `smoltcp` tem 41K SLoC — risco de compile-time ou bugs de integração | Testar incremental: ARP primeiro, depois TCP, depois HTTP |
| Tamanho do kernel pode crescer > 2 MB | Manter smoltcp como crate separada, linkar só features necessárias |
| Sem TLS — tráfego HTTP puro | MVP aceitável para rede local/QEMU. TLS é pós-MVP |

---

## 2. Roadmap Proposto — Fases de Rede

### Fase N1 — VirtIO-net Driver (Sprint 23, pós-MVP Hermes)

Dependências: PCI (Block 1) + MSI/MSI-X ou IOAPIC (Block 1)

- Criar `crates/virtio-net/` — driver VirtIO-net puro sobre PCI BARs
- Implementar `virtio-drivers::Hal` para nosso PCI (CF8/CFC + memory map)
- Virtqueues: RX + TX, driver-init, device-ACK
- Test: ping (ARP reply) from QEMU user-mode network (`-netdev user`)
- Entregável: interface de rede responde a ARP, transmite/recebe frames Ethernet

### Fase N2 — smoltcp TCP/IP Stack (Sprint 23, mesmo bloco)

Dependências: N1, heap allocator

- Integrar `smoltcp = "0.13"` como dependência
- `NetworkInterface` com smoltcp sobre our virtio-net device
- ARP resolution + IPv4 + TCP
- Test: nc (netcat) conecta ao Hermes via QEMU `-nic user,hostfwd=...`
- Entregável: Hermes pode abrir socket TCP, receber conexão

### Fase N3 — DNS + HTTP Client (Sprint 23, mesmo bloco)

Dependências: N2

- DNS resolver via smoltcp `dns` feature
- HTTP GET client mínimo (~200 LOC) sobre smoltcp TCP
- Test: Hermes baixa um arquivo de um servidor HTTP no host
- Entregável: `hermes> /fetch http://host/weights.bin` funciona

### Fase N4 — TLS + HTTPS (Pós-MVP, Sprint 25+)

Dependências: N3, WASM (Sprint 25+)

- `embedded-tls` ou similar para TLS 1.3
- Certificado embutido ou trust-on-first-use
- Entregável: HTTPS para skill updates seguros

---

## 3. Impacto no MVP

O MVP Hermes ISO (Sprint 22) **não terá rede**. O plano é:

| Bloco | Entrega | Sprint | Rede? |
|---|---|---|---|
| 0-3 | Genesis, PCI, SMP, Hermes Chat | 1-20 ✅ | ❌ Offline |
| 4-5 | MLP Architect, Skills, Trust, ISO | 21-22 | ❌ Offline |
| **N1-N3** | **VirtIO-net + smoltcp + HTTP** | **23** | **✅ MVP+1** |
| N4 | TLS/HTTPS + WASM net | 25+ | ✅ Seguro |

Isso significa que Sprint 23 (MVP+1) é o **Network Sprint** — adicionamos um bloco extra após os 6 originais.

---

## 4. Decisões Arquiteturais

### 4.1. VirtIO-net como primeiro driver NIC
**Motivo:** QEMU suporta nativamente. Hardware real (e1000, RTL8139) pode ser adicionado depois. VirtIO tem spec simples, registers MMIO, e ring-based.

### 4.2. smoltcp como stack TCP/IP
**Motivo:** Único crate no_std maduro com ARP/IP/TCP/UDP/DNS. 0BSD license (sem restrições). Usado pelo Redox OS. Sem heap requirement.

### 4.3. HTTP cliente custom sobre smoltcp
**Motivo:** Nenhum crate HTTP no_std leve o suficiente. ~200 LOC para GET request com header parsing mínimo.

### 4.4. Rede como bloco separado (não na chain MVP)
**Motivo:** MVP já tem 6 blocos. Adicionar rede atrasaria o Hermes ISO de Sprint 22 para Sprint 27+. Melhor MVP offline + Network Sprint imediatamente após.

### 4.5. Sem syscalls POSIX
**Motivo:** Não somos Linux. Rede acessada via Hermes command `/fetch` e via SkillRegistry (skills podem solicitar `requires_network: bool` no manifest).

---

## 5. Itens Novos no IDEA_BANK

| # | Item | Status | Target | Depende de |
|---|---|---|---|---|
| 117 | VirtIO-net driver (PCI) sobre `virtio-drivers` crate | 🟡 Sprint 23 | Sprint 23 | PCI scan, IOAPIC, heap |
| 118 | smoltcp TCP/IP stack integration | 🟡 Sprint 23 | Sprint 23 | Heap, VirtIO-net |
| 119 | DNS resolver (smoltcp dns) | 🟡 Sprint 23 | Sprint 23 | smoltcp |
| 120 | HTTP GET/POST client | 🟡 Sprint 23 | Sprint 23 | smoltcp TCP |
| 121 | Hermes `/fetch` command | 🟡 Sprint 23 | Sprint 23 | HTTP client |
| 122 | Skill `requires_network` manifest field | 🟡 Sprint 23 | Sprint 23 | SkillRegistry |
| 123 | TLS 1.3 client (`embedded-tls`) | ⏳ Pós-MVP | Sprint 25+ | WASM, HTTP |
| 124 | Wi-Fi (e1000/RTL8139 para hardware real) | ⏳ Pós-MVP | Sprint 26+ | VirtIO-net driver |

---

## 6. Timeline Atualizada

```
Sprint 21 ─── MLP + MHI (Block 4)              Offline
Sprint 22 ─── Skills + Trust + ISO (Block 5)    Offline
──── MVP HERMES ISO ────
Sprint 23 ─── NET: VirtIO-net + smoltcp + DNS + HTTP
Sprint 24 ─── NVMe + SFS persistente
Sprint 25+ ── WASM + TLS + multi-agent
```

---

## 7. Referências

- `smoltcp` v0.13.1: https://crates.io/crates/smoltcp (0BSD, no_std, 41K SLoC)
- `virtio-drivers` (rcore-os): https://github.com/rcore-os/virtio-drivers (MIT, no_std, suporta net)
- `astra-os` — reference implementation: https://github.com/programmersd21/astra-os (VirtIO-net + smoltcp em kernel bare-metal)
- `embedded-tls` (futuro): https://crates.io/crates/embedded-tls
