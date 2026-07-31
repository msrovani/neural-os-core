# SESSION_234 — P2P Mesh real entre instâncias QEMU + Migração transporte→k_nano (ADR-0081)

**Data:** 2026-07-31
**Sprint:** v1.9.9 TEST (residuals + mesh P2P)
**Objetivo:** Ativar skill_sync (Fase B) + marketplace P2P; migrar transporte+serviço mesh do bin para k_nano (R0).

---

## Resumo da conquista

**Dois kernels AIOS (10.0.3.2 e 10.0.3.3) se descobriram e trocaram skills via rede real (e1000 + broadcast UDP 42069).** Primeira vez que duas instâncias independentes do OS formam mesh e sincronizam skills entre si.

```
[B] MESH_ENGINE inicializado (ADR-0081)          ← k_nano (R0) lazy-init
[B] TX heartbeat node=4 t=4676 sent=true          ← broadcast UDP real
[A] RX source_id=0 clock=4796                     ← A recebeu o heartbeat de B (cruzado!)
[A] Master: push skill='audio_get_settings' broadcast=true   ← 15 skills empurradas
[B] Worker: skill 'audio_get_settings' ja existe  ← Sync aplicado via poll_p2p (handler OK)
```

## Problemas encontrados e corrigidos

| # | Problema | Fix |
|---|---|---|
| 1 | `run-qemu-p2p-mesh.ps1` com UTF-8 sem BOM → PS 5.1 parse quebrava (linha 28/128/144) | Script reescrito **ASCII puro** (PS 5.1 lê sem BOM como ANSI) |
| 2 | `$Root = Split-Path -Parent $PSScriptRoot` pegava o PAI (`C:\DEV`) | `$Root = $PSScriptRoot` (script está na raiz do repo) |
| 3 | OVMF com espaço em `C:\Program Files\...` quebrava no `-drive` via Start-Process | Caminho curto 8.3 `C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd` |
| 4 | `-smp 1` deixava o boot TCG ~4x mais lento (single-thread) | Reverter para `-smp 2` (MTTCG) — wake de AP é flaky mas raro; retry resolve |
| 5 | Boot travava na leitura FAT32 dos modelos (202MB via ATA PIO sob TCG) | Switch `-NoDisk` no script (teste P2P é rede pura) |
| 6 | `param()` no meio do script (PS exige no topo) | Movido para após o bloco `<# #>` de comentário |

## Migração arquitetural (decisão do oracle)

**Pergunta do maintainer:** "O mesh P2P é uma camada baixa que o sistema inteiro usa, diferente do marketplace (alta) e skill_sync (média)?"

**Veredicto do oracle (confirmado pelo código):** intuição correta.
- **Mesh P2P = R0 (k_nano):** transporte NIC + serviço mesh + protocolo. k_nano já tem smoltcp, e1000, nic_globals — dono natural. k_hal é DeviceCap/MMIO BE, não lugar de transporte.
- **skill_sync = R3 (hermes), NÃO k_ai:** sync é sobre skills (SkillRegistry); k_ai criaria inversão k_ai→hermes.
- **marketplace = R3 (hermes) + jarbas (UI/HITL):** correto como está.
- **net_bridge P2P deve sumir:** transporte em k_nano → hermes chama k_nano direto; não-heartbeat roteado via **EventBus** (k_nano não conhece hermes).

### Migração executada (commit `0eec18f`, 16 arquivos)

1. `k_nano/nic_globals.rs`: `NicConfig` ganhou `ip` + `set_nic_config(mac, ip)`.
2. `k_nano/net/udp_broadcast.rs`: `build_udp_broadcast_frame`/`send`/`recv` movidos do bin (~100 LOC, frame Ethernet+IP+UDP manual; NIC via nic_globals VIRTIO→E1000→RTL8139; contadores locais).
3. `k_nano/net/mesh.rs`: `p2p_tick()` movido do bin (heartbeat ~110 ticks via TIMER_TICKS, RX drain 42069, **não-heartbeat → EVENT_BUS topic `P2P_PACKET`**).
4. `bin/net.rs`: statics `RTL8139/E1000/VIRTIO_DEV` → `pub use k_nano::nic_globals::...` (transporte R0 usa o MESMO NIC; I225/NETSTACK/NET_CONFIG do bin ficam).
5. `bin/netstack.rs`: 3 fns de broadcast removidas; `set_static_ip` chama `k_nano::net::set_nic_config(mac, ip)` pós-config.
6. `bin/network_agent.rs`: `mesh_p2p_tick` removida; DHCP success chama set_nic_config.
7. `bin/bei_init.rs`: `bei_tick` → `k_nano::net::mesh::p2p_tick` + ativa skill_sync/marketplace por `node_count>=1` + `poll_p2p` + `sync_skills` + `marketplace_tick` (bin orquestra k_nano↔hermes).
8. `hermes/net_bridge.rs`: removidas UdpBroadcastSendFn/RecvFn (HTTP/TCP/DNS permanecem — NETSTACK smoltcp fica no bin).
9. `hermes/skill_sync.rs` + `skill_marketplace.rs`: TX via k_nano direto; RX via `subscribe_p2p()`/`poll_p2p()` no EventBus (gate Sync/ModelUpdate, self-activate).

## Lições aprendidas

- **`set_nic_config` NÃO deve rodar no driver-init** — só pós-configuração (set_static_ip/DHCP). Rodar no driver-init faz o transporte enviar heartbeats em modo sandbox sem NIC. Gate `ready` = `MAC != [0;6]`; sem setter pós-config = zero TX; com = TX imediato.
- **Fato decisivo:** `k_nano/Cargo.toml` já tinha smoltcp + e1000 completo + nic_globals — a migração foi ~100 LOC movidos, sem novas deps.
- **EventBus é compartilhado** entre k_nano e hermes (`hermes::globals` faz `pub use k_nano::{EVENT_BUS, SKILL_REGISTRY}`) — roteamento R0→R3 sem inversão de dependência.
- **`hermes::net` tem statics próprios E1000/NETSTACK que NÃO são os usados** (espelhos mortos) — armadilha para quem tocar no static errado. Limpeza pendente (oracle: remover espelhos + p2p_sim driftado).
- **Debug no_std:** logs de diagnóstico com nível "debug" podem ser filtrados — usar "info" quando precisar ver no serial.

## Estado do mesh (conhecido)

- **`nodes=1` persiste na eleição:** cada nó usa `local_role() as u8` como node_id → colisão (ambas as instâncias geram o mesmo ID) → cada uma se vê Master com 1 nó. Heartbeats cruzam (RX source_id visto) mas `add_or_update_node` colide.
- **Next:** derivar node_id do MAC/IP real (ex.: último octeto 10.0.3.2→2, .3→3) para convergir para Master+Worker.

## Verificação

- `cargo check --release -p neural-kernel`: **0 erros** (após cada edição).
- QEMU 2 instâncias (`-NoDisk`, 8G, OVMF, socket listen/connect): TX heartbeat ambas, RX cruzado, Master push 15 skills, Worker apply (handler OK).
- Commits: `0eec18f` (migração + fix .bss.heap SESSION_233 + script corrigido), `f240fa4` (Fase A discovery).
