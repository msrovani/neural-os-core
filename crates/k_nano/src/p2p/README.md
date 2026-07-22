# P2P Orchestration Module for Neural-OS-Core

This module implements bare-metal peer-to-peer communication between AIOS Master and Nodes using Lamport logical clocks, NoProto zero-copy serialization, and hybrid transport.

## Architecture Overview

The P2P orchestration system provides ultra-direct data exchange between AIOS Master and AIOS Nodes in bare-metal x86_64 environments.

### Components

1. **Logical Clock** (`k-nano::p2p::clock`)
   - Lamport logical clock for event ordering
   - Vector clock for causality tracking across multiple nodes
   - Atomic operations for thread-safe increments
   - No NTP/RTC dependencies

2. **NoProto Parser** (`k-nano::p2p::noproto`)
   - Zero-copy deserialization from network buffers
   - Strict C alignment (`#[repr(C, packed)]`)
   - Slice-overlay directly over network buffer
   - No memory allocation during parsing

3. **Async Runtime** (`k-nano::async_rt`)
   - Lock-free SPSC ring buffer for waker queue
   - APIC timer-based interrupt handling
   - Waker registration and wake notifications
   - Future polling mechanism

4. **NVMe Driver** (`k-nano::storage::nvme`)
   - PCIe MMIO-based NVMe controller initialization
   - Admin Submission/Completion Queue configuration
   - Block read/write operations
   - Raw NVMe command submission

5. **TicKV Integration** (`k-nano::storage::tickv`)
   - Flash Driver trait implementation for NVMe
   - Persistent storage for audit logs
   - Inference result persistence
   - Key-value storage interface

6. **Hybrid Transport** (`k-nano::net::transport`)
   - Raw L2 Ethernet mode (same subnet)
   - UDP/IP mode via smoltcp (routed networks)
   - Automatic transport selection
   - Ethernet/IPv4/UDP header construction

## Usage Examples

### Logical Clock

```rust
use k_nano::p2p::clock::LogicalClock;

let clock = LogicalClock::new();

// Before sending a message
let timestamp = clock.tick();

// After receiving a message
clock.update(received_timestamp);
```

### Vector Clock

```rust
use k_nano::p2p::clock::VectorClock;

let mut vc1 = VectorClock::new(0); // Node 0
let mut vc2 = VectorClock::new(1); // Node 1

vc1.tick(); // Node 0 sends message
vc2.update(&vc1); // Node 1 receives and updates

// Check causality
if vc1.happens_before(&vc2) {
    // vc1 happened before vc2
}
```

### NoProto Parser

```rust
use k_nano::p2p::noproto::{AiosTaskPacket, NoProtoParser, TaskType, PacketFlags};

let packet = AiosTaskPacket::new(
    42,                          // clock
    1,                           // source_id
    2,                           // dest_id
    TaskType::Inference,         // task_type
    255,                         // priority
    1024,                        // tensor_len
    512,                         // param_len
    PacketFlags::new(),          // flags
);

// Serialize header
let mut buffer = [0u8; 2048];
unsafe {
    NoProtoParser::serialize_header(&mut buffer, &packet);
}

// Zero-copy parse
let parsed = unsafe { NoProtoParser::parse(&buffer) };
```

### Async Runtime

```rust
use k_nano::async_rt::{AsyncExecutor, init_async_rt};

// Initialize async runtime
init_async_rt();

let executor = k_nano::async_rt::global_executor();

// Register a future
let waker_idx = executor.register_future(&my_future);

// Wake future from interrupt handler
executor.wake_future(waker_idx);
```

### NVMe Driver

```rust
use k_nano::storage::nvme::NvmeController;

unsafe {
    let mut controller = NvmeController::new(0x40000000); // BAR0 base
    controller.init()?;
    
    let mut buffer = [0u8; 512];
    controller.read_block(0, &mut buffer)?;
    
    controller.write_block(0, &buffer)?;
}
```

### TicKV Storage

```rust
use k_nano::storage::tickv::TicKVStorage;

unsafe {
    let mut storage = TicKVStorage::new(
        &mut controller as *mut NvmeController,
        0x100000,  // base LBA
        0x10000,   // total LBAs
    );
    storage.init()?;
    storage.enable_persistence();
    
    storage.store_inference_result(12345, &result_data)?;
}
```

### Hybrid Transport

```rust
use k_nano::net::transport::{HybridTransport, TransportConfig, TransportMode};

let mut config = TransportConfig::default();
config.mode = TransportMode::RawL2;
config.src_mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
config.dst_mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];

let transport = HybridTransport::new(config);
transport.init()?;

let packet_data = b"AIOS_PACKET";
let mut buffer = [0u8; 2048];
let size = transport.send_packet(packet_data, &mut buffer)?;
```

## Testing

Unit tests are included for each module:

```bash
cargo test -p k-nano
```

### Vector Clock Tests

The vector clock implementation includes comprehensive tests for:
- Concurrent event detection
- Causality relationships
- Clock update semantics
- Multi-node scenarios

## Performance Characteristics

- **Zero-Copy Parsing**: No memory allocation during deserialization
- **Lock-Free Queues**: SPSC ring buffers for waker notifications
- **Atomic Operations**: Lock-free logical clock increments
- **Direct MMIO**: NVMe commands via PCIe MMIO (no DMA overhead for admin queue)

## Safety Considerations

- All unsafe operations are documented with safety invariants
- Buffer bounds checking before pointer operations
- Atomic operations for shared state
- Phase tags for completion queue entries

## Future Enhancements

- DMA support for NVMe I/O queues
- Full TicKV integration (currently simplified interface)
- smoltcp integration for UDP/IP mode
- Multi-queue NVMe support
- Error recovery and retry mechanisms
