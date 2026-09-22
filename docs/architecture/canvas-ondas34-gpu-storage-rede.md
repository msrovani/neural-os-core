# Canvas — Ondas 3+4: §3 GPU/§6 Storage + §4 Rede (s397+s398)

> Premissas: fail-closed (DMA/W^X/caps), honestidade (sem stub-teatro), QEMU mascara vs HW, no_std soft-float.

## Onda 3 — §3 GPU + §6 Storage (exp-6)
### HIGH
- **H1** `ahci.rs:255,317` CFL com bit ATAPI=1 em disco ATA → CFL=5 sem A(5) (read) / sem W(6) correto (write).
- **H2** `k_hal/gpu/intel.rs:34,39,49` MI opcodes errados (hex errado): `0x31A00000`/`0x4C000001`/`0x30000000` → `const fn mi(op,len)` + validação i915.
- **H3** `ahci.rs:249,312` PRDT única + `dbc=count*512-1` — DMA não-contíguo/>4MB → loop por páginas ou reject >4KB.
- **H4** `ahci.rs:405`, `nvme.rs:450` alocação DMA por I/O → cache/fixo.
- **H5** `nvme.rs:496` 4Kn write descarta tail > 1 setor → refuse sectors>8 ou RMW em loop 4Kn.
- **H6** `ata.rs:126` IDENTIFY words 117-118 = words/lógico-sector (não bytes) → `bps*2` + validar bit 12.
### MED
M1 vram pow2 rejeita 6/3GB → range 256MB..64GB + decomp binária seed. M2 vram BAR2 hardcoded → de `detect`. M3 firmware/kernel_pack só ATA → `read_root_file_dev`. M4 PCIe ACS dword errado. M5 ring.rs head_reg=0, doorbell noop → fail-closed. M6 sys_installer &'static mut sem lock → closure + double-check. M7 Intel FORCE_WAKEUP como probe → reg ID. M8 firmware blob presença por load → cache dir/nome.
### LOW + simplificações
- Simplificações: AHCI single-page; Intel const fn; NVMe buffer fixo; firmware dir-cache.

## Onda 4 — §4 Rede (exp-7)
### HIGH
- **H1** `k_nano/net/mesh.rs:1362` probe sempre false + hang 176s; `sys_artifacts_degrade` — fix `return true` pós-success + cleanup movido a `p2p_tick`.
- **H2** `k_nano/ipc/mesh.rs:234` RemoteCellChannel é teatro (buffer stack) → deletar (substituir por mesh P2P real).
- **H3** `k_nano/net/transport.rs:289` headers LE → `to_be_bytes` ou dedupe a `udp_broadcast`.
- **H4** `k_hal/net/generic_wifi.rs` DMA UAF + MMIO inventado + send sem init → copiar buf, gate init, fail-closed mapa 0.
- **H5** `hermes/netstack.rs:4` inject_rx_packet no-op → contador DROPPED_INJECT.
### MED
M6 TimerFuture re-enfileira (feito em s396 drain_pending_wakes — confirmar) · M7 CorePairAllocator sem callers → get_timestamp=tsc::now_us ou residual doc · M8 Hamming gated no target (slog) · M9 e1000 facade dupla corrigida · M10 FRAG bounds + stash não-FRACK · M11 TLS smoke circular → 2 blobs · M12 MicroPython hash falso → honest msg / registrar skill.
### LOW
L13 beacon residual · L14 Lamport fetch_max · L15 PACKET_SIZE comment · L16 DNS TXID fixo · L17 browser cache sem TTL · L18 netfs mount false-positive · L19 kick_rx a cada poll · L20 slog unk → Warn rate-limited · L21 fallback wifi sem init · L22 MWAIT fetch_add.

## Upstream Onda 3+4 (lib-4/lib-5)
- virtio-drivers 0.13.0, embedded-tls 0.19, smoltcp 0.14, heapless 0.9.3, ed25519-compact 2.4.2, chacha20 0.11.0, wasmi 2.0.0 todos no latest ✅.
- zerocopy 0.8.55 → 0.8.57 (bump seguro). `pins firmware` por tag (`20260916`) para reprodutibilidade.
- smoltcp `device::TSO` em NetPhy (e1000) + dedupe checksum wire. ADR-0048: Nova=GSP (Pascal=PFIFO-eng-reverse); Xe-only em HW novo = rationale atual correta.
- linux-firmware `intel/iwlwifi` layout mudou (symlinks ainda ok); iwlwifi 77-ucode antiga mas funcional; ath10k estável.
- smoltcp mDNS one-shot (DNS socket) como ideia para brain-mesh DNS; sem mudanças.

## Pós-tarefas por onda
`cargo check --release` 0 errors; canvas registradas; commits + tags s397/s398.
