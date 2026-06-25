# ADR-0017: Critical Bugfix Sprint — Code Review & 10 Critical Fixes

**Status:** Accepted (2026-06-24)  
**Context:** Sprint 23 (Network Sprint, pós-MVP) introduced e1000/DHCP/ARP/DNS code with 10 CRITICAL bugs. Comprehensive code review via 5 parallel agents + Context7 docs.

## Decision

Fix all 10 CRITICAL bugs identified in the review before any new development. Document all learnings in `IDEA_BANK.md`.

## 10 Critical Bugs Fixed

| # | Severity | Module | Bug | Fix |
|---|---|---|---|---|
| 1 | CRITICAL | e1000.rs | RCTL/TCTL never programmed — NIC dead | Added `RCTL_EN`, `TCTL_EN` etc. constants + register writes |
| 2 | CRITICAL | e1000.rs | MMIO BAR mask ignored I/O bit — `if/else` on bit 0 | Unconditional `(bar0 & !0xF) as u64` |
| 3 | CRITICAL | proto.rs | DHCP `parse_dhcp_offer`/`parse_dhcp_ack` reject broadcast `FF:FF:FF:FF:FF:FF` dst MAC | Accept broadcast MAC as valid destination |
| 4 | CRITICAL | proto.rs | `dhcp_discover` returns `true` even without ACK | Changed to `return false` on missing ACK |
| 5 | CRITICAL | slab.rs | Off-by-one: `addr + block_size <= zone_end` writes past buffer | Changed to `< zone_end` |
| 6 | CRITICAL | main.rs | `nostack` option on `pushfq; pop` asm — UB (red zone corruption) | Removed `options(nostack)` |
| 7 | CRITICAL | pci.rs | Bridge bus number hardcoded to `bus + 1` — wrong on real hardware | Read secondary bus from config offset `0x19` |
| 8 | CRITICAL | acpi.rs | XSDT stride = 4 bytes, should be 8 — truncates 64-bit pointers | Detect XSDT vs RSDT; use 8-byte stride for XSDT |
| 9 | CRITICAL | mhi.rs | `alloc_by_tier` allocates non-contiguous frames + leak on failure | Use `allocate_contiguous()` first; fallback with cleanup on error |
| 10 | CRITICAL | nn.rs | Bias only applied to first batch row (iterates `0..out_features` not full batch) | Nested loop `for i in 0..batch_size { for j in 0..out_features }` |

## Additional Fixes

- DHCP `xid+1` for REQUEST (violates RFC — same xid must be used)
- DHCP hostname option length `12`→`11` (`b"neural-aios"` is 11 bytes)
- `FrameDeallocator` import added to `mhi.rs`

## Remaining Verification

`cargo check --release`: 0 errors, 27 expected warnings (per dead-code policy).  
QEMU boot: kernel boots, e1000 initializes (Link: UP), but DHCP discover triggers PageFault at `VirtAddr(0x2103b0)` in `e1000::send()` — pre-existing DMA buffer mapping bug exposed after RCTL/TCTL enablement.

## Consequences

- All 10 CRITICAL bugs fixed — system safer and more correct
- e1000 now alive (RCTL/TCTL) but DHCP crashes — needs DMA buffer fix in Sprint 24
- 12 HIGH, 16+ MEDIUM, 12+ LOW items remain for Sprint 24 planning
- Requires `cargo check --release` on every change before commit
