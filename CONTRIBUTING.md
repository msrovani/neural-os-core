# Contributing to Neural OS Hermes K³CHJ

**Thank you for wanting to help build the world's first AI-native bare-metal OS.**

Before contributing, please read this guide. It covers the legal and practical aspects of contributing to a project that protects its intellectual property while remaining open source.

---

## 1. Intellectual Property Assignment

By submitting a Pull Request (PR), patch, or any contribution to this repository, you agree to the following terms:

> **All contributions, including but not limited to code, documentation, models, datasets, recipes, and architectural designs, are submitted under the terms of the [AGPL-3.0 License](LICENSE).**
>
> Additionally, you grant the project maintainer an **irrevocable, worldwide, royalty-free license** to:
> - Use, modify, distribute, and sublicense your contribution under any license (including commercial licenses)
> - File patent applications covering your contribution
> - Enforce patent rights arising from your contribution
>
> You represent that you have the right to grant this license and that your contribution is your original work.

**If you cannot agree to these terms, do not submit a PR.** For alternative arrangements, contact `licensing@neural-os.io`.

### Why this clause?
Neural OS contains novel inventions (see [PATENTS.md](docs/community/PATENTS.md)). To protect the project's ability to offer commercial licenses and file patents, all contributions must be assignable. This is standard practice in open-source projects with commercial models (MongoDB, Elastic, MariaDB).

---

## 2. What We Accept

| Type | Status | Notes |
|------|--------|-------|
| **Bug fixes** | ✅ Always | Include test case if possible |
| **Drivers for new HW** | ✅ Welcome | Must follow DriverAgent pattern |
| **Documentation** | ✅ Welcome | Docs, recipes, tutorials |
| **New features** | ⚠️ Discuss first | Open an IDEA or ADR before coding |
| **Models/datasets** | ⚠️ Discuss first | Must include license metadata |
| **New skills** | ✅ Welcome | Follow Skill Manifest format |
| **Patches to vendored code** | ❌ | Submit upstream instead |

---

## 3. Getting Started

```bash
# Prerequisites
rustup toolchain install nightly-2026-07-05 -c rust-src -c llvm-tools-preview
rustup target add x86_64-unknown-none

# Build
cargo build --release

# Verify: 0 errors required
cargo check --release

# Build boot image
python tools/build_image.py

# Run in QEMU (Windows)
.\run-qemu-whpx.ps1 -Window

# Run in QEMU (Linux)
timeout 80 qemu-system-x86_64 -m 6G -smp 4 -accel tcg \
  -drive format=raw,file=target/uefi.img,if=ide,index=0 \
  -drive format=raw,file=target/disk_qemu.raw,if=ide,index=1 \
  -drive if=pflash,format=raw,file=/usr/share/ovmf/OVMF.fd,readonly=on \
  -serial file:logs/boot.txt -display none
```

> **⚠️** On Linux, `rust-toolchain.toml` targets Windows. Use `RUSTUP_TOOLCHAIN=nightly-2026-07-05` before cargo commands.

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
- Run `cargo clean -p neural-kernel` before structural changes (incremental cache hides errors)
- All `unsafe` blocks must have safety comments
- Prefer `slog_nano!` over `println!` for kernel logging

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
- Every commit must pass `cargo check --release`

---

## 6. Licensing Contact

For commercial licenses, OEM agreements, patent licensing, or alternative contribution terms:

📧 `licensing@neural-os.io`

---

*Neural OS is community-owned. Every contribution makes it stronger. Thank you.*
