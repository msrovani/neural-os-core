# SESSION_271 — DeviceTree + plano k_ai no boot (AIOS / emagrecer)

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-17  
**Premissas:** ADR-0088 (IA desde o boot; nada bypassado) + emagrecer bin (lógica nas crates)

## Problema

O comentário no `kernel_main` dizia que o Cortex acordava antes do HW para decidir drivers. Na prática:

1. `k_hal::init()` rodava **depois** de NIC/ATA/xHCI/modelos — DeviceTree nascia tarde.
2. DriverInit martelava **E1000 → I225 → RTL8139** sem olhar o PCI (laptop I225 pagava probe e1000 inútil).
3. `BootSelfHealAgent` com `ATA_DRIVER=None` (live USB) fazia inventário **vazio** — skip PCI “honesto” virava **bypass cognitivo** (silício existia no HAL).
4. `populate_from_pci` no segundo `k_hal::init()` fazia `clear_tree()` — um H1 cedo seria apagado.

## Decisão

| Anel | Mudança |
|------|---------|
| `k_hal` | `init_h1()` idempotente (`H1_RAN`); 2º call só `refresh_from_tree`. `DeviceId` guarda `pci_class`/`pci_subclass`. |
| `k_nano` | `boot_bind`: classifica NIC por tabela (não NN), rank I225>VirtIO>e1000>RTL; k_ai instala o plano. |
| `k_ai` | `boot_observe::observe_and_plan()` lê DeviceTree, instala plano, publica `BOOT_OBSERVE`. `HardwareInventory::from_khal()`. |
| `boot` | Comentário: `fat-boot-log` = canal DEV/TEST (SESSION_270). |
| bin | Wire: H1+observe pós-`init_platform_sync`; `probe_nics_from_bind_plan()`; SelfHeal usa a árvore, **sem** rescan PCI. |

Heurística de tabela até o Cortex ter pesos — log honesto: “Cortex sem pesos ainda”. Não fingir LLM no T+0.

## Testes

- `cargo test -p k-nano boot_bind`
- `cargo test -p k_ai boot_observe`
- `cargo check -p neural-kernel --features fat-boot-log`

## Lição

Comentário “o LLM participa das decisões de hardware” sem DeviceTree nem plano = teatro. Observe (H1) → Plan (k_ai rank) → Act (probe só o que existe) → Verify (SelfHeal na mesma evidência).
