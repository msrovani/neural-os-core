# Contributing to Neural OS Hermes K³CHJ

Thank you for wanting to help build a bare-metal AI-native operating system.
This guide covers the legal and practical aspects of contributing.

---

## 1. License and Developer Certificate of Origin

By submitting a Pull Request (PR), patch, or any contribution to this
repository, you agree that:

> Your contribution is submitted under the terms of the
> [AGPL-3.0 License](LICENSE) — the same license as the project
> (inbound = outbound).

We also require a **Developer Certificate of Origin (DCO)** sign-off on every
commit, the same convention used by the Linux kernel, CNCF and most large OSS
projects:

> Developer Certificate of Origin, Version 1.1
> (https://developercertificate.org/)
>
> By making a contribution to this project, I certify that:
> (a) The contribution was created in whole or in part by me and I have the
>     right to submit it under the open source license indicated in the file;
> (b) The contribution is based upon previous work that, to the best of my
>     knowledge, is covered under an appropriate open source license and I
>     have the right under that license to submit that work with
>     modifications, whether created in whole or in part by me, under the same
>     open source license; or
> (c) The contribution was provided directly to me by some other person who
>     certified (a) or (b) and I have not modified it;
> (d) I understand and agree that this project and the contribution are
>     public and that a record of the contribution (including all personal
>     information I submit with it, including my sign-off) is maintained
>     indefinitely and may be redistributed consistent with this project or
>     the open source license(s) involved.

**How to sign off:** add `Signed-off-by: Your Name <you@example.com>` to each
commit, or use `git commit -s`. If you cannot agree to the DCO, do not submit
a PR.

For alternative outbound arrangements (e.g. contributing under a dual-license
or commercial agreement), contact `licensing@neural-os.io` before submitting —
we can discuss a separate contributor agreement. We will never block or revoke
an accepted contribution from its AGPL-3.0 terms without a written agreement.

---

## 2. What We Accept

| Type | Status | Notes |
|------|--------|-------|
| **Bug fixes** | Always | Include a test case if possible |
| **Drivers for new HW** | Welcome | Must follow the DriverAgent pattern |
| **Documentation** | Welcome | Docs, recipes, tutorials |
| **New features** | Discuss first | Open an IDEA or ADR before coding |
| **Models/datasets** | Discuss first | Must include license metadata |
| **New skills** | Welcome | Follow the Skill Manifest format |
| **Patches to vendored code** | No | Submit upstream instead |

---

## 3. Getting Started

```bash
# Prerequisites: Rust nightly 1.98 (pinned in rust-toolchain.toml,
# cross-platform), Python 3.10+ (image step), QEMU 8+ (run)
rustup target add x86_64-unknown-none   # auto-installed via rust-toolchain.toml too

# Build + verify (0 errors required)
cargo check --release
cargo build --release

# Host unit tests
cargo test --workspace --exclude neural-kernel --exclude boot

# Generate the FAT32 data disk and run in QEMU (Linux)
python3 tools/build_image.py
timeout 80 qemu-system-x86_64 -m 6G -smp 4 -accel tcg \
  -drive format=raw,file=target/uefi.img,if=ide,index=0 \
  -drive format=raw,file=target/disk_qemu.raw,if=ide,index=1 \
  -drive if=pflash,format=raw,file=/usr/share/ovmf/OVMF.fd,readonly=on \
  -serial file:logs/boot.txt -display none

# Windows: .\run-qemu-whpx.ps1 -Window
```

CI (GitHub Actions) runs `cargo check --release`, the host tests and a QEMU
boot smoke test on every push and PR — you can validate a contribution the same
way locally with the commands above.

---

## 4. Code Standards

### Architecture

- **Everything is an Agent** — no tasks, no services, no standalone drivers
- **No POSIX** — no fork, no signals, no systemd, no containers
- **no_std + no_main** — no libc, no std, no Linux syscalls
- **K³CHJ rings**: k_nano (R0) → k_hal (R1) → cortex/k_ai (R2) → hermes/jarbas (R3)

### Rust

- `cargo check --release`: **0 errors required**
- Dead-code warnings are expected (Known Warnings policy)
- Run `cargo clean -p neural-kernel` before structural changes (incremental
  cache hides errors)
- All `unsafe` blocks must have safety comments
- Prefer `slog_nano!` over `println!` for kernel logging
- New logic goes in the crates; `neural-kernel` stays a thin wire (`pub use`)
- Don't duplicate code across crates — the canonical implementation lives in
  the base crate (see `tools/check_duplication.py`)

### Design

- **YAGNI** — don't add abstractions "for later"
- **Ponytail mode** — mark deliberate simplifications with `// ponytail: reason`
- **Boring over clever** — clever is what someone decodes at 3am

---

## 5. Contribution Workflow

```
1. IDEA (docs/memory/IDEA_BANK.md) → 2. ADR (docs/architecture/) → 3. Sprint → 4. TODO → 5. Code → 6. STATE.md + SESSION
```

- Small fixes go directly to TODO + SESSION
- New features MUST have an ADR before implementation
- Every commit must pass `cargo check --release` and carry a DCO sign-off

---

## 6. Licensing Contact

For commercial licenses, OEM agreements, patent licensing, or alternative
contribution terms:

Email `licensing@neural-os.io`

---

*Neural OS is community-owned. Every contribution makes it stronger. Thank you.*
