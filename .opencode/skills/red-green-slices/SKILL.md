---
name: red-green-slices
description: Test-driven development with a red-green-refactor loop, one vertical slice at a time. Use when the user wants to build a feature or fix a bug test-first, mentions "red-green", "test-first", "write a test", or wants integration tests. Works only where a host-testable seam exists — in this repo, lib crates (cargo test --workspace --exclude neural-kernel --exclude boot), never the bare-metal bins.
---

# Red-Green-Slices

TDD is the red → green loop. This skill makes that loop produce tests worth keeping: what a good test is, where tests go, the anti-patterns, and the rules of the loop. Consult every section before and during the loop, not after.

Start by reading `CONTEXT.md` (if it exists) so test names and interface vocabulary match the project's domain language, and respect ADRs in the area you're touching.

## What a good test is

Tests verify behavior through public interfaces, not implementation details. Code can change entirely; tests shouldn't. A good test reads like a specification — "user can checkout with valid cart" — and survives refactors because it doesn't care about internal structure.

## Seams — where tests go

A **seam** is the public boundary you test at: the interface where you observe behavior without reaching inside.

- **Test only at pre-agreed seams.** Before writing any test, write down the seams under test and confirm them with the user. No test is written at an unconfirmed seam.
- Prefer existing seams over new ones. Use the highest seam possible; the fewer seams across the codebase, the better — the ideal is one.
- Ask: "What's the public interface, and which seams should we test?"

## Anti-patterns

- **Implementation-coupled** — mocks internal collaborators, tests private methods, or verifies through a side channel. The tell: the test breaks when you refactor but behavior hasn't changed.
- **Tautological** — the assertion recomputes the expected value the way the code does (`expect(add(a, b)).toBe(a + b)`), so it passes by construction. Expected values must come from an independent source of truth: a known-good literal, a worked example, the spec, a FIPS vector.
- **Horizontal slicing** — writing all tests first, then all implementation. Bulk tests verify *imagined* behavior and commit you to test structure before you understand the implementation. Work in **vertical slices** instead: one test → one implementation → repeat, each test a tracer bullet that responds to what the last cycle taught you.

## Rules of the loop

- **Red before green.** Write the failing test first, then only enough code to pass it. Don't anticipate future tests or add speculative features.
- **One slice at a time.** One seam, one test, one minimal implementation per cycle.
- **Refactoring is not part of the loop.** It belongs to the review stage (see the `two-axis-review` skill), not the red → green cycle.

## This repo's testing reality

- **Host tests are the only viable TDD target.** `cargo test --workspace --exclude neural-kernel --exclude boot` runs on the host (lib crates are `no_std` only when not building for test). The two bare-metal bins are never host-tested — no red-green loop there; verify via `cargo check --release` + QEMU boot instead.
- **HW-only items are gated with `#[cfg(target_os = "none")]`, NOT `cfg(test)`** — `cfg(test)` is inert in dependency builds and silently compiles HW code into host tests. A host stub for a HW function must be gated on the *target*, e.g. `#[cfg(all(x86_64, not(target_os = "none")))]` for SIMD parity tests.
- **Exported artifacts are the contract.** For model/format code, test the ARTIFACT (the `.bitnet`/`.bin` file) with the Rust-exact loader, never in-memory metrics — an export can be 100% zeros while training metrics pass.
- **Crypto must be validated against FIPS vectors** before trusting it in tests (see the sha256 padding lesson in AGENTS.md).
- **Golden files** used by `include_bytes!` are swallowed by `.gitignore` `*.bin` — un-ignore them explicitly (`!tools/golden_*.bin`).
