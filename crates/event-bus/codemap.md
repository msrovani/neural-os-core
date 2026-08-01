# crates/event-bus/

## Responsibility

No_std inter-agent communication backbone: topic pub/sub (`EventBus`), point-to-point mailboxes (`MessageBus`), a bounded channel, capability tokens, a parallel latent-vector bus, and dedup. Boot phases and runtime agents coordinate exclusively through the topics it carries. 8 source files: `bus.rs`, `event.rs`, `channel.rs`, `capability.rs`, `message_bus.rs`, `latent.rs`, `dedup.rs`, `lib.rs`.

## Design

- **`EventBus` / `Receiver`** (`bus.rs`): topic → `Vec<Arc<TicketLock<VecDeque<Event>>>>`; `subscribe(topic) -> Receiver` (fan-out), `publish(Event)` assigns a monotonic `id` and rejects events whose `CapabilityToken` is invalid. `Receiver::try_receive`/`has_pending` drain per tick.
- **`Event`** (`event.rs`): `{ id: u64, topic: String, payload: Vec<u8>, token: CapabilityToken }`.
- **`CapabilityToken`** (`capability.rs`): `Legacy(u64)` (valid iff > 0) or `Ed25519(IdentityPayload { public_key: [u8; 32], signature: [u8; 64] })`; `From<u64>` keeps legacy compat.
- **`BoundedChannel<T>`** (`channel.rs`): SPSC-style mailbox (`TicketLock` + `VecDeque`, fixed capacity, `send` errors `"channel_full"`).
- **`MessageBus`** (`message_bus.rs`): per-`AgentId` mailboxes of `Envelope { from, to, kind, payload, token }`; `open_mailbox`/`send`/`send_to`/`try_recv`/`mailbox_drain`/`pending`; token 0 is denied. `self_test()` (cortex→rustcoder ping/pong) exported as `message_bus_self_test`.
- **`LatentBus`** (`latent.rs`): parallel channel for LLM hidden states `[f16; 256]` (`LATENT_DIM`, `TOPIC_THOUGHT_LLM`); `LatentPacket { vec: [F16Bits; 256], norm_bits }`; publish/recv counters for stats (ADR-0047 Pilar 1).
- **`DedupWindow`** (`dedup.rs`): FNV-1a hash ring — window of 300 ticks, max 64 hashes — for duplicate-event suppression.

## Flow

Producers (drivers, kernel phases, agents) `publish` Events on named topics; consumers that `Listen`/`Router` on a topic subscribe at init and drain `try_receive()` each scheduler tick. Every publish and point-to-point `send` is gated by a capability token.

## Integration

Depends on `ticket-lock` (+ `libm`); `skill-registry` depends on it for `CapabilityToken`. Canonical topics used across the workspace: `BOOT_PHASE` (neural-kernel `TOPIC_BOOT_PHASE` — boot-phase announcements), `P2P_PACKET` (k_nano `net/mesh.rs TOPIC_P2P_PACKET` — non-heartbeat mesh traffic consumed by hermes `skill_sync`/`skill_marketplace`), `HEALTH_ISSUE` (self-heal pipeline — published by `k_ai` self-heal, subscribed by hermes/neural-kernel agents), `AUDIO_IN` (jarbas `TOPIC_AUDIO_IN`; published by k_hal `audio/hda.rs`), `CARD_ACTION` (jarbas UI card actions, subscribed by k_ai `feedback_agent`), `THOUGHT_LLM` (LatentBus).
