# SESSION_398 — Onda 4: §4 Rede (mesh/P2P/SGDB/TLS/WiFi; loop TECNOLOGIAS)

**Canvas:** `docs/architecture/canvas-ondas34-gpu-storage-rede.md`. Onda 4: §4.1–4.25.

## Fixes aplicados (fixer)
- **H1** mesh probe: `return true` pós-PONG + TTL-cleanup migrado a `p2p_tick` (não mais bloqueio 176s do probe); **H2** RemoteCellChannel deletado (teatro de buffer; transporte real = mesh P2P); **H3** transport.rs headers big-endian corretos; **H4** generic_wifi: tx_buf interno + `initialized` gate + mapa zerado fail-closed; **H5** netstack inject counter DROPPED_INJECT.
- **M7** corepair `get_timestamp=tsc::now_us`; **M9** comentários e1000 (k_nano = true, k_hal=facade); **M10** FRAG bounds + UCAST_STASH; **M11** TLS smoke 2-blobs; **M12** MicroPython honesto (não "resultado") + registro do skill.
- **LOW** Lamport `fetch_max` · DNS TXID `KernelRng` · browser cache TTL 18k ticks · netfs mount LIST probe · slog unk→Warn rate-limited · MWAIT `fetch_add` · wifi driver init Err log.

## Upstream (lib-5)
tudo no latest (smoltcp 0.14, embedded-tls 0.19, ed25519 2.4.2, chacha 0.11, heapless 0.9.3, wasmi 2.0, virtio-drivers 0.13). Ideias: dedupe checksum smoltcp::wire, mDNS oneshot p/ brain-mesh, wetware attenuation de caps.

## Verificação
`cargo check -p k-nano -p hermes -p k-hal -p event-bus --release` 0 erros; `cargo test -p k-nano` 191/0. Sem DUP novo. Commit + tag `v1.9.99-s398`.
