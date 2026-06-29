#!/usr/bin/env python3
import tempfile, subprocess, os, shutil
from pathlib import Path

# Build a bootable BIOS image using bootloader 0.11
# Needs nightly Rust (adds rust-toolchain.toml to temp project)

root = Path("C:/dev/neural-os-core")
kernel = root / "target" / "x86_64-unknown-none" / "release" / "neural-kernel"
bios_img = root / "target" / "neural-os-bios.img"

if not kernel.exists():
    print(f"Kernel not found at {kernel}")
    print("Run: cargo build --release")
    exit(1)

tmp = Path(tempfile.mkdtemp(prefix="bt_"))
print(f"Building in {tmp}")

try:
    # rust-toolchain.toml with nightly
    (tmp / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "nightly"\n')
    (tmp / "Cargo.toml").write_text('[package]\nname = "bt"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\nbootloader = "0.11.15"\n')
    (tmp / "src").mkdir()
    # Convert Windows paths to Rust-compatible strings
    k_str = str(kernel).replace("\\", "/")
    o_str = str(bios_img).replace("\\", "/")
    (tmp / "src" / "main.rs").write_text(
        f'fn main() {{\n'
        f'    let k = std::path::PathBuf::from("{k_str}");\n'
        f'    let o = std::path::PathBuf::from("{o_str}");\n'
        f'    println!("Creating BIOS image...");\n'
        f'    bootloader::BiosBoot::new(&k).create_disk_image(&o).unwrap();\n'
        f'    println!("OK! {{}} bytes", std::fs::metadata(&o).map(|m| m.len()).unwrap_or(0));\n'
        f'}}\n'
    )

    result = subprocess.run(["cargo", "build", "--release"], cwd=tmp, capture_output=True, text=True)
    if result.returncode != 0:
        print("STDERR:", result.stderr[:2000])
        exit(1)

    exe = tmp / "target" / "release" / "bt.exe"
    subprocess.run([str(exe)], check=True)
    print(f"BIOS image: {bios_img} ({bios_img.stat().st_size} bytes)")

finally:
    shutil.rmtree(tmp, ignore_errors=True)
