# SESSION_343 — VirtIO Modern Drivers (virtio-drivers crate integration)

**Data:** 2026-09-13
**Sprint:** s343
**Objetivo:** Integrar o crate `virtio-drivers` (rcore-os) no k_nano para transporte moderno VirtIO PCI.

## Contexto

O virtio_net.rs e virtio_blk.rs usavam drivers manuais com transporte legacy (I/O ports).
O crate `virtio-drivers` fornece drivers maduros com transporte moderno (BAR-based common config),
descriptor chain management, e buffer recycling — mas exige implementação de traits `Hal` e
`ConfigurationAccess`.

## Implementação

### 1. `virtio_hal.rs` — Bridge HAL (k_nano ↔ virtio-drivers)
- **`AiosHal`** implementa `Hal`: DMA via GLOBAL_ALLOCATOR, MMIO via `map_page_uc`, share/unshare identity.
- **`AiosPciAccess`** implementa `ConfigurationAccess`: bridge para `read_config_dword`/`write_config_dword` do k_nano.
- **`enumerate_virtio_devices()`**: enumera dispositivos VirtIO via `PciRoot` do crate.
- **`try_create_transport()`**: cria `PciTransport` para um device específico.

### 2. `virtio_modern.rs` — Wrappers modernos
- **`VirtIoNetModern`**: wraps `VirtIONet<AiosHal, PciTransport, 64>` com API `send(&[u8])` / `recv() -> Option<Vec<u8>>`.
- **`VirtIoBlkModern`**: wraps `VirtIOBlk<AiosHal, PciTransport>` implementando `BlockDevice` (read/write_sectors).
- **`MODERN_NET` / `MODERN_BLK`**: globals estáticos para os drivers modernos.
- **`init_driver_virtio_modern()` / `init_driver_virtio_blk_modern()`**: init idempotente.

### 3. Integração no boot path
- `init_driver_virtio()` agora tenta **modern primeiro**, fallback para legacy.
- `init_driver_virtio_blk()` idem.
- `virtio_send()` / `virtio_recv()` funções unificadas: checam modern primeiro, depois legacy.

### 4. Hermes netstack atualizado
- `nic_send()` e `nic_recv()` em `netstack.rs` usam `virtio_send`/`virtio_recv` em vez de `VIRTIO_DEV.lock()`.

## Resultado
- `cargo check --release` — 0 erros (36 warnings pré-existentes)
- `cargo test --workspace` — todos passam (k-nano 192/192, hermes 188/188, jarbas 89/89)

## Notas técnicas
- O crate usa transporte **modern** PCI (BAR-based common config), não legacy I/O ports.
- QEMU default para `virtio-net-pci` é modern (sem `disable-modern=on`).
- `zerocopy` (dependência do crate) compila no toolchain atual (MinGW/resolved).
- O `PciRoot` do crate faz sua própria enumeração PCI — não reusa `scan_pci()` do k_nano (aceitável, happen once at boot).

## Pendências
- **VirtIO-GPU 2D via crate**: o crate tem `VirtIOGpu` com suporte 2D completo (`resource_create_2d`, `transfer_to_host_2d`, `flush`). O `virtio_gpu.rs` atual usa HalOffer path, não o crate. Integrar seria criar wrapper similar ao net/blk.
- **IRQ/MSI-X**: os drivers modernos suportam IRQ via `ack_interrupt()`. Hoje é poll-based. Habilitar IRQ reduziria latência de RX.
