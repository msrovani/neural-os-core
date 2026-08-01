# crates/k_hal/src/ — k-hal implementation

## Responsibility

Ring 1 (R1) hardware abstraction layer (ADR-0041 §9): device discovery, DeviceCap/
HalOffer capability model, Cap-gated MMIO backends (GPU/audio/net), and VirtIO
transport — "silicon and queues", no persona/LLM/Trust.

**Full crate map:** [`crates/k_hal/codemap.md`](../codemap.md) — responsibility, design
patterns, data/control flow, integration points, and the submodule table.

## Quick Orientation

- **Entry:** `lib.rs::init_h1()` — `discovery::populate_from_pci()` →
  `unlock_dag::boot_platform_tokens()` → `offer::refresh_from_tree()`.
- **Capability model:** `device_cap.rs` → `offer.rs` → `cap_gate.rs` (ring-gated FE
  caps); trust gate in `device_recipe.rs` (ADR-0056).
- **Backends:** `gpu/` (detect→plan→backend→canary golden), `audio/hda.rs` (HDA MMIO),
  `net/` (WiFi MMIO drivers), `virtio.rs` (transport).
- **Memory rule:** use `k_nano::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET}` — never
  `crate::memory::`.
