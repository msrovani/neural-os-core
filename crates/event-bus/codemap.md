# crates/event-bus/

## Responsibility

No_std inter-agent communication backbone: topic pub/sub (`EventBus`), point-to-point mailboxes (`MessageBus`), bounded channel, capability tokens, and parallel latent-vector bus. Boot phases and runtime agents coordinate through the topics it carries.

## Design

- **`EventBus` / `Receiver`** (`bus.rs`): topic → per-subscriber `Arc<TicketLock<VecDeque<Event>>>`.
  - **Bounded** queues: `DEFAULT_QUEUE_DEPTH=64`, stream topics `AUDIO_*` → `STREAM_QUEUE_DEPTH=8`, policy **drop_oldest**.
  - **Zombie prune**: dropped `Receiver` ⇒ `Arc::strong_count==1` skipped/removed on publish.
  - **`publish` → `Result<usize, _>`**: Ok(n)=delivered; Ok(0)=no listeners; Err=invalid token.
  - **`unsubscribe`**: explicit lifecycle (also used after ADR-0047 smoke).
- **`Event`**: `{ id, topic, payload, token }`.
- **`CapabilityToken`**: `Legacy(u64)` valid iff >0; `Ed25519` always `is_valid=false` and **`as_legacy()→0`** (never impersonate token 1).
- **`BoundedChannel<T>`**: fixed capacity; `capacity==0` → send `Err("capacity_zero")`; Clone shares Arc (shared mailbox, not exclusive SPSC).
- **`MessageBus`**: per-`AgentId` mailboxes; token 0 denied; `self_test` ping/pong.
- **`LatentBus`**: `[f16;256]` parallel channel; `recv_count` increments only on `try_receive` (not enqueue).

## Flow

Producers `publish` Events; consumers subscribe at init and drain `try_receive()` each tick. High-rate PCM stays on EventBus only with stream caps — prefer dedicated rings for new audio paths (IDEA #562).

## Integration

Depends on **ticket-lock only** (libm removed s375). Canonical topics: `BOOT_PHASE`, `P2P_PACKET`, `HEALTH_ISSUE`, `AUDIO_IN`/`AUDIO_FRAME`/`AUDIO_OUT` (stream-capped), `CARD_ACTION`, `THOUGHT_LLM` (LatentBus).
