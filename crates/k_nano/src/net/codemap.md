# crates/k_nano/src/net/ — P2P Mesh Transport (R0)

**Responsibility**: raw-L2/network peer transport for distributed inference — packet
codec, UDP broadcast frames over the real NIC, transport-mode selection, and cluster
mesh logic (discovery/heartbeat/election/roles). `mod.rs` re-exports `nic_globals` and
the submodules.

**Key symbols**: `noproto::{AiosTaskPacket, NoProtoParser, AIOS_MAGIC, PACKET_HEADER_SIZE,
PacketFlags}`; `udp_broadcast::{udp_broadcast_send/recv, build_udp_broadcast_frame,
serialize/parse, sign_packet/verify_packet, k_nano_tx/rx_count}` (VirtIO→E1000→RTL8139
fallback, port 42069); `transport::{HybridTransport, TransportConfig, TransportMode}`;
`mesh::{init, mesh_tick, p2p_tick, TOPIC_P2P_PACKET, NodeCapabilities, NodeRole}`.

**Integration**: hermes consumes `TOPIC_P2P_PACKET` EventBus events (`skill_marketplace`,
`skill_sync`) and `noproto`/`udp_broadcast` directly; `p2p_tick` runs from bin and reads
`nic_globals::NET_CONFIG` (MAC/IP synced by `set_nic_config` post-IP). The smoltcp stack
is NOT here — it lives in hermes/bin; k_nano only provides NIC handles + raw frames.
